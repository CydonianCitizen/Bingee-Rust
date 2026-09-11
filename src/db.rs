//! SQLite persistence for the R2 spike: one `media` table, seeded once from
//! the deterministic R1 dataset, and the SQL version of `library::search`.
//!
//! This is a spike schema, not the Bingee schema, and it has no migrations.
//! After a schema change, delete the spike database file.

use std::path::Path;

use rusqlite::types::Type;
use rusqlite::{Connection, Row, params};

use crate::library::{self, MediaItem, MediaKind, Progress};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS media (
    local_id       INTEGER PRIMARY KEY,
    title          TEXT NOT NULL,
    original_title TEXT NOT NULL,
    year           INTEGER NOT NULL,
    media_type     TEXT NOT NULL CHECK (media_type IN ('Movie', 'TV')),
    progress_state TEXT NOT NULL CHECK (progress_state IN ('planned', 'watched', 'episodes')),
    -- Episodes watched / total; NULL unless progress_state = 'episodes'.
    progress_value INTEGER,
    progress_total INTEGER,
    overview       TEXT NOT NULL,
    -- Lowercased title and original title, computed in Rust: SQLite's lower()
    -- and LIKE fold only ASCII case, while the R1 search folds Unicode case.
    search_text    TEXT NOT NULL
)";

/// Opens the database at `path`, creating it if needed, and seeds it if it is
/// empty. `":memory:"` opens a private in-memory database.
pub fn open(path: &Path) -> rusqlite::Result<Connection> {
    let mut conn = Connection::open(path)?;
    init(&mut conn)?;
    Ok(conn)
}

/// Creates the schema and inserts the 1,000 R1 records if the table is empty.
/// Both happen in one transaction, so a failed seed leaves no partial data,
/// and a seeded database is never seeded again.
fn init(conn: &mut Connection) -> rusqlite::Result<()> {
    let tx = conn.transaction()?;
    tx.execute_batch(SCHEMA)?;
    let count: i64 = tx.query_row("SELECT count(*) FROM media", [], |row| row.get(0))?;
    if count == 0 {
        let mut insert = tx.prepare(
            "INSERT INTO media (local_id, title, original_title, year, media_type,
                 progress_state, progress_value, progress_total, overview, search_text)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        )?;
        for item in library::generate_library() {
            let (state, value, total) = match item.progress {
                Progress::Planned => ("planned", None, None),
                Progress::Watched => ("watched", None, None),
                Progress::Episodes { watched, total } => ("episodes", Some(watched), Some(total)),
            };
            insert.execute(params![
                item.id,
                item.title,
                item.original_title,
                item.year,
                item.kind.label(),
                state,
                value,
                total,
                item.overview,
                item.search_text(),
            ])?;
        }
    }
    tx.commit()
}

/// SQL version of `library::search`: records whose title or original title
/// contains `query`, case-insensitively, in id order. A blank query matches
/// everything. `instr` is a literal substring test, so `%` and `_` in the
/// query are not wildcards.
pub fn search(conn: &Connection, query: &str) -> rusqlite::Result<Vec<MediaItem>> {
    // ponytail: loads every matching record (at most 1,000 here). For much
    // larger libraries, query ids only and load visible rows by id.
    let mut stmt = conn.prepare_cached(
        "SELECT local_id, title, original_title, year, media_type,
                progress_state, progress_value, progress_total, overview
         FROM media
         WHERE instr(search_text, ?1) > 0
         ORDER BY local_id",
    )?;
    let rows = stmt.query_map([query.trim().to_lowercase()], item_from_row)?;
    rows.collect()
}

/// Maps a `search` row back to the domain type. An unknown enum value is an
/// error, not a guess.
fn item_from_row(row: &Row) -> rusqlite::Result<MediaItem> {
    let kind = match row.get_ref(4)?.as_str()? {
        "Movie" => MediaKind::Movie,
        "TV" => MediaKind::Tv,
        other => return Err(unknown_value(4, "media_type", other)),
    };
    let progress = match row.get_ref(5)?.as_str()? {
        "planned" => Progress::Planned,
        "watched" => Progress::Watched,
        "episodes" => Progress::Episodes {
            watched: row.get(6)?,
            total: row.get(7)?,
        },
        other => return Err(unknown_value(5, "progress_state", other)),
    };
    Ok(MediaItem::new(
        row.get(0)?,
        row.get(1)?,
        row.get(2)?,
        row.get(3)?,
        kind,
        progress,
        row.get(8)?,
    ))
}

fn unknown_value(column: usize, name: &str, value: &str) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(
        column,
        Type::Text,
        format!("unknown {name} {value:?}").into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    fn memory_db() -> Connection {
        open(Path::new(":memory:")).unwrap()
    }

    fn count(conn: &Connection) -> i64 {
        conn.query_row("SELECT count(*) FROM media", [], |row| row.get(0))
            .unwrap()
    }

    fn ids(items: &[MediaItem]) -> Vec<u32> {
        items.iter().map(|item| item.id).collect()
    }

    /// A per-test database file in the OS temp dir, deleted on drop. Never the
    /// app's own database.
    struct TempDb(PathBuf);

    impl TempDb {
        fn new(name: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("bingee-{name}-{}.db", std::process::id()));
            let _ = std::fs::remove_file(&path);
            Self(path)
        }
    }

    impl Drop for TempDb {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }

    #[test]
    fn open_creates_schema_and_seeds_exactly_1000_records() {
        let conn = memory_db();
        let tables: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name = 'media'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(tables, 1);
        assert_eq!(count(&conn), 1000);
    }

    #[test]
    fn reopening_keeps_data_without_reseeding() {
        let db = TempDb::new("reopen");
        drop(open(&db.0).unwrap());
        let mut conn = open(&db.0).unwrap();
        assert_eq!(count(&conn), 1000);
        init(&mut conn).unwrap();
        assert_eq!(count(&conn), 1000, "init is idempotent");
        // Persisted records, including stable ids, Movie/TV and every
        // progress variant, round-trip unchanged and in id order.
        assert_eq!(search(&conn, "").unwrap(), library::generate_library());
    }

    #[test]
    fn movie_and_tv_round_trip() {
        let conn = memory_db();
        let all = search(&conn, "").unwrap();
        assert_eq!(
            (all[0].title.as_str(), all[0].kind),
            ("Severance", MediaKind::Tv)
        );
        assert_eq!(
            (all[2].title.as_str(), all[2].kind),
            ("Dune: Part Two", MediaKind::Movie)
        );
        let tv = all.iter().filter(|i| i.kind == MediaKind::Tv).count();
        let expected = library::generate_library()
            .iter()
            .filter(|i| i.kind == MediaKind::Tv)
            .count();
        assert_eq!(tv, expected);
    }

    #[test]
    fn search_cases() {
        let conn = memory_db();
        let found = |query| ids(&search(&conn, query).unwrap());
        assert_eq!(found("").len(), 1000);
        assert_eq!(found("   ").len(), 1000);
        assert_eq!(found("SEVERANCE"), [1], "title, any case");
        assert_eq!(found("dArK"), [2]);
        assert_eq!(found("premier"), [5], "original title");
        assert_eq!(found("shogun"), [6]);
        assert!(found("zzzz").is_empty());
        assert!(found("%").is_empty(), "no LIKE wildcards");
        assert!(found("'; DROP TABLE media; --").is_empty());
        assert_eq!(count(&conn), 1000, "query is a bound parameter");
        let harbors = found("harbor");
        assert!(harbors.len() > 1);
        assert!(harbors.windows(2).all(|w| w[0] < w[1]), "id order");
        assert_eq!(harbors, found("HAFEN"), "Hafen is Harbor's original title");
        assert_eq!(harbors, found("harbor"), "repeatable");
    }

    #[test]
    fn sql_search_matches_the_r1_reference() {
        let conn = memory_db();
        let items = library::generate_library();
        for query in [
            "",
            "  ",
            "Severance",
            "severance",
            "SEVERANCE",
            "dark",
            "the",
            "The ",
            "an",
            "harbor",
            "HaRbOr",
            "Hafen",
            "premier",
            "Deuxième",
            "shogun",
            "Shōgun",
            "ö",
            "NÖRDLICHE",
            "Brücke",
            "2049",
            "zzzz",
            "%",
            "_",
            "'",
        ] {
            let reference: Vec<u32> = library::search(&items, query)
                .into_iter()
                .map(|index| items[index].id)
                .collect();
            assert_eq!(ids(&search(&conn, query).unwrap()), reference, "{query:?}");
        }
    }

    #[test]
    fn unknown_enum_values_are_errors() {
        let conn = memory_db();
        // The CHECK constraint rejects bad writes...
        let insert = conn.execute(
            "INSERT INTO media VALUES (2000, 'X', 'X', 2000, 'Film', 'planned', NULL, NULL, '', 'x')",
            [],
        );
        assert!(insert.is_err());
        // ...and the row mapper rejects bad reads instead of guessing.
        let row = |sql: &str| conn.query_row(sql, [], item_from_row);
        assert!(row("SELECT 1, 'X', 'X', 2000, 'Film', 'planned', NULL, NULL, ''").is_err());
        assert!(row("SELECT 1, 'X', 'X', 2000, 'TV', 'paused', NULL, NULL, ''").is_err());
        assert!(row("SELECT 1, 'X', 'X', 2000, 'TV', 'episodes', NULL, NULL, ''").is_err());
        assert!(row("SELECT 1, 'X', 'X', 2000, 'TV', 'episodes', 1, 2, ''").is_ok());
    }

    #[test]
    fn failures_surface_as_errors() {
        let dir = std::env::temp_dir().join("bingee-missing-dir-xyz");
        assert!(open(&dir.join("library.db")).is_err(), "open");

        // A `media` table with the wrong shape: seeding must fail, not
        // continue with an empty library.
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE media (x)").unwrap();
        assert!(init(&mut conn).is_err(), "seed");

        conn.execute_batch("DROP TABLE media").unwrap();
        assert!(search(&conn, "").is_err(), "query");
    }

    /// Informal R2 observation, not the R4 benchmark. Run with
    /// `cargo test --release informal_query_timings -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn informal_query_timings() {
        const SAMPLES: usize = 50;
        let micros = |d: Duration| d.as_secs_f64() * 1e6;
        let db = TempDb::new("timing");

        let start = Instant::now();
        drop(open(&db.0).unwrap());
        println!(
            "open + create + seed (fresh file): {:.0} us",
            micros(start.elapsed())
        );
        let start = Instant::now();
        let conn = open(&db.0).unwrap();
        println!(
            "open existing (no reseed): {:.0} us",
            micros(start.elapsed())
        );

        let items = library::generate_library();
        for query in ["", "harbor", "zzzz"] {
            let mut sql: Vec<Duration> = (0..SAMPLES)
                .map(|_| {
                    let start = Instant::now();
                    std::hint::black_box(search(&conn, query).unwrap());
                    start.elapsed()
                })
                .collect();
            let first = sql[0];
            sql.sort();
            let mut mem: Vec<Duration> = (0..SAMPLES)
                .map(|_| {
                    let start = Instant::now();
                    std::hint::black_box(library::search(&items, query));
                    start.elapsed()
                })
                .collect();
            mem.sort();
            println!(
                "{query:>8?} rows={:4} sql: first={:.0} min={:.0} median={:.0} p95={:.0} max={:.0} us | in-memory R1: median={:.0} us",
                search(&conn, query).unwrap().len(),
                micros(first),
                micros(sql[0]),
                micros(sql[SAMPLES / 2]),
                micros(sql[SAMPLES * 95 / 100]),
                micros(sql[SAMPLES - 1]),
                micros(mem[SAMPLES / 2]),
            );
        }
    }
}
