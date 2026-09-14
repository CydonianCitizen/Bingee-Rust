//! The user's library: media with a `library_entries` row, read from the
//! production database. Plain Rust types, no Slint.

use crate::database::Database;
use crate::error::{AppError, ErrorKind};

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
}

impl LibraryItem {
    pub fn year(&self) -> Option<&str> {
        self.release_date.as_deref().and_then(|date| date.get(..4))
    }
}

/// Library titles whose title or original title contains `query`, ignoring
/// Unicode case, ordered by title. A blank query lists the whole library.
/// Cached media that is not in the library never appears.
pub fn search(db: &Database, query: &str) -> Result<Vec<LibraryItem>, AppError> {
    let failed = |err| AppError::database("Your library could not be searched.", err);
    // ponytail: loads every match. For very large libraries, query ids only
    // and load the visible rows by id.
    let mut stmt = db
        .conn()
        .prepare_cached(
            "SELECT m.local_media_id, m.media_type, m.title, m.original_title,
                    m.release_date, m.overview
             FROM library_entries AS l JOIN media AS m ON m.local_media_id = l.local_media_id
             WHERE instr(bingee_fold(m.title), ?1) > 0
                OR instr(bingee_fold(m.original_title), ?1) > 0
             ORDER BY bingee_fold(m.title), m.local_media_id",
        )
        .map_err(failed)?;
    let rows = stmt
        .query_map([query.trim().to_lowercase()], |row| {
            Ok((
                row.get::<_, String>(1)?,
                LibraryItem {
                    id: row.get(0)?,
                    media_type: MediaType::Movie,
                    title: row.get(2)?,
                    original_title: row.get(3)?,
                    release_date: row.get(4)?,
                    overview: row.get(5)?,
                },
            ))
        })
        .map_err(failed)?;
    rows.map(|row| {
        let (kind, mut item) = row.map_err(failed)?;
        item.media_type = match kind.as_str() {
            "movie" => MediaType::Movie,
            "tv" => MediaType::Tv,
            _ => {
                return Err(AppError::new(
                    ErrorKind::InvalidData,
                    format!("A title in your library has an unknown media type ({kind})."),
                ));
            }
        };
        Ok(item)
    })
    .collect()
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
    use rusqlite::params;

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

    fn titles(db: &Database, query: &str) -> Vec<String> {
        search(db, query)
            .unwrap()
            .into_iter()
            .map(|item| item.title)
            .collect()
    }

    #[test]
    fn empty_library_lists_nothing() {
        let db = Database::open_in_memory();
        assert!(search(&db, "").unwrap().is_empty());
        add(&db, "movie", "Cached, not in the library", None, false);
        assert!(search(&db, "").unwrap().is_empty());
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

        let dark = &search(&db, "dark").unwrap()[0];
        assert_eq!(
            (dark.media_type, dark.year()),
            (MediaType::Tv, Some("2017"))
        );
        assert_eq!(dark.original_title, None);
    }

    #[test]
    fn initials_skip_punctuation_and_handle_non_ascii() {
        assert_eq!(initials("Dune: Part Two"), "DP");
        assert_eq!(initials("Shōgun"), "S");
        assert_eq!(initials(""), "");
    }
}
