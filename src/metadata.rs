//! Provider-owned detail metadata: movie and series details, genres, season
//! summaries and episode metadata, read from and written to SQLite
//! (ADR-0013, ADR-0014, ADR-0015).
//!
//! Three rules hold everywhere in this module:
//!
//! * **Provider only.** Nothing here is personal state. A refresh writes
//!   provider-owned columns and nothing else, with targeted upserts, so a
//!   later watched/rating table cannot be touched by a metadata refresh.
//! * **Freshness is not coverage.** `details_fetched_at` says when the detail
//!   endpoint last answered; `seasons.episodes_fetched_at` and
//!   `episodes_known` say when a season's episode list was last fetched and
//!   how much of it arrived. A season summary can be fresh while its episodes
//!   were never fetched.
//! * **Nothing is invented.** A field the provider did not give stays `None`
//!   and is left out of the UI, never replaced with a zero or a guess.
//!
//! Plain Rust types, no Slint, no network.

use std::sync::Arc;

use rusqlite::{OptionalExtension, Row, Transaction, TransactionBehavior, params};

use crate::database::Database;
use crate::error::{AppError, ErrorKind};
use crate::library::MediaType;
use crate::search::Source;

/// How long stored provider metadata counts as fresh (ADR-0013). Opening a
/// title within this window sends no request; past it, a refresh runs in the
/// background while the cached detail stays on screen. One week is far shorter
/// than a title's metadata usually changes and far longer than a session.
pub const FRESH_FOR: i64 = 7 * 24 * 60 * 60;

/// What is known about the age of one stored timestamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// Never fetched from the provider.
    Never,
    Fresh,
    Stale,
}

impl Freshness {
    /// Whether opening the title should start a background refresh.
    pub fn wants_refresh(self) -> bool {
        self != Freshness::Fresh
    }
}

/// `fetched_at` is Unix seconds, UTC, or `None` if it never happened. A
/// timestamp in the future (a moved system clock) counts as fresh.
pub fn freshness(fetched_at: Option<i64>, now: i64) -> Freshness {
    match fetched_at {
        None => Freshness::Never,
        Some(at) if now.saturating_sub(at) > FRESH_FOR => Freshness::Stale,
        Some(_) => Freshness::Fresh,
    }
}

/// The current time in Unix seconds, injected so freshness is testable
/// without sleeping. The real clock is read in exactly one place.
#[derive(Clone)]
pub struct Clock(Arc<dyn Fn() -> i64 + Send + Sync>);

impl Clock {
    pub fn system() -> Self {
        Self(Arc::new(crate::library::unix_now))
    }

    pub fn now(&self) -> i64 {
        (self.0)()
    }

    /// A clock the test moves by hand.
    #[cfg(test)]
    pub fn fake(start: i64) -> (Self, Arc<std::sync::atomic::AtomicI64>) {
        let seconds = Arc::new(std::sync::atomic::AtomicI64::new(start));
        let read = seconds.clone();
        (
            Self(Arc::new(move || {
                read.load(std::sync::atomic::Ordering::SeqCst)
            })),
            seconds,
        )
    }
}

impl std::fmt::Debug for Clock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Clock({})", self.now())
    }
}

/// One genre at a provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Genre {
    pub id: String,
    pub name: String,
}

/// A title's provider metadata: the fields a search gives plus everything the
/// detail endpoint adds. The same type carries a freshly fetched response and
/// what SQLite stored; `fetched_at` and the per-season coverage fields are
/// `None` on a fetched value and filled on a stored one.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaDetails {
    pub media_type: MediaType,
    pub title: String,
    pub original_title: Option<String>,
    pub overview: Option<String>,
    pub tagline: Option<String>,
    /// `YYYY-MM-DD`; for a series, the first air date.
    pub release_date: Option<String>,
    /// The provider's production/release status, e.g. "Released", "Ended".
    pub status: Option<String>,
    /// A movie's runtime, or a series' typical episode runtime.
    pub runtime_minutes: Option<u32>,
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
    pub genres: Vec<Genre>,
    /// When the detail endpoint last answered, or `None` if it never has.
    pub fetched_at: Option<i64>,
    /// Present exactly for series.
    pub tv: Option<TvDetails>,
}

impl MediaDetails {
    pub fn year(&self) -> Option<&str> {
        self.release_date.as_deref().and_then(|date| date.get(..4))
    }

    /// Genres in one line, for display. Empty when there are none.
    pub fn genre_line(&self) -> String {
        self.genres
            .iter()
            .map(|genre| genre.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }
}

/// What only a series has.
#[derive(Debug, Clone, PartialEq)]
pub struct TvDetails {
    pub last_air_date: Option<String>,
    pub season_count: Option<u32>,
    pub episode_count: Option<u32>,
    /// Season summaries in season order; season 0 (specials) comes first.
    pub seasons: Vec<Season>,
}

/// A season summary. Episodes are fetched separately, per season.
#[derive(Debug, Clone, PartialEq)]
pub struct Season {
    /// 0 is the specials season and is as real as any other.
    pub number: i64,
    pub external_id: Option<String>,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub air_date: Option<String>,
    /// How many episodes the provider says this season has.
    pub episode_count: Option<u32>,
    pub poster_path: Option<String>,
    /// Coverage, not freshness: when the episode list last arrived...
    pub episodes_fetched_at: Option<i64>,
    /// ...and how many episodes it held.
    pub episodes_known: Option<u32>,
}

/// How much of a season's episode list is stored. Deliberately distinct from
/// how old that list is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coverage {
    /// The summary is known; the episode list was never fetched.
    NotFetched,
    /// The last fetch gave every episode the provider counts (or the provider
    /// gave no count to compare with).
    Complete(u32),
    /// The last fetch gave fewer episodes than the provider counts.
    Partial { known: u32, expected: u32 },
}

impl Season {
    pub fn coverage(&self) -> Coverage {
        let Some(known) = self
            .episodes_known
            .filter(|_| self.episodes_fetched_at.is_some())
        else {
            return Coverage::NotFetched;
        };
        match self.episode_count {
            Some(expected) if known < expected => Coverage::Partial { known, expected },
            _ => Coverage::Complete(known),
        }
    }

    /// "Specials", the provider's name, or "Season 3".
    pub fn label(&self) -> String {
        match (self.number, self.name.as_deref()) {
            (_, Some(name)) if !name.is_empty() => name.to_owned(),
            (0, _) => "Specials".to_owned(),
            (number, _) => format!("Season {number}"),
        }
    }

    pub fn year(&self) -> Option<&str> {
        self.air_date.as_deref().and_then(|date| date.get(..4))
    }
}

/// Episode metadata. R9 stores no watched state, rating or progress: those are
/// personal data and get their own tables in a later milestone.
#[derive(Debug, Clone, PartialEq)]
pub struct Episode {
    pub season_number: i64,
    pub number: i64,
    /// The provider's episode id, the stable identity where it exists.
    pub external_id: Option<String>,
    pub name: Option<String>,
    pub overview: Option<String>,
    pub air_date: Option<String>,
    pub runtime_minutes: Option<u32>,
    pub still_path: Option<String>,
}

fn read_failed(err: rusqlite::Error) -> AppError {
    AppError::database("The details of this title could not be read.", err)
}

fn write_failed(err: rusqlite::Error) -> AppError {
    AppError::database("The refreshed details could not be saved.", err)
}

/// Everything stored about one title: its `media` row, its genres and, for a
/// series, its season summaries. Three queries, whatever the season count.
/// `None` when no such title is stored.
pub fn read(db: &Database, id: i64) -> Result<Option<MediaDetails>, AppError> {
    let conn = db.conn();
    let row = conn
        .prepare_cached(
            "SELECT media_type, title, original_title, overview, tagline, release_date, status,
                    runtime_minutes, poster_path, backdrop_path, details_fetched_at,
                    last_air_date, season_count, episode_count
             FROM media WHERE local_media_id = ?1",
        )
        .and_then(|mut stmt| {
            stmt.query_row([id], |row| {
                Ok((row.get::<_, String>(0)?, details_row(row)?, tv_row(row)?))
            })
            .optional()
        })
        .map_err(read_failed)?;
    let Some((kind, mut details, tv)) = row else {
        return Ok(None);
    };
    details.media_type = MediaType::from_key(&kind)?;
    details.genres = read_genres(db, id)?;
    if details.media_type == MediaType::Tv {
        details.tv = Some(TvDetails {
            seasons: read_seasons(db, id)?,
            ..tv
        });
    }
    Ok(Some(details))
}

fn details_row(row: &Row) -> rusqlite::Result<MediaDetails> {
    Ok(MediaDetails {
        media_type: MediaType::Movie, // replaced by the caller after validation
        title: row.get(1)?,
        original_title: row.get(2)?,
        overview: row.get(3)?,
        tagline: row.get(4)?,
        release_date: row.get(5)?,
        status: row.get(6)?,
        runtime_minutes: row.get(7)?,
        poster_path: row.get(8)?,
        backdrop_path: row.get(9)?,
        genres: Vec::new(),
        fetched_at: row.get(10)?,
        tv: None,
    })
}

fn tv_row(row: &Row) -> rusqlite::Result<TvDetails> {
    Ok(TvDetails {
        last_air_date: row.get(11)?,
        season_count: row.get(12)?,
        episode_count: row.get(13)?,
        seasons: Vec::new(),
    })
}

fn read_genres(db: &Database, id: i64) -> Result<Vec<Genre>, AppError> {
    let mut stmt = db
        .conn()
        .prepare_cached(
            "SELECT g.external_id, g.name
             FROM media_genres AS mg
             JOIN genres AS g ON g.source = mg.source AND g.external_id = mg.external_id
             WHERE mg.local_media_id = ?1
             ORDER BY g.name",
        )
        .map_err(read_failed)?;
    let rows = stmt
        .query_map([id], |row| {
            Ok(Genre {
                id: row.get(0)?,
                name: row.get(1)?,
            })
        })
        .map_err(read_failed)?;
    rows.collect::<rusqlite::Result<_>>().map_err(read_failed)
}

fn read_seasons(db: &Database, id: i64) -> Result<Vec<Season>, AppError> {
    let mut stmt = db
        .conn()
        .prepare_cached(
            "SELECT season_number, external_id, name, overview, air_date, episode_count,
                    poster_path, episodes_fetched_at, episodes_known
             FROM seasons WHERE local_media_id = ?1 ORDER BY season_number",
        )
        .map_err(read_failed)?;
    let rows = stmt
        .query_map([id], |row| {
            Ok(Season {
                number: row.get(0)?,
                external_id: row.get(1)?,
                name: row.get(2)?,
                overview: row.get(3)?,
                air_date: row.get(4)?,
                episode_count: row.get(5)?,
                poster_path: row.get(6)?,
                episodes_fetched_at: row.get(7)?,
                episodes_known: row.get(8)?,
            })
        })
        .map_err(read_failed)?;
    rows.collect::<rusqlite::Result<_>>().map_err(read_failed)
}

/// One season's stored episodes, in episode order. Empty when the season's
/// episodes were never fetched — `Season::coverage` tells the two apart.
pub fn read_episodes(db: &Database, id: i64, season: i64) -> Result<Vec<Episode>, AppError> {
    let mut stmt = db
        .conn()
        .prepare_cached(
            "SELECT season_number, episode_number, external_id, name, overview, air_date,
                    runtime_minutes, still_path
             FROM episodes WHERE local_media_id = ?1 AND season_number = ?2
             ORDER BY episode_number",
        )
        .map_err(read_failed)?;
    let rows = stmt
        .query_map(params![id, season], |row| {
            Ok(Episode {
                season_number: row.get(0)?,
                number: row.get(1)?,
                external_id: row.get(2)?,
                name: row.get(3)?,
                overview: row.get(4)?,
                air_date: row.get(5)?,
                runtime_minutes: row.get(6)?,
                still_path: row.get(7)?,
            })
        })
        .map_err(read_failed)?;
    rows.collect::<rusqlite::Result<_>>().map_err(read_failed)
}

/// Stores a fetched detail response for `id` in one transaction: the `media`
/// row's provider fields, the genre set, and — for a series — the season
/// summaries. Membership, `added_at` and each season's episode coverage are
/// never touched. All or nothing.
pub fn save(db: &Database, id: i64, details: &MediaDetails, now: i64) -> Result<(), AppError> {
    let tx = Transaction::new_unchecked(db.conn(), TransactionBehavior::Immediate)
        .map_err(write_failed)?;
    let tv = details.tv.as_ref();
    let updated = tx
        .execute(
            "UPDATE media SET title = ?3, original_title = ?4, overview = ?5, tagline = ?6,
                 release_date = ?7, status = ?8, runtime_minutes = ?9, poster_path = ?10,
                 backdrop_path = ?11, last_air_date = ?12, season_count = ?13,
                 episode_count = ?14, metadata_updated_at = ?15, details_fetched_at = ?15
             WHERE local_media_id = ?1 AND media_type = ?2",
            params![
                id,
                details.media_type.key(),
                details.title,
                details.original_title,
                details.overview,
                details.tagline,
                details.release_date,
                details.status,
                details.runtime_minutes,
                details.poster_path,
                details.backdrop_path,
                tv.and_then(|tv| tv.last_air_date.clone()),
                tv.and_then(|tv| tv.season_count),
                tv.and_then(|tv| tv.episode_count),
                now,
            ],
        )
        .map_err(write_failed)?;
    if updated == 0 {
        return Err(AppError::new(
            ErrorKind::InvalidData,
            "That title is no longer stored on this computer, so its details were not saved.",
        ));
    }
    save_genres(&tx, id, &details.genres)?;
    if let Some(tv) = tv {
        save_seasons(&tx, id, &tv.seasons, now)?;
    }
    tx.commit().map_err(write_failed)
}

/// Replaces the title's genre set. The join rows go first, so a genre the
/// provider dropped disappears; the `genres` table itself only grows, since
/// other titles may still use an entry.
fn save_genres(tx: &Transaction, id: i64, genres: &[Genre]) -> Result<(), AppError> {
    let source = Source::Tmdb.key();
    tx.execute("DELETE FROM media_genres WHERE local_media_id = ?1", [id])
        .map_err(write_failed)?;
    for genre in genres {
        tx.execute(
            "INSERT INTO genres (source, external_id, name) VALUES (?1, ?2, ?3)
             ON CONFLICT (source, external_id) DO UPDATE SET name = excluded.name",
            params![source, genre.id, genre.name],
        )
        .map_err(write_failed)?;
        // A provider that lists the same genre twice still gets one join row.
        tx.execute(
            "INSERT INTO media_genres (local_media_id, source, external_id) VALUES (?1, ?2, ?3)
             ON CONFLICT DO NOTHING",
            params![id, source, genre.id],
        )
        .map_err(write_failed)?;
    }
    Ok(())
}

/// Upserts the season summaries. `episodes_fetched_at` and `episodes_known`
/// are left out of the update on purpose: coverage belongs to the episode
/// fetch, not to the series detail.
fn save_seasons(tx: &Transaction, id: i64, seasons: &[Season], now: i64) -> Result<(), AppError> {
    for season in seasons {
        tx.execute(
            "INSERT INTO seasons (local_media_id, media_type, season_number, external_id, name,
                                  overview, air_date, episode_count, poster_path,
                                  metadata_updated_at)
             VALUES (?1, 'tv', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT (local_media_id, season_number) DO UPDATE SET
                 external_id = excluded.external_id, name = excluded.name,
                 overview = excluded.overview, air_date = excluded.air_date,
                 episode_count = excluded.episode_count, poster_path = excluded.poster_path,
                 metadata_updated_at = excluded.metadata_updated_at",
            params![
                id,
                season.number,
                season.external_id,
                season.name,
                season.overview,
                season.air_date,
                season.episode_count,
                season.poster_path,
                now,
            ],
        )
        .map_err(write_failed)?;
    }
    // Seasons the provider no longer lists go, with their episodes. An empty
    // list is treated as no information rather than as "no seasons", so a
    // truncated answer cannot wipe what is cached.
    if !seasons.is_empty() {
        tx.execute(
            "DELETE FROM seasons WHERE local_media_id = ?1
             AND season_number NOT IN (SELECT value FROM json_each(?2))",
            params![id, numbers(seasons.iter().map(|s| s.number))],
        )
        .map_err(write_failed)?;
    }
    Ok(())
}

/// Stores one season's episode list in its own transaction, so a failure for
/// one season leaves every other season as it was. Episodes the fetch no
/// longer lists are removed first (which also frees a renumbered episode's old
/// row), then every listed episode is upserted, then the season's coverage is
/// recorded. An empty list records coverage without removing anything.
pub fn save_episodes(
    db: &Database,
    id: i64,
    season: i64,
    episodes: &[Episode],
    now: i64,
) -> Result<(), AppError> {
    let tx = Transaction::new_unchecked(db.conn(), TransactionBehavior::Immediate)
        .map_err(write_failed)?;
    // The season row carries the coverage and is the episodes' parent. It
    // exists after a series detail fetch; this keeps a direct season fetch
    // working, and its foreign key still refuses a movie.
    tx.execute(
        "INSERT INTO seasons (local_media_id, media_type, season_number, metadata_updated_at)
         VALUES (?1, 'tv', ?2, ?3) ON CONFLICT (local_media_id, season_number) DO NOTHING",
        params![id, season, now],
    )
    .map_err(write_failed)?;
    if !episodes.is_empty() {
        tx.execute(
            "DELETE FROM episodes WHERE local_media_id = ?1 AND season_number = ?2
             AND episode_number NOT IN (SELECT value FROM json_each(?3))",
            params![id, season, numbers(episodes.iter().map(|e| e.number))],
        )
        .map_err(write_failed)?;
    }
    for episode in episodes {
        // OR REPLACE: the episode number is the key, and a provider that moves
        // an episode id to another number replaces that one row rather than
        // failing the whole season.
        tx.execute(
            "INSERT OR REPLACE INTO episodes (local_media_id, season_number, episode_number,
                 external_id, name, overview, air_date, runtime_minutes, still_path,
                 metadata_updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                id,
                season,
                episode.number,
                episode.external_id,
                episode.name,
                episode.overview,
                episode.air_date,
                episode.runtime_minutes,
                episode.still_path,
                now,
            ],
        )
        .map_err(write_failed)?;
    }
    tx.execute(
        "UPDATE seasons SET episodes_fetched_at = ?3, episodes_known = ?4
         WHERE local_media_id = ?1 AND season_number = ?2",
        params![id, season, now, episodes.len() as i64],
    )
    .map_err(write_failed)?;
    tx.commit().map_err(write_failed)
}

/// A JSON array of numbers, for `json_each` in a NOT IN clause.
fn numbers(values: impl Iterator<Item = i64>) -> String {
    let values: Vec<i64> = values.collect();
    serde_json::to_string(&values).expect("numbers always serialize")
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::library::{self, Added};
    use crate::search::tests::item;

    /// Adds `kind` TMDB `id` to the library and returns its local id.
    pub fn stored(db: &Database, kind: MediaType, id: u32, title: &str) -> i64 {
        match library::add(db, &item(kind, id, title), 1_000).unwrap() {
            Added::New(local) | Added::AlreadyThere(local) => local,
        }
    }

    pub fn genre(id: &str, name: &str) -> Genre {
        Genre {
            id: id.into(),
            name: name.into(),
        }
    }

    pub fn movie(title: &str, genres: Vec<Genre>) -> MediaDetails {
        MediaDetails {
            media_type: MediaType::Movie,
            title: title.into(),
            original_title: Some(title.into()),
            overview: Some("An overview.".into()),
            tagline: Some("A tagline.".into()),
            release_date: Some("1999-03-31".into()),
            status: Some("Released".into()),
            runtime_minutes: Some(136),
            poster_path: Some("/p.jpg".into()),
            backdrop_path: Some("/b.jpg".into()),
            genres,
            fetched_at: None,
            tv: None,
        }
    }

    pub fn season(number: i64, episodes: Option<u32>) -> Season {
        Season {
            number,
            external_id: Some(format!("{}", 3000 + number)),
            name: None,
            overview: Some("About the season.".into()),
            air_date: Some("2011-04-17".into()),
            episode_count: episodes,
            poster_path: None,
            episodes_fetched_at: None,
            episodes_known: None,
        }
    }

    pub fn series(title: &str, seasons: Vec<Season>) -> MediaDetails {
        let episodes = seasons.iter().filter_map(|s| s.episode_count).sum();
        MediaDetails {
            media_type: MediaType::Tv,
            title: title.into(),
            original_title: None,
            overview: Some("About the series.".into()),
            tagline: None,
            release_date: Some("2011-04-17".into()),
            status: Some("Ended".into()),
            runtime_minutes: Some(57),
            poster_path: Some("/s.jpg".into()),
            backdrop_path: None,
            genres: vec![genre("18", "Drama")],
            fetched_at: None,
            tv: Some(TvDetails {
                last_air_date: Some("2019-05-19".into()),
                season_count: Some(seasons.len() as u32),
                episode_count: Some(episodes),
                seasons,
            }),
        }
    }

    pub fn episode(season: i64, number: i64) -> Episode {
        Episode {
            season_number: season,
            number,
            external_id: Some(format!("{}{number:02}", 60000 + season)),
            name: Some(format!("Episode {number}")),
            overview: Some("Things happen.".into()),
            air_date: Some("2011-04-17".into()),
            runtime_minutes: Some(62),
            still_path: Some(format!("/s{season}e{number}.jpg")),
        }
    }

    fn seasons_of(db: &Database, id: i64) -> Vec<Season> {
        match read(db, id).unwrap().unwrap().tv {
            Some(tv) => tv.seasons,
            None => Vec::new(),
        }
    }

    fn rows(db: &Database, table: &str) -> i64 {
        db.conn()
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    #[test]
    fn freshness_has_three_states_around_the_interval() {
        assert_eq!(freshness(None, 1_000), Freshness::Never);
        assert_eq!(freshness(Some(1_000), 1_000), Freshness::Fresh);
        assert_eq!(freshness(Some(1_000), 1_000 + FRESH_FOR), Freshness::Fresh);
        assert_eq!(
            freshness(Some(1_000), 1_001 + FRESH_FOR),
            Freshness::Stale,
            "one second past the window"
        );
        // A clock that moved backwards must not make everything stale.
        assert_eq!(freshness(Some(9_000), 1_000), Freshness::Fresh);
        assert!(Freshness::Never.wants_refresh() && Freshness::Stale.wants_refresh());
        assert!(!Freshness::Fresh.wants_refresh());
    }

    #[test]
    fn the_clock_is_injectable() {
        let (clock, seconds) = Clock::fake(100);
        assert_eq!(clock.now(), 100);
        seconds.store(500, std::sync::atomic::Ordering::SeqCst);
        assert_eq!(clock.now(), 500);
        assert!(Clock::system().now() > 1_700_000_000);
    }

    #[test]
    fn a_title_without_details_reads_back_what_the_search_gave() {
        let db = Database::open_in_memory();
        let id = stored(&db, MediaType::Movie, 603, "The Matrix");
        let details = read(&db, id).unwrap().unwrap();
        assert_eq!(details.media_type, MediaType::Movie);
        assert_eq!(details.title, "The Matrix");
        assert_eq!(details.fetched_at, None, "detail was never fetched");
        assert_eq!(freshness(details.fetched_at, 0), Freshness::Never);
        assert!(details.genres.is_empty() && details.tv.is_none());
        assert_eq!(details.runtime_minutes, None, "nothing is invented");
        assert_eq!(read(&db, 9999).unwrap(), None);
    }

    #[test]
    fn movie_details_persist_and_refresh_in_place() {
        let db = Database::open_in_memory();
        let id = stored(&db, MediaType::Movie, 603, "The Matrix");
        let first = movie(
            "The Matrix",
            vec![genre("28", "Action"), genre("878", "Sci-Fi")],
        );
        save(&db, id, &first, 5_000).unwrap();

        let stored_details = read(&db, id).unwrap().unwrap();
        assert_eq!(stored_details.fetched_at, Some(5_000));
        assert_eq!(stored_details.runtime_minutes, Some(136));
        assert_eq!(stored_details.status.as_deref(), Some("Released"));
        assert_eq!(stored_details.tagline.as_deref(), Some("A tagline."));
        assert_eq!(stored_details.genre_line(), "Action, Sci-Fi");
        assert_eq!(stored_details.year(), Some("1999"));
        assert_eq!(
            MediaDetails {
                fetched_at: Some(5_000),
                ..first
            },
            stored_details
        );

        // A later refresh replaces the provider's own fields.
        let mut newer = movie("The Matrix", vec![genre("878", "Science Fiction")]);
        newer.tagline = None;
        newer.runtime_minutes = Some(150);
        save(&db, id, &newer, 9_000).unwrap();
        let after = read(&db, id).unwrap().unwrap();
        assert_eq!(after.tagline, None, "a dropped field is not kept alive");
        assert_eq!(after.runtime_minutes, Some(150));
        assert_eq!(after.genre_line(), "Science Fiction");
        assert_eq!(after.fetched_at, Some(9_000));
        // Library membership and the added date are untouched by a refresh.
        let added: i64 = db
            .conn()
            .query_row("SELECT added_at FROM library_entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(added, 1_000);
    }

    #[test]
    fn genres_are_relational_deduplicated_and_replaceable() {
        let db = Database::open_in_memory();
        let one = stored(&db, MediaType::Movie, 603, "The Matrix");
        let two = stored(&db, MediaType::Movie, 604, "Reloaded");

        // The same provider genre id twice in one payload: one join row.
        let dupes = vec![
            genre("28", "Action"),
            genre("28", "Action"),
            genre("18", "Drama"),
        ];
        save(&db, one, &movie("The Matrix", dupes), 1).unwrap();
        assert_eq!(rows(&db, "media_genres"), 2);
        assert_eq!(rows(&db, "genres"), 2);

        // A shared genre is stored once and used by both titles.
        save(&db, two, &movie("Reloaded", vec![genre("28", "Action")]), 1).unwrap();
        assert_eq!((rows(&db, "genres"), rows(&db, "media_genres")), (2, 3));

        // Refreshing with the same set changes nothing.
        let same = vec![genre("28", "Action"), genre("18", "Drama")];
        save(&db, one, &movie("The Matrix", same), 2).unwrap();
        assert_eq!((rows(&db, "genres"), rows(&db, "media_genres")), (2, 3));

        // A different set replaces it; a renamed genre is renamed everywhere.
        let other = vec![genre("28", "Action & Adventure"), genre("53", "Thriller")];
        save(&db, one, &movie("The Matrix", other), 3).unwrap();
        assert_eq!(
            read(&db, one).unwrap().unwrap().genre_line(),
            "Action & Adventure, Thriller"
        );
        assert_eq!(
            read(&db, two).unwrap().unwrap().genre_line(),
            "Action & Adventure"
        );

        // An empty set is a valid answer.
        save(&db, one, &movie("The Matrix", Vec::new()), 4).unwrap();
        assert_eq!(read(&db, one).unwrap().unwrap().genres, Vec::new());
        assert_eq!(
            rows(&db, "media_genres"),
            1,
            "the other title keeps its own"
        );
    }

    #[test]
    fn series_details_store_season_summaries_including_specials() {
        let db = Database::open_in_memory();
        let id = stored(&db, MediaType::Tv, 1396, "Game of Thrones");
        let mut specials = season(0, Some(3));
        specials.air_date = None;
        save(
            &db,
            id,
            &series("Game of Thrones", vec![specials, season(1, Some(10))]),
            100,
        )
        .unwrap();

        let details = read(&db, id).unwrap().unwrap();
        let tv = details.tv.as_ref().unwrap();
        assert_eq!(tv.last_air_date.as_deref(), Some("2019-05-19"));
        assert_eq!((tv.season_count, tv.episode_count), (Some(2), Some(13)));
        let numbers: Vec<i64> = tv.seasons.iter().map(|s| s.number).collect();
        assert_eq!(numbers, [0, 1], "season 0 is stored and listed first");
        assert_eq!(tv.seasons[0].label(), "Specials");
        assert_eq!(tv.seasons[1].label(), "Season 1");
        assert_eq!(tv.seasons[0].air_date, None);
        assert_eq!(tv.seasons[1].year(), Some("2011"));
        assert_eq!(tv.seasons[0].coverage(), Coverage::NotFetched);

        // A named season keeps the provider's name.
        let mut named = season(1, Some(10));
        named.name = Some("The Beginning".into());
        save(&db, id, &series("Game of Thrones", vec![named]), 200).unwrap();
        let seasons = seasons_of(&db, id);
        assert_eq!(seasons.len(), 1, "a season the provider dropped is gone");
        assert_eq!(seasons[0].label(), "The Beginning");
    }

    #[test]
    fn a_series_detail_refresh_keeps_episode_coverage() {
        let db = Database::open_in_memory();
        let id = stored(&db, MediaType::Tv, 1396, "Game of Thrones");
        save(
            &db,
            id,
            &series("Game of Thrones", vec![season(1, Some(2))]),
            100,
        )
        .unwrap();
        save_episodes(&db, id, 1, &[episode(1, 1), episode(1, 2)], 150).unwrap();
        assert_eq!(seasons_of(&db, id)[0].coverage(), Coverage::Complete(2));

        // The series detail says the season grew. Coverage is not freshness:
        // the stored episodes stay, and the season is now partially covered.
        save(
            &db,
            id,
            &series("Game of Thrones", vec![season(1, Some(4))]),
            900,
        )
        .unwrap();
        let season = seasons_of(&db, id).remove(0);
        assert_eq!(season.episodes_fetched_at, Some(150), "coverage untouched");
        assert_eq!(season.episode_count, Some(4), "the summary is refreshed");
        assert_eq!(
            season.coverage(),
            Coverage::Partial {
                known: 2,
                expected: 4
            }
        );
        assert_eq!(read_episodes(&db, id, 1).unwrap().len(), 2);
    }

    #[test]
    fn episodes_persist_are_idempotent_and_survive_repeated_fetches() {
        let db = Database::open_in_memory();
        let id = stored(&db, MediaType::Tv, 1396, "Game of Thrones");
        save(
            &db,
            id,
            &series(
                "Game of Thrones",
                vec![season(0, Some(1)), season(1, Some(3))],
            ),
            10,
        )
        .unwrap();

        let first = [episode(1, 1), episode(1, 2), episode(1, 3)];
        save_episodes(&db, id, 1, &first, 20).unwrap();
        assert_eq!(read_episodes(&db, id, 1).unwrap(), first);
        assert_eq!(seasons_of(&db, id)[1].coverage(), Coverage::Complete(3));

        // The same fetch again: no duplicates, no change.
        save_episodes(&db, id, 1, &first, 30).unwrap();
        assert_eq!(read_episodes(&db, id, 1).unwrap(), first);
        assert_eq!(rows(&db, "episodes"), 3);

        // Season 0 is its own list.
        save_episodes(&db, id, 0, &[episode(0, 1)], 40).unwrap();
        assert_eq!(read_episodes(&db, id, 0).unwrap().len(), 1);
        assert_eq!(read_episodes(&db, id, 1).unwrap().len(), 3);
        assert_eq!(rows(&db, "episodes"), 4);
    }

    #[test]
    fn an_episode_list_that_changed_is_reconciled_not_rebuilt() {
        let db = Database::open_in_memory();
        let id = stored(&db, MediaType::Tv, 1396, "Show");
        save(&db, id, &series("Show", vec![season(1, Some(3))]), 10).unwrap();
        save_episodes(
            &db,
            id,
            1,
            &[episode(1, 1), episode(1, 2), episode(1, 3)],
            20,
        )
        .unwrap();

        // The season grew: the three cached episodes still correspond.
        let mut grown: Vec<Episode> = (1..=5).map(|n| episode(1, n)).collect();
        grown[0].name = Some("Winter Is Coming".into());
        save_episodes(&db, id, 1, &grown, 30).unwrap();
        let after = read_episodes(&db, id, 1).unwrap();
        assert_eq!(after.len(), 5);
        assert_eq!(after[0].name.as_deref(), Some("Winter Is Coming"));

        // The season shrank: the episodes it no longer lists go.
        save_episodes(&db, id, 1, &grown[..2], 40).unwrap();
        let numbers: Vec<i64> = read_episodes(&db, id, 1)
            .unwrap()
            .iter()
            .map(|e| e.number)
            .collect();
        assert_eq!(numbers, [1, 2]);

        // An empty answer never wipes a cached list; it only records coverage.
        save_episodes(&db, id, 1, &[], 50).unwrap();
        assert_eq!(read_episodes(&db, id, 1).unwrap().len(), 2, "kept");
        let season = seasons_of(&db, id).remove(0);
        assert_eq!(season.episodes_fetched_at, Some(50));
        assert_eq!(
            season.coverage(),
            Coverage::Partial {
                known: 0,
                expected: 3
            }
        );
    }

    #[test]
    fn a_provider_episode_id_cannot_appear_twice_in_a_season() {
        let db = Database::open_in_memory();
        let id = stored(&db, MediaType::Tv, 1396, "Show");
        save(&db, id, &series("Show", vec![season(1, Some(2))]), 10).unwrap();
        save_episodes(&db, id, 1, &[episode(1, 1), episode(1, 2)], 20).unwrap();

        // The provider moved the id of episode 1 onto episode 2 and dropped
        // episode 1: one row survives, under the new number.
        let mut moved = episode(1, 2);
        moved.external_id = episode(1, 1).external_id;
        save_episodes(&db, id, 1, &[moved.clone()], 30).unwrap();
        assert_eq!(read_episodes(&db, id, 1).unwrap(), [moved]);

        // A malformed payload that repeats one provider id at two numbers is
        // deduplicated before it gets here (`tmdb::episodes`); should one get
        // through anyway, the unique index still leaves a single row rather
        // than two copies of one episode.
        let mut clash = episode(1, 7);
        clash.external_id = episode(1, 1).external_id;
        save_episodes(&db, id, 1, &[episode(1, 1), clash.clone()], 40).unwrap();
        assert_eq!(read_episodes(&db, id, 1).unwrap(), [clash]);
    }

    #[test]
    fn nullable_provider_fields_stay_empty() {
        let db = Database::open_in_memory();
        let id = stored(&db, MediaType::Tv, 1396, "Show");
        let mut bare = season(1, None);
        bare.overview = None;
        bare.air_date = None;
        bare.external_id = None;
        let mut details = series("Show", vec![bare]);
        details.overview = None;
        details.status = None;
        details.runtime_minutes = None;
        details.poster_path = None;
        if let Some(tv) = details.tv.as_mut() {
            tv.last_air_date = None;
            tv.episode_count = None;
        }
        save(&db, id, &details, 10).unwrap();

        let mut empty = episode(1, 1);
        empty.name = None;
        empty.overview = None;
        empty.air_date = None;
        empty.runtime_minutes = None;
        empty.still_path = None;
        empty.external_id = None;
        save_episodes(&db, id, 1, &[empty.clone()], 20).unwrap();

        let read_back = read(&db, id).unwrap().unwrap();
        assert_eq!(read_back.runtime_minutes, None);
        let season = &read_back.tv.as_ref().unwrap().seasons[0];
        assert_eq!(
            (season.air_date.as_deref(), season.episode_count),
            (None, None)
        );
        assert_eq!(
            season.coverage(),
            Coverage::Complete(1),
            "no count to fall short of"
        );
        assert_eq!(read_episodes(&db, id, 1).unwrap(), [empty]);
        assert_eq!(read_back.genre_line(), "Drama");
    }

    #[test]
    fn details_cannot_be_saved_for_the_wrong_media_type_or_a_missing_title() {
        let db = Database::open_in_memory();
        let movie_id = stored(&db, MediaType::Movie, 603, "The Matrix");
        // A series payload against a movie row updates nothing.
        let error = save(
            &db,
            movie_id,
            &series("Wrong", vec![season(1, Some(1))]),
            10,
        )
        .unwrap_err();
        assert_eq!(error.kind, ErrorKind::InvalidData);
        assert_eq!(rows(&db, "seasons"), 0);
        assert_eq!(read(&db, movie_id).unwrap().unwrap().title, "The Matrix");
        assert!(save(&db, 4242, &movie("Ghost", Vec::new()), 10).is_err());
        // Seasons never attach to a movie, whichever way in.
        assert!(save_episodes(&db, movie_id, 1, &[episode(1, 1)], 10).is_err());
        assert_eq!(rows(&db, "episodes"), 0);
    }

    #[test]
    fn removing_a_title_from_the_library_keeps_its_details() {
        let db = Database::open_in_memory();
        let id = stored(&db, MediaType::Tv, 1396, "Show");
        save(&db, id, &series("Show", vec![season(1, Some(1))]), 10).unwrap();
        save_episodes(&db, id, 1, &[episode(1, 1)], 20).unwrap();
        assert!(library::remove(&db, id).unwrap());
        let details = read(&db, id).unwrap().unwrap();
        assert_eq!(details.fetched_at, Some(10));
        assert_eq!(read_episodes(&db, id, 1).unwrap().len(), 1);
    }

    /// Informal R9 observation, not a `BENCHMARK_SPEC.md` run: detail reads
    /// and writes for a movie, a 60-season series and a 250-episode season,
    /// in a file-backed database. Run with
    /// `cargo test --release informal_detail_timings -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn informal_detail_timings() {
        use crate::diagnostics::Log;
        use crate::paths::TestDir;
        use std::time::{Duration, Instant};

        let dir = TestDir::new("metadata-timings");
        let db = Database::open(&dir.0.join("bingee.db"), &Log::stderr_only()).unwrap();
        let movie_id = stored(&db, MediaType::Movie, 603, "The Matrix");
        let series_id = stored(&db, MediaType::Tv, 1, "Long Series");
        let genres: Vec<Genre> = (1..=5)
            .map(|n| genre(&n.to_string(), &format!("G{n}")))
            .collect();
        let many_seasons: Vec<Season> = (0..60).map(|n| season(n, Some(250))).collect();
        let episodes: Vec<Episode> = (1..=250).map(|n| episode(1, n)).collect();
        save(
            &db,
            series_id,
            &series("Long Series", many_seasons.clone()),
            1,
        )
        .unwrap();
        for number in 0..60 {
            let list: Vec<Episode> = (1..=250).map(|n| episode(number, n)).collect();
            save_episodes(&db, series_id, number, &list, 1).unwrap();
        }

        let median = |mut samples: Vec<Duration>| {
            samples.sort();
            samples[samples.len() / 2].as_secs_f64() * 1e3
        };
        let time = |label: &str, runs: usize, f: &mut dyn FnMut()| {
            let samples: Vec<Duration> = (0..runs)
                .map(|_| {
                    let start = Instant::now();
                    f();
                    start.elapsed()
                })
                .collect();
            println!("{label:<44} median {:.3} ms ({runs} runs)", median(samples));
        };
        let mut movie_details = movie("The Matrix", genres);
        save(&db, movie_id, &movie_details, 1).unwrap();
        time("read movie detail", 200, &mut || {
            std::hint::black_box(read(&db, movie_id).unwrap());
        });
        time("read series detail, 60 seasons", 200, &mut || {
            std::hint::black_box(read(&db, series_id).unwrap());
        });
        time("read one season, 250 episodes", 200, &mut || {
            std::hint::black_box(read_episodes(&db, series_id, 1).unwrap());
        });
        time("two season switches (read 2 seasons)", 200, &mut || {
            std::hint::black_box(read_episodes(&db, series_id, 2).unwrap());
            std::hint::black_box(read_episodes(&db, series_id, 3).unwrap());
        });
        time("save movie detail (commit)", 50, &mut || {
            movie_details.runtime_minutes = movie_details.runtime_minutes.map(|m| m % 200 + 1);
            save(&db, movie_id, &movie_details, 2).unwrap();
        });
        time("save series detail, 60 seasons (commit)", 50, &mut || {
            save(
                &db,
                series_id,
                &series("Long Series", many_seasons.clone()),
                2,
            )
            .unwrap();
        });
        time("save 250 episodes, unchanged (commit)", 50, &mut || {
            save_episodes(&db, series_id, 1, &episodes, 2).unwrap();
        });
    }

    /// The whole detail layer survives a close and reopen, offline.
    #[test]
    fn everything_survives_a_restart() {
        use crate::diagnostics::Log;
        use crate::paths::TestDir;
        let dir = TestDir::new("metadata-restart");
        let path = dir.0.join("bingee.db");
        let log = Log::stderr_only();
        let id = {
            let db = Database::open(&path, &log).unwrap();
            let id = stored(&db, MediaType::Tv, 1396, "Show");
            save(
                &db,
                id,
                &series("Show", vec![season(0, Some(1)), season(1, Some(2))]),
                10,
            )
            .unwrap();
            save_episodes(&db, id, 1, &[episode(1, 1), episode(1, 2)], 20).unwrap();
            id
        };
        let db = Database::open(&path, &log).unwrap();
        let details = read(&db, id).unwrap().unwrap();
        assert_eq!(details.title, "Show");
        assert_eq!(details.fetched_at, Some(10));
        let seasons = details.tv.unwrap().seasons;
        assert_eq!(seasons[0].coverage(), Coverage::NotFetched, "specials");
        assert_eq!(seasons[1].coverage(), Coverage::Complete(2));
        assert_eq!(read_episodes(&db, id, 1).unwrap().len(), 2);
        assert!(read_episodes(&db, id, 0).unwrap().is_empty());
    }
}
