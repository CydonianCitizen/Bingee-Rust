//! The production database: open, configure the connection, migrate the
//! schema. See `docs/adr/0006-production-storage-and-schema-v1.md`.
//!
//! `PRAGMA user_version` holds the schema version. All pending migrations run
//! in one transaction, so a failure leaves the file exactly as it was. A file
//! that cannot be opened or migrated is reported, never deleted or recreated.

use std::path::Path;
use std::sync::{Arc, Mutex, PoisonError};

use rusqlite::functions::FunctionFlags;
use rusqlite::{Connection, ErrorCode, TransactionBehavior};

use crate::diagnostics::Log;
use crate::error::{AppError, ErrorKind};

/// `PRAGMA application_id`: marks the file as Bingee's ("Bing").
const APPLICATION_ID: i32 = 0x4269_6E67;

/// Step `n` (1-based) upgrades schema version `n - 1` to `n`. Never edit a
/// step that has shipped: append a new one.
const MIGRATIONS: &[&str] = &[SCHEMA_V1, SCHEMA_V2];

const SCHEMA_V1: &str = "
PRAGMA application_id = 1114205799;

-- Canonical local metadata. Cached metadata exists independently of library
-- membership. AUTOINCREMENT: a local id is never reused after a delete.
CREATE TABLE media (
    local_media_id      INTEGER PRIMARY KEY AUTOINCREMENT,
    media_type          TEXT NOT NULL CHECK (media_type IN ('movie', 'tv')),
    title               TEXT NOT NULL CHECK (title <> ''),
    original_title      TEXT,
    -- ISO 8601 date; for TV, the first air date.
    release_date        TEXT CHECK (release_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    overview            TEXT,
    -- Provider image paths (for TMDB, relative to its image base URL).
    poster_path         TEXT,
    backdrop_path       TEXT,
    runtime_minutes     INTEGER CHECK (runtime_minutes > 0),
    -- Unix seconds, UTC.
    metadata_updated_at INTEGER,
    -- Parent key for external_refs' type-checked foreign key.
    UNIQUE (local_media_id, media_type)
) STRICT;

-- Provider identity. The key includes media_type: TMDB movie 603 and TMDB
-- TV 603 are different titles. The composite foreign key makes the type
-- match the media row it points to.
CREATE TABLE external_refs (
    source         TEXT NOT NULL CHECK (source GLOB '[a-z]*' AND source NOT GLOB '*[^a-z0-9_]*'),
    media_type     TEXT NOT NULL CHECK (media_type IN ('movie', 'tv')),
    external_id    TEXT NOT NULL CHECK (external_id <> ''),
    local_media_id INTEGER NOT NULL,
    PRIMARY KEY (source, media_type, external_id),
    FOREIGN KEY (local_media_id, media_type)
        REFERENCES media (local_media_id, media_type) ON DELETE CASCADE,
    -- One spelling per TMDB id, so '0603' cannot dodge the key.
    CHECK (source <> 'tmdb' OR (external_id GLOB '[1-9]*' AND external_id NOT GLOB '*[^0-9]*'))
) STRICT, WITHOUT ROWID;

-- The refs of one media row: the foreign-key child index (cascade and
-- parent checks) and the future media -> TMDB id lookup.
CREATE INDEX external_refs_by_media ON external_refs (local_media_id, media_type);

-- Membership in the user's library, separate from metadata. RESTRICT:
-- removing cached metadata can never silently drop a library entry.
CREATE TABLE library_entries (
    local_media_id INTEGER PRIMARY KEY
        REFERENCES media (local_media_id) ON DELETE RESTRICT,
    -- Unix seconds, UTC.
    added_at       INTEGER NOT NULL
) STRICT;
";

/// R9: provider-owned detail metadata (ADR-0013, ADR-0014). Everything here
/// mirrors what a metadata provider says. Nothing personal (watched state,
/// ratings, progress) belongs in these tables, and a future personal table
/// must never be a cascade child of one of them (ADR-0014).
const SCHEMA_V2: &str = "
-- Detail-only fields of a title, filled by /3/movie/{id} and /3/tv/{id}.
-- `runtime_minutes` already exists: a movie's runtime, or a series' typical
-- episode runtime.
ALTER TABLE media ADD COLUMN status TEXT;
ALTER TABLE media ADD COLUMN tagline TEXT;
ALTER TABLE media ADD COLUMN last_air_date TEXT
    CHECK (last_air_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]');
ALTER TABLE media ADD COLUMN season_count INTEGER CHECK (season_count >= 0);
ALTER TABLE media ADD COLUMN episode_count INTEGER CHECK (episode_count >= 0);
-- Unix seconds, UTC: when the detail endpoint last answered. NULL means the
-- row only holds what a search result gave, so detail was never fetched.
ALTER TABLE media ADD COLUMN details_fetched_at INTEGER;

-- Genres as data, not a presentation string. One row per provider genre.
CREATE TABLE genres (
    source      TEXT NOT NULL CHECK (source GLOB '[a-z]*' AND source NOT GLOB '*[^a-z0-9_]*'),
    external_id TEXT NOT NULL CHECK (external_id <> ''),
    name        TEXT NOT NULL CHECK (name <> ''),
    PRIMARY KEY (source, external_id)
) STRICT, WITHOUT ROWID;

-- Which genres a title has. The primary key makes a genre unrepeatable per
-- title, however often a provider repeats it.
CREATE TABLE media_genres (
    local_media_id INTEGER NOT NULL REFERENCES media (local_media_id) ON DELETE CASCADE,
    source         TEXT NOT NULL,
    external_id    TEXT NOT NULL,
    PRIMARY KEY (local_media_id, source, external_id),
    FOREIGN KEY (source, external_id) REFERENCES genres (source, external_id)
) STRICT, WITHOUT ROWID;

-- Season summaries, from the series detail response. media_type in the
-- foreign key keeps seasons off movie rows.
CREATE TABLE seasons (
    local_media_id      INTEGER NOT NULL,
    media_type          TEXT NOT NULL CHECK (media_type = 'tv'),
    -- 0 is TMDB's specials season and is as valid as any other.
    season_number       INTEGER NOT NULL CHECK (season_number >= 0),
    external_id         TEXT CHECK (external_id <> ''),
    name                TEXT,
    overview            TEXT,
    air_date            TEXT CHECK (air_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    -- How many episodes the provider says the season has, or NULL.
    episode_count       INTEGER CHECK (episode_count >= 0),
    poster_path         TEXT,
    -- Coverage, deliberately separate from the summary's freshness: when the
    -- episode list was last fetched, and how many episodes that fetch gave.
    episodes_fetched_at INTEGER,
    episodes_known      INTEGER CHECK (episodes_known >= 0),
    metadata_updated_at INTEGER,
    PRIMARY KEY (local_media_id, season_number),
    FOREIGN KEY (local_media_id, media_type)
        REFERENCES media (local_media_id, media_type) ON DELETE CASCADE
) STRICT;

-- Episode metadata only. No watched state, no rating, no progress: those are
-- a later milestone and get their own tables.
CREATE TABLE episodes (
    local_media_id      INTEGER NOT NULL,
    season_number       INTEGER NOT NULL,
    episode_number      INTEGER NOT NULL CHECK (episode_number >= 0),
    -- The provider's episode id, the stable identity when it exists.
    external_id         TEXT CHECK (external_id <> ''),
    name                TEXT,
    overview            TEXT,
    air_date            TEXT CHECK (air_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
    runtime_minutes     INTEGER CHECK (runtime_minutes > 0),
    still_path          TEXT,
    metadata_updated_at INTEGER,
    PRIMARY KEY (local_media_id, season_number, episode_number),
    FOREIGN KEY (local_media_id, season_number)
        REFERENCES seasons (local_media_id, season_number) ON DELETE CASCADE
) STRICT;

-- One provider episode id per season, so a repeated refresh cannot list the
-- same episode twice. Scoped to the season: a provider that moves an episode
-- to another season is a metadata change, not a conflict.
CREATE UNIQUE INDEX episodes_by_external
    ON episodes (local_media_id, season_number, external_id) WHERE external_id IS NOT NULL;
";

/// The one connection to the library database. Owned by whoever needs it
/// (the library view); closed when dropped. No global, no pool.
pub struct Database {
    conn: Connection,
}

impl Database {
    /// Opens or creates the database at `path` and migrates it to the latest
    /// schema version. The parent folder must exist.
    pub fn open(path: &Path, log: &Log) -> Result<Self, AppError> {
        let conn = Connection::open(path)
            .map_err(|err| AppError::database("The library database could not be opened.", err))?;
        Self::init(conn, MIGRATIONS, log)
    }

    #[cfg(test)]
    pub fn open_in_memory() -> Self {
        Self::init(
            Connection::open_in_memory().unwrap(),
            MIGRATIONS,
            &Log::stderr_only(),
        )
        .unwrap()
    }

    fn init(mut conn: Connection, migrations: &[&str], log: &Log) -> Result<Self, AppError> {
        configure(&conn)?;
        migrate(&mut conn, migrations, log)?;
        Ok(Self { conn })
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }

    pub fn schema_version(&self) -> Result<u32, AppError> {
        user_version(&self.conn).map_err(read_error)
    }
}

/// The user's database, shared by the Library page and Discover: empty until
/// it opens. Only ever locked on the UI thread; the mutex makes it `Send` for
/// the code that also holds worker-side state.
#[derive(Clone, Default)]
pub struct SharedDb(Arc<Mutex<Option<Database>>>);

impl SharedDb {
    pub fn set(&self, db: Database) {
        *self.0.lock().unwrap_or_else(PoisonError::into_inner) = Some(db);
    }

    /// Runs `f` on the open database, or fails if it is not open.
    pub fn with<T>(&self, f: impl FnOnce(&Database) -> Result<T, AppError>) -> Result<T, AppError> {
        match self
            .0
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .as_ref()
        {
            Some(db) => f(db),
            None => Err(AppError::new(
                ErrorKind::Database,
                "Your library is not open.",
            )),
        }
    }
}

fn configure(conn: &Connection) -> Result<(), AppError> {
    let setup = || -> rusqlite::Result<bool> {
        conn.pragma_update(None, "foreign_keys", true)?;
        // Unicode-aware lowercase for search: SQLite's lower() folds ASCII
        // only. Used in queries, never in the schema, so other tools can
        // still open the file.
        conn.create_scalar_function(
            "bingee_fold",
            1,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| {
                Ok(ctx
                    .get::<Option<String>>(0)?
                    .map(|text| text.to_lowercase()))
            },
        )?;
        conn.pragma_query_value(None, "foreign_keys", |row| row.get(0))
    };
    match setup() {
        Ok(true) => Ok(()),
        Ok(false) => Err(AppError::new(
            ErrorKind::Internal,
            "The database engine does not enforce foreign keys.",
        )),
        Err(err) => Err(read_error(err)),
    }
}

fn user_version(conn: &Connection) -> rusqlite::Result<u32> {
    conn.pragma_query_value(None, "user_version", |row| row.get(0))
}

/// Brings the file to the last version in `migrations`, or refuses it
/// without writing anything.
fn migrate(conn: &mut Connection, migrations: &[&str], log: &Log) -> Result<(), AppError> {
    let latest = migrations.len() as u32;
    // IMMEDIATE takes the write lock first, so a second running copy cannot
    // migrate the same file at the same time.
    let tx = conn
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(read_error)?;
    let version = user_version(&tx).map_err(read_error)?;
    let app_id: i32 = tx
        .pragma_query_value(None, "application_id", |row| row.get(0))
        .map_err(read_error)?;
    // Also reads the schema page, so a damaged file fails here.
    let objects: i64 = tx
        .query_row("SELECT count(*) FROM sqlite_schema", [], |row| row.get(0))
        .map_err(read_error)?;
    let empty = version == 0 && app_id == 0 && objects == 0;
    if !empty && app_id != APPLICATION_ID {
        return Err(AppError::new(
            ErrorKind::InvalidData,
            "This file is not a Bingee Desktop library database. It was left unchanged.",
        ));
    }
    if version > latest {
        return Err(AppError::new(
            ErrorKind::Database,
            format!(
                "The library database was created by a newer version of Bingee Desktop \
                 (schema version {version}; this version supports up to {latest}). It was left \
                 unchanged. Update Bingee Desktop to open it."
            ),
        ));
    }
    if version == latest {
        return Ok(()); // Nothing written: dropping the transaction rolls back.
    }

    log.info(format!(
        "Migrating the database from schema version {version} to {latest}"
    ));
    let failed = |err| {
        AppError::database(
            format!(
                "The library database could not be upgraded to schema version {latest}. It was \
                 left unchanged."
            ),
            err,
        )
    };
    for step in &migrations[version as usize..] {
        tx.execute_batch(step).map_err(failed)?;
    }
    tx.pragma_update(None, "user_version", latest)
        .map_err(failed)?;
    tx.commit().map_err(failed)?;
    log.info(format!("Migration to schema version {latest} complete"));
    Ok(())
}

/// A failure to read the file at all. Damaged and non-SQLite files get their
/// own message.
fn read_error(err: rusqlite::Error) -> AppError {
    match err.sqlite_error_code() {
        Some(ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt) => AppError::new(
            ErrorKind::InvalidData,
            "The library database is damaged or is not a database file. It was left unchanged.",
        )
        .with_source(err),
        _ => AppError::database("The library database could not be read.", err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::TestDir;
    use rusqlite::params;

    fn open(path: &Path) -> Result<Database, AppError> {
        Database::open(path, &Log::stderr_only())
    }

    fn tables(conn: &Connection) -> Vec<String> {
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name")
            .unwrap();
        stmt.query_map([], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    }

    fn insert_media(conn: &Connection, kind: &str, title: &str) -> i64 {
        conn.execute(
            "INSERT INTO media (media_type, title) VALUES (?1, ?2)",
            params![kind, title],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    fn add_ref(
        conn: &Connection,
        source: &str,
        kind: &str,
        id: &str,
        media: i64,
    ) -> rusqlite::Result<usize> {
        conn.execute(
            "INSERT INTO external_refs (source, media_type, external_id, local_media_id)
             VALUES (?1, ?2, ?3, ?4)",
            params![source, kind, id, media],
        )
    }

    fn constraint(result: rusqlite::Result<usize>) -> bool {
        result.is_err_and(|err| err.sqlite_error_code() == Some(ErrorCode::ConstraintViolation))
    }

    #[test]
    fn fresh_database_migrates_to_the_latest_schema() {
        let dir = TestDir::new("db-fresh");
        let db = open(&dir.0.join("bingee.db")).unwrap();
        assert_eq!(MIGRATIONS.len(), 2);
        assert_eq!(db.schema_version().unwrap(), 2);
        let app_id: i32 = db
            .conn()
            .pragma_query_value(None, "application_id", |r| r.get(0))
            .unwrap();
        assert_eq!(app_id, APPLICATION_ID);
        assert_eq!(
            tables(db.conn()),
            [
                "episodes",
                "external_refs",
                "genres",
                "library_entries",
                "media",
                "media_genres",
                "seasons",
                "sqlite_sequence"
            ]
        );
        let foreign_keys: bool = db
            .conn()
            .pragma_query_value(None, "foreign_keys", |r| r.get(0))
            .unwrap();
        assert!(foreign_keys);
    }

    /// A schema v1 file exactly as R6-R8 wrote it, with `media`, an external
    /// ref and a library entry, used to test the real v1 -> v2 upgrade.
    fn v1_fixture(path: &Path) -> i64 {
        let conn = Connection::open(path).unwrap();
        let db = Database::init(conn, &MIGRATIONS[..1], &Log::stderr_only()).unwrap();
        assert_eq!(db.schema_version().unwrap(), 1);
        db.conn()
            .execute(
                "INSERT INTO media (media_type, title, original_title, release_date, overview,
                                    poster_path, backdrop_path, runtime_minutes, metadata_updated_at)
                 VALUES ('tv', 'Severance', 'Severance', '2022-02-17', 'Work-life balance.',
                         '/p.jpg', '/b.jpg', 47, 1757808000)",
                [],
            )
            .unwrap();
        let id = db.conn().last_insert_rowid();
        add_ref(db.conn(), "tmdb", "tv", "95396", id).unwrap();
        db.conn()
            .execute("INSERT INTO library_entries VALUES (?1, 1757808000)", [id])
            .unwrap();
        id
    }

    #[test]
    fn a_real_v1_file_upgrades_to_v2_and_keeps_everything() {
        let dir = TestDir::new("db-v1-to-v2");
        let path = dir.0.join("bingee.db");
        let id = v1_fixture(&path);

        let db = open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), 2);
        // Library membership, provider identity and poster references survive.
        let row: (i64, String, String, String, i64, Option<i64>) = db
            .conn()
            .query_row(
                "SELECT m.local_media_id, m.title, m.poster_path, r.external_id, l.added_at,
                        m.details_fetched_at
                 FROM media AS m JOIN external_refs AS r USING (local_media_id)
                 JOIN library_entries AS l USING (local_media_id)",
                [],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get(3)?,
                        r.get(4)?,
                        r.get(5)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(
            row,
            (
                id,
                "Severance".into(),
                "/p.jpg".into(),
                "95396".into(),
                1_757_808_000,
                None
            ),
            "detail was never fetched for an upgraded row"
        );
        // The new tables are there and empty.
        for table in ["genres", "media_genres", "seasons", "episodes"] {
            let rows: i64 = db
                .conn()
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap();
            assert_eq!(rows, 0, "{table}");
        }
        // Reopening v2 writes nothing.
        drop(db);
        let before = std::fs::read(&path).unwrap();
        assert_eq!(open(&path).unwrap().schema_version().unwrap(), 2);
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }

    #[test]
    fn a_failed_v2_migration_leaves_a_v1_file_untouched() {
        let dir = TestDir::new("db-v2-rollback");
        let path = dir.0.join("bingee.db");
        v1_fixture(&path);
        let before = std::fs::read(&path).unwrap();
        let broken: &[&str] = &[SCHEMA_V1, "ALTER TABLE media ADD COLUMN ok TEXT; NOT SQL"];
        let conn = Connection::open(&path).unwrap();
        assert!(Database::init(conn, broken, &Log::stderr_only()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before, "file changed");
        let db = open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), 2, "still upgradable");
        assert!(!tables(db.conn()).contains(&"ok".to_owned()));
    }

    #[test]
    fn reopening_the_latest_schema_changes_nothing_and_keeps_data() {
        let dir = TestDir::new("db-reopen");
        let path = dir.0.join("bingee.db");
        {
            let db = open(&path).unwrap();
            let id = insert_media(db.conn(), "tv", "Severance");
            add_ref(db.conn(), "tmdb", "tv", "95396", id).unwrap();
            db.conn()
                .execute("INSERT INTO library_entries VALUES (?1, 1757808000)", [id])
                .unwrap();
        }
        let before = std::fs::read(&path).unwrap();
        for _ in 0..3 {
            let db = open(&path).unwrap();
            assert_eq!(db.schema_version().unwrap(), 2);
            let row: (String, String, i64) = db
                .conn()
                .query_row(
                    "SELECT m.title, r.external_id, l.added_at
                     FROM library_entries l JOIN media m USING (local_media_id)
                     JOIN external_refs r USING (local_media_id)",
                    [],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .unwrap();
            assert_eq!(row, ("Severance".into(), "95396".into(), 1_757_808_000));
        }
        assert_eq!(
            std::fs::read(&path).unwrap(),
            before,
            "reopening wrote to the file"
        );
    }

    #[test]
    fn foreign_keys_are_enforced() {
        let db = Database::open_in_memory();
        let conn = db.conn();
        assert!(constraint(
            conn.execute("INSERT INTO library_entries VALUES (42, 0)", [])
        ));
        assert!(constraint(add_ref(conn, "tmdb", "movie", "603", 42)));

        // A library entry keeps its media; refs go with their media.
        let kept = insert_media(conn, "movie", "The Matrix");
        conn.execute("INSERT INTO library_entries VALUES (?1, 0)", [kept])
            .unwrap();
        assert!(constraint(
            conn.execute("DELETE FROM media WHERE local_media_id = ?1", [kept])
        ));
        let cached = insert_media(conn, "movie", "Cached only");
        add_ref(conn, "tmdb", "movie", "604", cached).unwrap();
        conn.execute("DELETE FROM media WHERE local_media_id = ?1", [cached])
            .unwrap();
        let refs: i64 = conn
            .query_row("SELECT count(*) FROM external_refs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(refs, 0);
    }

    #[test]
    fn external_identity_includes_the_media_type() {
        let db = Database::open_in_memory();
        let conn = db.conn();
        let movie = insert_media(conn, "movie", "The Matrix");
        let show = insert_media(conn, "tv", "Some Show");
        let other = insert_media(conn, "movie", "Another Movie");

        add_ref(conn, "tmdb", "movie", "603", movie).unwrap();
        // Same integer, other type: a different identity.
        add_ref(conn, "tmdb", "tv", "603", show).unwrap();
        // The same (source, media_type, external_id) twice: rejected, even
        // for another media row.
        assert!(constraint(add_ref(conn, "tmdb", "movie", "603", other)));
        // A TV ref cannot point at a movie.
        assert!(constraint(add_ref(conn, "tmdb", "tv", "700", movie)));
        // Canonical spelling only.
        for (source, id) in [
            ("tmdb", "0603"),
            ("tmdb", "tt0133093"),
            ("TMDB", "605"),
            ("tmdb", ""),
        ] {
            assert!(
                constraint(add_ref(conn, source, "movie", id, other)),
                "{source} {id}"
            );
        }
        assert!(constraint(add_ref(conn, "tmdb", "person", "1", other)));
    }

    #[test]
    fn media_values_are_checked() {
        let db = Database::open_in_memory();
        let insert = |sql: &str| db.conn().execute(sql, []);
        assert!(constraint(insert(
            "INSERT INTO media (media_type, title) VALUES ('film', 'X')"
        )));
        assert!(constraint(insert(
            "INSERT INTO media (media_type, title) VALUES ('movie', '')"
        )));
        assert!(constraint(insert(
            "INSERT INTO media (media_type, title, release_date) VALUES ('movie', 'X', '2024')"
        )));
        assert!(constraint(insert(
            "INSERT INTO media (media_type, title, runtime_minutes) VALUES ('movie', 'X', 0)"
        )));
        // STRICT: a string is not an integer.
        assert!(insert("INSERT INTO media (media_type, title, runtime_minutes) VALUES ('movie', 'X', 'long')").is_err());
        insert("INSERT INTO media (media_type, title, release_date, runtime_minutes) VALUES ('movie', 'X', '1999-03-31', 136)").unwrap();
    }

    #[test]
    fn failed_migration_rolls_back_completely() {
        let broken: &[&str] = &[
            SCHEMA_V1,
            SCHEMA_V2,
            "CREATE TABLE extra (x); THIS IS NOT SQL",
        ];
        let dir = TestDir::new("db-rollback");

        // Fresh file, the real steps + a failing one in one transaction:
        // nothing remains.
        let path = dir.0.join("fresh.db");
        let conn = Connection::open(&path).unwrap();
        let error = Database::init(conn, broken, &Log::stderr_only())
            .err()
            .unwrap();
        assert_eq!(error.kind, ErrorKind::Database);
        let conn = Connection::open(&path).unwrap();
        assert_eq!(user_version(&conn).unwrap(), 0);
        assert!(tables(&conn).is_empty());

        // A migrated file with data: the failed step leaves it as it was.
        let path = dir.0.join("current.db");
        let id = insert_media(open(&path).unwrap().conn(), "movie", "Kept");
        let before = std::fs::read(&path).unwrap();
        let conn = Connection::open(&path).unwrap();
        assert!(Database::init(conn, broken, &Log::stderr_only()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let db = open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), MIGRATIONS.len() as u32);
        assert!(!tables(db.conn()).contains(&"extra".to_owned()));
        let title: String = db
            .conn()
            .query_row(
                "SELECT title FROM media WHERE local_media_id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(title, "Kept");
    }

    /// Opens `bytes` as a database file: it must be refused with `kind`, and
    /// the file must still hold exactly `bytes`.
    fn assert_refused_untouched(name: &str, bytes: &[u8], kind: ErrorKind) {
        let dir = TestDir::new(name);
        let path = dir.0.join("bingee.db");
        std::fs::write(&path, bytes).unwrap();
        for _ in 0..2 {
            let error = open(&path).err().expect("refused");
            assert_eq!(error.kind, kind, "{error}");
            assert!(
                !error.message.contains("SQLITE"),
                "user message: {}",
                error.message
            );
        }
        assert_eq!(std::fs::read(&path).unwrap(), bytes, "file changed");
        let names: Vec<_> = std::fs::read_dir(&dir.0)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(
            names,
            ["bingee.db"],
            "no journal or replacement left behind"
        );
    }

    #[test]
    fn corrupt_file_is_refused_and_never_deleted() {
        assert_refused_untouched("db-garbage", &[0x5a; 8192], ErrorKind::InvalidData);
        // A valid v1 header over a damaged schema page.
        let dir = TestDir::new("db-mangled-src");
        let path = dir.0.join("src.db");
        drop(open(&path).unwrap());
        let mut bytes = std::fs::read(&path).unwrap();
        bytes[100..4096].fill(0xff);
        assert_refused_untouched("db-mangled", &bytes, ErrorKind::InvalidData);
    }

    #[test]
    fn foreign_and_newer_databases_are_refused_untouched() {
        let dir = TestDir::new("db-sources");
        let bytes = |name: &str, setup: &str| {
            let path = dir.0.join(name);
            Connection::open(&path)
                .unwrap()
                .execute_batch(setup)
                .unwrap();
            std::fs::read(&path).unwrap()
        };
        // The R0-R5 spike database: unversioned, with its own `media` table.
        let spike = bytes(
            "spike.db",
            "CREATE TABLE media (local_id INTEGER PRIMARY KEY, title TEXT)",
        );
        assert_refused_untouched("db-spike", &spike, ErrorKind::InvalidData);
        let other_app = bytes(
            "other.db",
            "PRAGMA application_id = 7; PRAGMA user_version = 1;",
        );
        assert_refused_untouched("db-other", &other_app, ErrorKind::InvalidData);

        let path = dir.0.join("newer.db");
        drop(open(&path).unwrap());
        Connection::open(&path)
            .unwrap()
            .pragma_update(None, "user_version", MIGRATIONS.len() as u32 + 1)
            .unwrap();
        let newer = std::fs::read(&path).unwrap();
        assert_refused_untouched("db-newer", &newer, ErrorKind::Database);
    }

    #[test]
    fn unicode_fold_function_is_registered() {
        let db = Database::open_in_memory();
        let folded: Option<String> = db
            .conn()
            .query_row("SELECT bingee_fold('NÖRDLICHE Brücke')", [], |r| r.get(0))
            .unwrap();
        assert_eq!(folded.as_deref(), Some("nördliche brücke"));
        let null: Option<String> = db
            .conn()
            .query_row("SELECT bingee_fold(NULL)", [], |r| r.get(0))
            .unwrap();
        assert_eq!(null, None);
    }
}
