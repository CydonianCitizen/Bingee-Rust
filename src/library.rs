//! The user's library, read from and written to SQLite only (ADR-0010).
//!
//! Three concepts stay apart here as in the schema: cached metadata
//! (`media`), provider identity (`external_refs`) and membership
//! (`library_entries`). Nothing in this module needs the network or a token.
//! Plain Rust types, no Slint.

use std::collections::HashSet;

use rusqlite::{OptionalExtension, Row, Transaction, TransactionBehavior, params};

use crate::database::Database;
use crate::error::{AppError, ErrorKind};
use crate::search::{ExternalRef, MediaSearchResult, Source};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaType {
    Movie,
    Tv,
}

impl MediaType {
    pub fn label(self) -> &'static str {
        match self {
            MediaType::Movie => "Movie",
            MediaType::Tv => "TV",
        }
    }

    /// The stored and wire form: `media.media_type`, and TMDB's path segment.
    pub fn key(self) -> &'static str {
        match self {
            MediaType::Movie => "movie",
            MediaType::Tv => "tv",
        }
    }

    fn from_key(key: &str) -> Result<Self, AppError> {
        match key {
            "movie" => Ok(MediaType::Movie),
            "tv" => Ok(MediaType::Tv),
            other => Err(AppError::new(
                ErrorKind::InvalidData,
                format!("A title in your library has an unknown media type ({other})."),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LibraryItem {
    pub id: i64,
    pub media_type: MediaType,
    pub title: String,
    pub original_title: Option<String>,
    /// `YYYY-MM-DD`, enforced by the schema.
    pub release_date: Option<String>,
    pub overview: Option<String>,
    /// The provider's poster path, for the poster cache.
    pub poster_path: Option<String>,
}

impl LibraryItem {
    pub fn year(&self) -> Option<&str> {
        self.release_date.as_deref().and_then(|date| date.get(..4))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sort {
    #[default]
    RecentlyAdded,
    Title,
}

/// What the Library page shows: a text search, a type filter and an order.
#[derive(Debug, Clone, Default)]
pub struct Query {
    pub text: String,
    pub kind: Option<MediaType>,
    pub sort: Sort,
}

/// Library titles matching `query`: the text (trimmed, Unicode case
/// folded) inside the title or original title, of `kind` if set, in the
/// requested order with the local id as the tiebreaker. Cached media that is
/// not in the library never appears.
pub fn search(db: &Database, query: &Query) -> Result<Vec<LibraryItem>, AppError> {
    let failed = |err| AppError::database("Your library could not be searched.", err);
    let order = match query.sort {
        Sort::RecentlyAdded => "l.added_at DESC, m.local_media_id DESC",
        Sort::Title => "bingee_fold(m.title), m.local_media_id",
    };
    // ponytail: loads every match. For very large libraries, query ids only
    // and load the visible rows by id.
    let mut stmt = db
        .conn()
        .prepare_cached(&format!(
            "SELECT m.local_media_id, m.media_type, m.title, m.original_title,
                    m.release_date, m.overview, m.poster_path
             FROM library_entries AS l JOIN media AS m ON m.local_media_id = l.local_media_id
             WHERE (?2 IS NULL OR m.media_type = ?2)
               AND (instr(bingee_fold(m.title), ?1) > 0
                    OR instr(bingee_fold(m.original_title), ?1) > 0)
             ORDER BY {order}"
        ))
        .map_err(failed)?;
    let text = query.text.trim().to_lowercase();
    let kind = query.kind.map(MediaType::key);
    let rows = stmt
        .query_map(params![text, kind], |row| {
            Ok((row.get::<_, String>(1)?, item(row)?))
        })
        .map_err(failed)?;
    rows.map(|row| {
        let (kind, mut item) = row.map_err(failed)?;
        item.media_type = MediaType::from_key(&kind)?;
        Ok(item)
    })
    .collect()
}

fn item(row: &Row) -> rusqlite::Result<LibraryItem> {
    Ok(LibraryItem {
        id: row.get(0)?,
        media_type: MediaType::Movie, // replaced by the caller after validation
        title: row.get(2)?,
        original_title: row.get(3)?,
        release_date: row.get(4)?,
        overview: row.get(5)?,
        poster_path: row.get(6)?,
    })
}

/// Titles in the library, without any search or filter.
pub fn count(db: &Database) -> Result<usize, AppError> {
    db.conn()
        .query_row("SELECT count(*) FROM library_entries", [], |row| {
            row.get::<_, i64>(0)
        })
        .map(|n| n as usize)
        .map_err(|err| AppError::database("Your library could not be read.", err))
}

/// The outcome of `add`, with the title's local id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Added {
    New(i64),
    AlreadyThere(i64),
}

/// Adds a search result to the library in one transaction: finds or creates
/// its `media` row through its provider identity, refreshes the metadata the
/// search provided (keeping stored values it lacks), and creates the
/// membership unless it exists. All or nothing; repeating it changes nothing.
/// `now` is Unix seconds, UTC.
pub fn add(db: &Database, result: &MediaSearchResult, now: i64) -> Result<Added, AppError> {
    let failed = |err| AppError::database("The title could not be added to your library.", err);
    let tx =
        Transaction::new_unchecked(db.conn(), TransactionBehavior::Immediate).map_err(failed)?;
    let id = match media_for(&tx, &result.external).map_err(failed)? {
        Some(id) => {
            tx.execute(
                "UPDATE media SET title = ?2,
                     original_title = coalesce(?3, original_title),
                     release_date = coalesce(?4, release_date),
                     overview = coalesce(?5, overview),
                     poster_path = coalesce(?6, poster_path),
                     backdrop_path = coalesce(?7, backdrop_path),
                     metadata_updated_at = ?8
                 WHERE local_media_id = ?1",
                params![
                    id,
                    result.title,
                    result.original_title,
                    result.release_date,
                    result.overview,
                    result.poster_path,
                    result.backdrop_path,
                    now
                ],
            )
            .map_err(failed)?;
            id
        }
        None => {
            tx.execute(
                "INSERT INTO media (media_type, title, original_title, release_date, overview,
                                    poster_path, backdrop_path, metadata_updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    result.external.media_type.key(),
                    result.title,
                    result.original_title,
                    result.release_date,
                    result.overview,
                    result.poster_path,
                    result.backdrop_path,
                    now
                ],
            )
            .map_err(failed)?;
            let id = tx.last_insert_rowid();
            let external = &result.external;
            tx.execute(
                "INSERT INTO external_refs (source, media_type, external_id, local_media_id)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    external.source.key(),
                    external.media_type.key(),
                    external.id,
                    id
                ],
            )
            .map_err(failed)?;
            id
        }
    };
    let inserted = tx
        .execute(
            "INSERT INTO library_entries (local_media_id, added_at) VALUES (?1, ?2)
             ON CONFLICT (local_media_id) DO NOTHING",
            params![id, now],
        )
        .map_err(failed)?;
    tx.commit().map_err(failed)?;
    Ok(match inserted {
        0 => Added::AlreadyThere(id),
        _ => Added::New(id),
    })
}

fn media_for(tx: &Transaction, external: &ExternalRef) -> rusqlite::Result<Option<i64>> {
    tx.query_row(
        "SELECT local_media_id FROM external_refs
         WHERE source = ?1 AND media_type = ?2 AND external_id = ?3",
        params![
            external.source.key(),
            external.media_type.key(),
            external.id
        ],
        |row| row.get(0),
    )
    .optional()
}

/// Takes a title out of the library: only its membership goes. Its metadata,
/// provider identity and cached poster stay for a later re-add (ADR-0010).
/// Returns whether it was in the library.
pub fn remove(db: &Database, id: i64) -> Result<bool, AppError> {
    db.conn()
        .execute(
            "DELETE FROM library_entries WHERE local_media_id = ?1",
            [id],
        )
        .map(|deleted| deleted == 1)
        .map_err(|err| AppError::database("The title could not be removed from your library.", err))
}

/// Which of `refs` are in the library, in one query however many there are.
pub fn membership(db: &Database, refs: &[ExternalRef]) -> Result<HashSet<ExternalRef>, AppError> {
    if refs.is_empty() {
        return Ok(HashSet::new());
    }
    let failed = |err| AppError::database("Your library could not be read.", err);
    let keys: Vec<[&str; 3]> = refs
        .iter()
        .map(|r| [r.source.key(), r.media_type.key(), r.id.as_str()])
        .collect();
    let keys = serde_json::to_string(&keys).expect("strings always serialize");
    let mut stmt = db
        .conn()
        .prepare_cached(
            "SELECT r.source, r.media_type, r.external_id
             FROM json_each(?1) AS k
             JOIN external_refs AS r ON r.source = k.value ->> 0
                 AND r.media_type = k.value ->> 1 AND r.external_id = k.value ->> 2
             JOIN library_entries AS l ON l.local_media_id = r.local_media_id",
        )
        .map_err(failed)?;
    let rows = stmt
        .query_map([keys], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get(2)?,
            ))
        })
        .map_err(failed)?;
    rows.map(|row| {
        let (source, kind, id) = row.map_err(failed)?;
        let source = match source.as_str() {
            "tmdb" => Source::Tmdb,
            other => {
                return Err(AppError::new(
                    ErrorKind::InvalidData,
                    format!("Unknown metadata source {other}."),
                ));
            }
        };
        let media_type = MediaType::from_key(&kind)?;
        Ok(ExternalRef {
            source,
            media_type,
            id,
        })
    })
    .collect()
}

/// The current time as Unix seconds, UTC, for `added_at` and
/// `metadata_updated_at`.
pub fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Up to two uppercase initials from the first two words, for the poster
/// placeholder.
pub fn initials(title: &str) -> String {
    title
        .split_whitespace()
        .filter_map(|word| word.chars().find(|c| c.is_alphanumeric()))
        .take(2)
        .flat_map(char::to_uppercase)
        .collect()
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::diagnostics::Log;
    use crate::paths::TestDir;
    use crate::search::tests::item;
    use rusqlite::ErrorCode;

    /// Adds a title to `media` and, if `in_library`, to the library.
    pub fn add(
        db: &Database,
        kind: &str,
        title: &str,
        original: Option<&str>,
        in_library: bool,
    ) -> i64 {
        db.conn()
            .execute(
                "INSERT INTO media (media_type, title, original_title, release_date, overview)
                 VALUES (?1, ?2, ?3, '2017-12-01', 'An overview.')",
                params![kind, title, original],
            )
            .unwrap();
        let id = db.conn().last_insert_rowid();
        if in_library {
            db.conn()
                .execute("INSERT INTO library_entries VALUES (?1, 1757808000)", [id])
                .unwrap();
        }
        id
    }

    fn query(text: &str, kind: Option<MediaType>, sort: Sort) -> Query {
        Query {
            text: text.into(),
            kind,
            sort,
        }
    }

    fn titles(db: &Database, text: &str) -> Vec<String> {
        titles_for(db, &query(text, None, Sort::Title))
    }

    fn titles_for(db: &Database, query: &Query) -> Vec<String> {
        search(db, query)
            .unwrap()
            .into_iter()
            .map(|item| item.title)
            .collect()
    }

    fn rows(db: &Database, table: &str) -> i64 {
        db.conn()
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    /// Row counts of media, external_refs and library_entries.
    fn counts(db: &Database) -> [i64; 3] {
        ["media", "external_refs", "library_entries"].map(|t| rows(db, t))
    }

    fn result(kind: MediaType, id: u32, title: &str) -> MediaSearchResult {
        let mut result = item(kind, id, title);
        result.overview = Some(format!("About {title}."));
        result.poster_path = Some(format!("/p{id}.jpg"));
        result
    }

    #[test]
    fn empty_library_lists_nothing() {
        let db = Database::open_in_memory();
        assert!(titles(&db, "").is_empty());
        add(&db, "movie", "Cached, not in the library", None, false);
        assert!(titles(&db, "").is_empty());
        assert_eq!(count(&db).unwrap(), 0);
    }

    #[test]
    fn search_folds_unicode_case_and_orders_by_title() {
        let db = Database::open_in_memory();
        add(&db, "tv", "Dark", None, true);
        add(&db, "movie", "arrival", Some("Premier contact"), true);
        add(&db, "tv", "Nördliche Brücke", None, true);
        add(&db, "movie", "Hidden", None, false);
        assert_eq!(titles(&db, ""), ["arrival", "Dark", "Nördliche Brücke"]);
        assert_eq!(titles(&db, "  "), titles(&db, ""));
        assert_eq!(titles(&db, "NÖRDLICHE"), ["Nördliche Brücke"]);
        assert_eq!(titles(&db, "PREMIER"), ["arrival"], "original title");
        assert!(titles(&db, "hidden").is_empty(), "not in the library");
        assert!(titles(&db, "%").is_empty(), "no LIKE wildcards");

        let dark = &search(&db, &query("dark", None, Sort::Title)).unwrap()[0];
        assert_eq!(
            (dark.media_type, dark.year()),
            (MediaType::Tv, Some("2017"))
        );
        assert_eq!(dark.original_title, None);
    }

    #[test]
    fn filter_and_sort_run_in_sqlite() {
        let db = Database::open_in_memory();
        let add_at = |kind, id, title, at| add_result(&db, result(kind, id, title), at);
        add_at(MediaType::Movie, 1, "Zodiac", 100);
        add_at(MediaType::Tv, 1, "Andor", 300);
        add_at(MediaType::Movie, 2, "Élite", 200);
        add_at(MediaType::Tv, 2, "Babylon Berlin", 300);

        let recent = query("", None, Sort::RecentlyAdded);
        // Same second: the later local id first.
        assert_eq!(
            titles_for(&db, &recent),
            ["Babylon Berlin", "Andor", "Élite", "Zodiac"]
        );
        // Case folded, then code point order: accented initials after Z.
        let by_title = query("", None, Sort::Title);
        assert_eq!(
            titles_for(&db, &by_title),
            ["Andor", "Babylon Berlin", "Zodiac", "Élite"]
        );
        let tv = query("", Some(MediaType::Tv), Sort::Title);
        assert_eq!(titles_for(&db, &tv), ["Andor", "Babylon Berlin"]);
        let movies_with_i = query("I", Some(MediaType::Movie), Sort::Title);
        assert_eq!(titles_for(&db, &movies_with_i), ["Zodiac", "Élite"]);
        let item = &search(&db, &tv).unwrap()[0];
        assert_eq!(item.poster_path.as_deref(), Some("/p1.jpg"));
    }

    fn add_result(db: &Database, result: MediaSearchResult, at: i64) -> Added {
        super::add(db, &result, at).unwrap()
    }

    #[test]
    fn movie_and_tv_with_the_same_tmdb_id_are_two_titles() {
        let db = Database::open_in_memory();
        let movie = add_result(&db, result(MediaType::Movie, 603, "The Matrix"), 10);
        let tv = add_result(&db, result(MediaType::Tv, 603, "Some Series"), 10);
        let (Added::New(movie), Added::New(tv)) = (movie, tv) else {
            panic!("{movie:?} {tv:?}")
        };
        assert_ne!(movie, tv);
        assert_eq!(counts(&db), [2, 2, 2]);
    }

    #[test]
    fn adding_again_is_idempotent_and_refreshes_metadata() {
        let db = Database::open_in_memory();
        let first = add_result(&db, result(MediaType::Movie, 603, "The Matrix"), 10);
        let mut newer = result(MediaType::Movie, 603, "The Matrix (1999)");
        newer.overview = None; // the stored overview must survive
        assert_eq!(
            add_result(&db, newer, 99),
            Added::AlreadyThere(match first {
                Added::New(id) => id,
                other => panic!("{other:?}"),
            })
        );
        assert_eq!(counts(&db), [1, 1, 1]);
        let (title, overview, updated, added): (String, String, i64, i64) = db
            .conn()
            .query_row(
                "SELECT title, overview, metadata_updated_at, added_at
                 FROM media JOIN library_entries USING (local_media_id)",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!(title, "The Matrix (1999)");
        assert_eq!(overview, "About The Matrix.");
        assert_eq!((updated, added), (99, 10), "membership keeps its date");
    }

    /// A temporary trigger that makes one statement of `add` fail.
    fn fail_on(db: &Database, when: &str) {
        db.conn()
            .execute_batch(&format!(
                "CREATE TEMP TRIGGER fail {when} BEGIN SELECT RAISE(ABORT, 'simulated'); END;"
            ))
            .unwrap();
    }

    #[test]
    fn a_failure_at_any_step_leaves_nothing_behind() {
        for when in [
            "BEFORE INSERT ON external_refs",   // after the media insert
            "BEFORE INSERT ON library_entries", // after the external-ref insert
        ] {
            let db = Database::open_in_memory();
            fail_on(&db, when);
            let error = super::add(&db, &result(MediaType::Tv, 1, "Dark"), 10).unwrap_err();
            assert_eq!(error.kind, ErrorKind::Database, "{when}");
            assert_eq!(counts(&db), [0, 0, 0], "{when}");
        }

        // Re-adding a removed title: the metadata update is rolled back too.
        let db = Database::open_in_memory();
        let Added::New(id) = add_result(&db, result(MediaType::Tv, 1, "Dark"), 10) else {
            panic!()
        };
        remove(&db, id).unwrap();
        fail_on(&db, "BEFORE INSERT ON library_entries");
        assert!(super::add(&db, &result(MediaType::Tv, 1, "Renamed"), 20).is_err());
        assert_eq!(counts(&db), [1, 1, 0]);
        let title: String = db
            .conn()
            .query_row("SELECT title FROM media", [], |r| r.get(0))
            .unwrap();
        assert_eq!(title, "Dark");
    }

    #[test]
    fn remove_keeps_metadata_and_re_add_reuses_it() {
        let db = Database::open_in_memory();
        let Added::New(id) = add_result(&db, result(MediaType::Movie, 603, "The Matrix"), 10)
        else {
            panic!()
        };
        assert!(remove(&db, id).unwrap());
        assert!(!remove(&db, id).unwrap(), "already removed");
        assert_eq!(counts(&db), [1, 1, 0], "media and identity stay");
        assert!(titles(&db, "").is_empty());

        assert_eq!(
            add_result(&db, result(MediaType::Movie, 603, "The Matrix"), 50),
            Added::New(id)
        );
        assert_eq!(counts(&db), [1, 1, 1], "no duplicate metadata or identity");
        let added: i64 = db
            .conn()
            .query_row("SELECT added_at FROM library_entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(added, 50);
    }

    #[test]
    fn membership_is_one_query_over_full_identities() {
        let db = Database::open_in_memory();
        add_result(&db, result(MediaType::Movie, 603, "The Matrix"), 10);
        let Added::New(removed) = add_result(&db, result(MediaType::Movie, 7, "Gone"), 10) else {
            panic!()
        };
        remove(&db, removed).unwrap();
        let refs: Vec<ExternalRef> = [
            (MediaType::Movie, 603),
            (MediaType::Tv, 603),
            (MediaType::Movie, 7),
            (MediaType::Tv, 8),
        ]
        .into_iter()
        .map(|(kind, id)| result(kind, id, "x").external)
        .collect();
        let members = membership(&db, &refs).unwrap();
        assert_eq!(members, HashSet::from([refs[0].clone()]));
        assert!(membership(&db, &[]).unwrap().is_empty());
        assert!(membership(&db, &refs[1..]).unwrap().is_empty());
    }

    #[test]
    fn the_library_survives_reopening() {
        let dir = TestDir::new("library-reopen");
        let path = dir.0.join("bingee.db");
        let log = Log::stderr_only();
        {
            let db = Database::open(&path, &log).unwrap();
            add_result(&db, result(MediaType::Movie, 1, "Arrival"), 10);
            add_result(&db, result(MediaType::Tv, 1, "Dark"), 20);
        }
        let db = Database::open(&path, &log).unwrap();
        assert_eq!(
            titles_for(&db, &query("", None, Sort::RecentlyAdded)),
            ["Dark", "Arrival"]
        );
        assert_eq!(count(&db).unwrap(), 2);
    }

    #[test]
    fn the_schema_still_rejects_what_the_app_would_not_write() {
        let db = Database::open_in_memory();
        let mut bad = result(MediaType::Movie, 1, "Bad date");
        bad.release_date = Some("soon".into());
        let error = super::add(&db, &bad, 10).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Database);
        assert_eq!(counts(&db), [0, 0, 0]);
        let constraint = db
            .conn()
            .execute("INSERT INTO library_entries VALUES (999, 0)", [])
            .unwrap_err();
        assert_eq!(
            constraint.sqlite_error_code(),
            Some(ErrorCode::ConstraintViolation)
        );
    }

    /// Informal R8 observation, not a `BENCHMARK_SPEC.md` run: a 1,000-title
    /// library built from the fixture dataset. Run with
    /// `cargo test --release informal_library_timings -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn informal_library_timings() {
        use std::time::{Duration, Instant};
        let db = Database::open_in_memory();
        let dataset = crate::fixture::library::generate_library();
        let mut adds = Vec::new();
        for (n, record) in dataset.iter().enumerate() {
            let kind = if n % 3 == 0 {
                MediaType::Tv
            } else {
                MediaType::Movie
            };
            let mut result = item(kind, record.id, &record.title);
            result.original_title = Some(record.original_title.clone());
            result.poster_path = Some(format!("/p{}.jpg", record.id));
            let start = Instant::now();
            super::add(&db, &result, n as i64).unwrap();
            adds.push(start.elapsed());
        }
        let refs: Vec<ExternalRef> = dataset[..40]
            .iter()
            .map(|r| item(MediaType::Movie, r.id, "x").external)
            .collect();
        let median = |mut samples: Vec<Duration>| {
            samples.sort();
            samples[samples.len() / 2].as_secs_f64() * 1e3
        };
        println!(
            "add (one transaction each, in memory): median {:.3} ms",
            median(adds)
        );
        let cases = [
            ("all, recently added", query("", None, Sort::RecentlyAdded)),
            ("all, title", query("", None, Sort::Title)),
            (
                "TV only, title",
                query("", Some(MediaType::Tv), Sort::Title),
            ),
            ("search harbor", query("harbor", None, Sort::RecentlyAdded)),
            ("search NÖRDLICHE", query("NÖRDLICHE", None, Sort::Title)),
        ];
        for (label, q) in cases {
            let times: Vec<Duration> = (0..50)
                .map(|_| {
                    let start = Instant::now();
                    std::hint::black_box(search(&db, &q).unwrap());
                    start.elapsed()
                })
                .collect();
            let rows = search(&db, &q).unwrap().len();
            println!("{label:<22} {rows:4} rows: median {:.3} ms", median(times));
        }
        let times: Vec<Duration> = (0..50)
            .map(|_| {
                let start = Instant::now();
                std::hint::black_box(membership(&db, &refs).unwrap());
                start.elapsed()
            })
            .collect();
        println!("membership of 40 results: median {:.3} ms", median(times));
    }

    #[test]
    fn initials_skip_punctuation_and_handle_non_ascii() {
        assert_eq!(initials("Dune: Part Two"), "DP");
        assert_eq!(initials("Shōgun"), "S");
        assert_eq!(initials(""), "");
    }
}
