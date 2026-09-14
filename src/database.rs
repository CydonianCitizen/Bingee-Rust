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
const MIGRATIONS: &[&str] = &[SCHEMA_V1];

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
    fn fresh_database_migrates_to_v1() {
        let dir = TestDir::new("db-fresh");
        let db = open(&dir.0.join("bingee.db")).unwrap();
        assert_eq!(MIGRATIONS.len(), 1);
        assert_eq!(db.schema_version().unwrap(), 1);
        let app_id: i32 = db
            .conn()
            .pragma_query_value(None, "application_id", |r| r.get(0))
            .unwrap();
        assert_eq!(app_id, APPLICATION_ID);
        assert_eq!(
            tables(db.conn()),
            [
                "external_refs",
                "library_entries",
                "media",
                "sqlite_sequence"
            ]
        );
        let foreign_keys: bool = db
            .conn()
            .pragma_query_value(None, "foreign_keys", |r| r.get(0))
            .unwrap();
        assert!(foreign_keys);
    }

    #[test]
    fn reopening_v1_changes_nothing_and_keeps_data() {
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
            assert_eq!(db.schema_version().unwrap(), 1);
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
        let broken: &[&str] = &[SCHEMA_V1, "CREATE TABLE extra (x); THIS IS NOT SQL"];
        let dir = TestDir::new("db-rollback");

        // Fresh file, v1 + a failing v2 in one transaction: nothing remains.
        let path = dir.0.join("fresh.db");
        let conn = Connection::open(&path).unwrap();
        let error = Database::init(conn, broken, &Log::stderr_only())
            .err()
            .unwrap();
        assert_eq!(error.kind, ErrorKind::Database);
        let conn = Connection::open(&path).unwrap();
        assert_eq!(user_version(&conn).unwrap(), 0);
        assert!(tables(&conn).is_empty());

        // A v1 file with data: the failed step leaves it at v1, unchanged.
        let path = dir.0.join("v1.db");
        let id = insert_media(open(&path).unwrap().conn(), "movie", "Kept");
        let before = std::fs::read(&path).unwrap();
        let conn = Connection::open(&path).unwrap();
        assert!(Database::init(conn, broken, &Log::stderr_only()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
        let db = open(&path).unwrap();
        assert_eq!(db.schema_version().unwrap(), 1);
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
            .pragma_update(None, "user_version", 2)
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
