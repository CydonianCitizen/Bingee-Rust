//! Personal tracking: watched state, ratings, watch history and the progress
//! derived from them (ADR-0016, ADR-0017, ADR-0018).
//!
//! Rules that hold everywhere in this module:
//!
//! * **Personal only.** These functions write `media_tracking`,
//!   `episode_tracking` and `watch_events`, never provider tables, and
//!   provider code never writes these. Nothing here uses the network.
//! * **State is not history.** Marking watched sets state and records one
//!   event only if the state changed (or for an explicit "watch again").
//!   Unmarking never deletes events; deleting an event never changes state.
//! * **Progress is coverage-aware.** Specials (season 0) are tracked but kept
//!   out of the main count, and a series is complete only when every regular
//!   season's episode list is fully downloaded and watched.
//! * **One transaction per action**, with set-based SQL for whole seasons.
//!
//! Plain Rust types, no Slint.

use std::collections::{HashMap, HashSet};

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

use crate::database::Database;
use crate::error::{AppError, ErrorKind};
use crate::library::MediaType;
use crate::metadata::{self, Coverage};

/// The personal rating scale: whole numbers from 1 to 10.
pub const RATING_MAX: u8 = 10;

fn read_failed(err: rusqlite::Error) -> AppError {
    AppError::database("Your watch progress could not be read.", err)
}

fn write_failed(err: rusqlite::Error) -> AppError {
    AppError::database(
        "Your change could not be saved. Your watch progress is as it was.",
        err,
    )
}

fn missing(what: &str) -> AppError {
    AppError::new(
        ErrorKind::InvalidData,
        format!("{what} is no longer stored on this computer, so nothing was changed."),
    )
}

fn begin(db: &Database) -> Result<Transaction<'_>, AppError> {
    Transaction::new_unchecked(db.conn(), TransactionBehavior::Immediate).map_err(write_failed)
}

/// The title's type, or an error if it is not stored or not `expected`.
fn require(conn: &Connection, id: i64, expected: MediaType) -> Result<(), AppError> {
    let kind: Option<String> = conn
        .query_row(
            "SELECT media_type FROM media WHERE local_media_id = ?1",
            [id],
            |row| row.get(0),
        )
        .optional()
        .map_err(write_failed)?;
    match kind {
        None => Err(missing("That title")),
        Some(kind) if MediaType::from_key(&kind)? == expected => Ok(()),
        Some(_) => Err(AppError::new(
            ErrorKind::InvalidData,
            match expected {
                MediaType::Movie => "Only a movie can be marked watched as a whole.",
                MediaType::Tv => "Only a series has episodes to mark watched.",
            },
        )),
    }
}

// Titles

/// A title's own personal state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TitleState {
    /// When a movie was last watched (Unix seconds, UTC); `None` if unwatched.
    pub watched_at: Option<i64>,
    pub rating: Option<u8>,
}

pub fn title_state(db: &Database, id: i64) -> Result<TitleState, AppError> {
    db.conn()
        .prepare_cached("SELECT watched_at, rating FROM media_tracking WHERE local_media_id = ?1")
        .and_then(|mut stmt| {
            stmt.query_row([id], |row| {
                Ok(TitleState {
                    watched_at: row.get(0)?,
                    rating: row.get(1)?,
                })
            })
            .optional()
        })
        .map(Option::unwrap_or_default)
        .map_err(read_failed)
}

/// Marks a movie watched at `now`. Already watched: nothing changes, unless
/// `again` records a rewatch. Returns whether a watch event was recorded.
pub fn watch_movie(db: &Database, id: i64, now: i64, again: bool) -> Result<bool, AppError> {
    let tx = begin(db)?;
    require(&tx, id, MediaType::Movie)?;
    let changed = tx
        .execute(
            "INSERT INTO media_tracking (local_media_id, media_type, watched_at)
             VALUES (?1, 'movie', ?2)
             ON CONFLICT (local_media_id) DO UPDATE SET watched_at = excluded.watched_at
             WHERE ?3 OR watched_at IS NULL",
            params![id, now, again],
        )
        .map_err(write_failed)?;
    if changed > 0 {
        tx.execute(
            "INSERT INTO watch_events (local_media_id, media_type, watched_at)
             VALUES (?1, 'movie', ?2)",
            params![id, now],
        )
        .map_err(write_failed)?;
    }
    tx.commit().map_err(write_failed)?;
    Ok(changed > 0)
}

/// Marks a movie unwatched. Its history stays. Returns whether it was watched.
pub fn unwatch_movie(db: &Database, id: i64) -> Result<bool, AppError> {
    db.conn()
        .execute(
            "UPDATE media_tracking SET watched_at = NULL
             WHERE local_media_id = ?1 AND watched_at IS NOT NULL",
            [id],
        )
        .map(|changed| changed > 0)
        .map_err(write_failed)
}

/// Sets (1..=10) or clears the user's rating of a movie or series. Does not
/// change watched state.
pub fn set_rating(db: &Database, id: i64, rating: Option<u8>) -> Result<(), AppError> {
    if rating.is_some_and(|r| !(1..=RATING_MAX).contains(&r)) {
        return Err(AppError::new(
            ErrorKind::InvalidData,
            format!("A rating is a whole number from 1 to {RATING_MAX}."),
        ));
    }
    let changed = db
        .conn()
        .execute(
            "INSERT INTO media_tracking (local_media_id, media_type, rating)
             SELECT local_media_id, media_type, ?2 FROM media WHERE local_media_id = ?1
             ON CONFLICT (local_media_id) DO UPDATE SET rating = excluded.rating",
            params![id, rating],
        )
        .map_err(write_failed)?;
    match changed {
        0 => Err(missing("That title")),
        _ => Ok(()),
    }
}

// Episodes

/// Marks one stored episode watched at `now`. Already watched: nothing
/// changes, unless `again` records a rewatch. Returns whether an event was
/// recorded.
pub fn watch_episode(
    db: &Database,
    id: i64,
    season: i64,
    episode: i64,
    now: i64,
    again: bool,
) -> Result<bool, AppError> {
    let tx = begin(db)?;
    require(&tx, id, MediaType::Tv)?;
    let known: bool = tx
        .query_row(
            "SELECT EXISTS (SELECT 1 FROM episodes
             WHERE local_media_id = ?1 AND season_number = ?2 AND episode_number = ?3)",
            params![id, season, episode],
            |row| row.get(0),
        )
        .map_err(write_failed)?;
    if !known {
        return Err(missing("That episode"));
    }
    let changed = tx
        .execute(
            "INSERT INTO episode_tracking VALUES (?1, 'tv', ?2, ?3, ?4)
             ON CONFLICT (local_media_id, season_number, episode_number)
             DO UPDATE SET watched_at = excluded.watched_at WHERE ?5",
            params![id, season, episode, now, again],
        )
        .map_err(write_failed)?;
    if changed > 0 {
        tx.execute(
            "INSERT INTO watch_events (local_media_id, media_type, season_number, episode_number,
                                       watched_at)
             VALUES (?1, 'tv', ?2, ?3, ?4)",
            params![id, season, episode, now],
        )
        .map_err(write_failed)?;
    }
    tx.commit().map_err(write_failed)?;
    Ok(changed > 0)
}

/// Marks one episode unwatched; its history stays. Returns whether it was
/// watched.
pub fn unwatch_episode(
    db: &Database,
    id: i64,
    season: i64,
    episode: i64,
) -> Result<bool, AppError> {
    db.conn()
        .execute(
            "DELETE FROM episode_tracking
             WHERE local_media_id = ?1 AND season_number = ?2 AND episode_number = ?3",
            params![id, season, episode],
        )
        .map(|deleted| deleted > 0)
        .map_err(write_failed)
}

/// Marks every stored episode of a season watched in one transaction: the
/// unwatched ones get `now` and one event each (in episode order); watched
/// ones are left as they are. Returns how many were newly marked.
pub fn watch_season(db: &Database, id: i64, season: i64, now: i64) -> Result<usize, AppError> {
    let tx = begin(db)?;
    require(&tx, id, MediaType::Tv)?;
    let events = tx
        .execute(
            "INSERT INTO watch_events (local_media_id, media_type, season_number, episode_number,
                                       watched_at)
             SELECT e.local_media_id, 'tv', e.season_number, e.episode_number, ?3
             FROM episodes AS e
             WHERE e.local_media_id = ?1 AND e.season_number = ?2
               AND NOT EXISTS (SELECT 1 FROM episode_tracking AS t
                   WHERE t.local_media_id = e.local_media_id
                     AND t.season_number = e.season_number
                     AND t.episode_number = e.episode_number)
             ORDER BY e.episode_number",
            params![id, season, now],
        )
        .map_err(write_failed)?;
    let marked = tx
        .execute(
            "INSERT INTO episode_tracking
             SELECT local_media_id, 'tv', season_number, episode_number, ?3 FROM episodes
             WHERE local_media_id = ?1 AND season_number = ?2
             ON CONFLICT DO NOTHING",
            params![id, season, now],
        )
        .map_err(write_failed)?;
    if marked != events {
        // Both statements select the same episodes inside one write lock.
        return Err(AppError::new(
            ErrorKind::Internal,
            "The season could not be marked watched consistently. Nothing was changed.",
        ));
    }
    tx.commit().map_err(write_failed)?;
    Ok(marked)
}

/// Marks every stored episode of a season unwatched; history stays. Returns
/// how many were watched.
pub fn unwatch_season(db: &Database, id: i64, season: i64) -> Result<usize, AppError> {
    db.conn()
        .execute(
            "DELETE FROM episode_tracking
             WHERE local_media_id = ?1 AND season_number = ?2
               AND episode_number IN (SELECT episode_number FROM episodes
                                      WHERE local_media_id = ?1 AND season_number = ?2)",
            params![id, season],
        )
        .map_err(write_failed)
}

/// The watched episode numbers of one season, in one query.
pub fn watched_episodes(db: &Database, id: i64, season: i64) -> Result<HashSet<i64>, AppError> {
    let mut stmt = db
        .conn()
        .prepare_cached(
            "SELECT episode_number FROM episode_tracking
             WHERE local_media_id = ?1 AND season_number = ?2",
        )
        .map_err(read_failed)?;
    let rows = stmt
        .query_map(params![id, season], |row| row.get(0))
        .map_err(read_failed)?;
    rows.collect::<rusqlite::Result<_>>().map_err(read_failed)
}

// Progress

/// One season's progress over its locally stored episodes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SeasonProgress {
    pub number: i64,
    /// Episodes stored locally.
    pub known: u32,
    /// Of those, watched. Tracking of episodes no longer stored does not count.
    pub watched: u32,
    /// The episode list was downloaded and holds every episode the provider
    /// counts (ADR-0015).
    pub covered: bool,
}

impl SeasonProgress {
    pub fn is_specials(&self) -> bool {
        self.number == 0
    }
}

/// A series' progress. Specials are reported apart and never count toward
/// the main progress, completion or next episode (ADR-0018).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SeriesProgress {
    /// The series detail (and so its season list) was downloaded.
    pub details_fetched: bool,
    /// In season order.
    pub seasons: Vec<SeasonProgress>,
}

impl SeriesProgress {
    fn regular(&self) -> impl Iterator<Item = &SeasonProgress> {
        self.seasons.iter().filter(|season| !season.is_specials())
    }

    /// Watched regular episodes.
    pub fn watched(&self) -> u32 {
        self.regular().map(|season| season.watched).sum()
    }

    /// Stored regular episodes: the denominator, which is not TMDB's count.
    pub fn known(&self) -> u32 {
        self.regular().map(|season| season.known).sum()
    }

    /// (watched, known) specials, when any are stored.
    pub fn specials(&self) -> Option<(u32, u32)> {
        self.seasons
            .iter()
            .find(|season| season.is_specials() && season.known > 0)
            .map(|season| (season.watched, season.known))
    }

    /// Every regular season is known and fully downloaded.
    pub fn coverage_complete(&self) -> bool {
        self.details_fetched
            && self.regular().next().is_some()
            && self.regular().all(|season| season.covered)
    }

    /// Watched through: complete coverage, and every regular episode watched.
    /// `watched == known` alone is never enough.
    pub fn is_complete(&self) -> bool {
        self.coverage_complete() && self.known() > 0 && self.watched() == self.known()
    }

    pub fn season(&self, number: i64) -> Option<&SeasonProgress> {
        self.seasons.iter().find(|season| season.number == number)
    }
}

/// What a library row shows about a title's personal state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Movie(TitleState),
    Series(SeriesProgress),
}

/// Title and season rows with their episode counts, aggregated in SQLite:
/// one statement whatever the number of titles, seasons or episodes.
const STATUS_ROWS: &str = "
    SELECT m.local_media_id, m.media_type, m.details_fetched_at, mt.watched_at, mt.rating,
           s.season_number, s.episodes_fetched_at, s.episodes_known, s.episode_count,
           (SELECT count(*) FROM episodes AS e
            WHERE e.local_media_id = s.local_media_id AND e.season_number = s.season_number),
           (SELECT count(*) FROM episode_tracking AS t
            WHERE t.local_media_id = s.local_media_id AND t.season_number = s.season_number
              AND EXISTS (SELECT 1 FROM episodes AS e
                  WHERE e.local_media_id = t.local_media_id AND e.season_number = t.season_number
                    AND e.episode_number = t.episode_number))
    FROM media AS m
    LEFT JOIN media_tracking AS mt ON mt.local_media_id = m.local_media_id
    LEFT JOIN seasons AS s ON s.local_media_id = m.local_media_id";

fn statuses(
    db: &Database,
    filter: &str,
    args: impl rusqlite::Params,
) -> Result<HashMap<i64, Status>, AppError> {
    let mut stmt = db
        .conn()
        .prepare_cached(&format!("{STATUS_ROWS} WHERE {filter}"))
        .map_err(read_failed)?;
    let mut rows = stmt.query(args).map_err(read_failed)?;
    let mut statuses = HashMap::new();
    while let Some(row) = rows.next().map_err(read_failed)? {
        let get = || -> rusqlite::Result<_> {
            let season: Option<i64> = row.get(5)?;
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<i64>>(2)?.is_some(),
                TitleState {
                    watched_at: row.get(3)?,
                    rating: row.get(4)?,
                },
                season.map(|number| -> rusqlite::Result<_> {
                    Ok(SeasonProgress {
                        number,
                        known: row.get(9)?,
                        watched: row.get(10)?,
                        covered: matches!(
                            metadata::coverage(row.get(6)?, row.get(7)?, row.get(8)?),
                            Coverage::Complete(_)
                        ),
                    })
                }),
            ))
        };
        let (id, kind, details_fetched, state, season) = get().map_err(read_failed)?;
        let status = statuses
            .entry(id)
            .or_insert(match MediaType::from_key(&kind)? {
                MediaType::Movie => Status::Movie(state),
                MediaType::Tv => Status::Series(SeriesProgress {
                    details_fetched,
                    seasons: Vec::new(),
                }),
            });
        if let (Status::Series(progress), Some(season)) = (status, season) {
            progress.seasons.push(season.map_err(read_failed)?);
        }
    }
    for status in statuses.values_mut() {
        if let Status::Series(progress) = status {
            progress.seasons.sort_by_key(|season| season.number);
        }
    }
    Ok(statuses)
}

/// The personal status of every title in the library, in one query.
pub fn library_status(db: &Database) -> Result<HashMap<i64, Status>, AppError> {
    statuses(
        db,
        "m.local_media_id IN (SELECT local_media_id FROM library_entries)",
        [],
    )
}

/// One series' progress. Empty for a movie or an unknown title.
pub fn series_progress(db: &Database, id: i64) -> Result<SeriesProgress, AppError> {
    Ok(
        match statuses(db, "m.local_media_id = ?1", [id])?.remove(&id) {
            Some(Status::Series(progress)) => progress,
            _ => SeriesProgress::default(),
        },
    )
}

/// Where a series continues.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NextEpisode {
    /// The first unwatched stored regular episode, by season then episode.
    Episode {
        season: i64,
        number: i64,
        name: Option<String>,
    },
    /// Every regular episode watched, with complete coverage.
    Complete,
    /// Every stored regular episode watched, but coverage is incomplete: more
    /// episodes may exist that are not downloaded.
    CaughtUp,
    /// No regular episode is stored yet.
    NothingKnown,
}

/// The next episode of a series, given its `progress`. Never marks anything.
pub fn next_episode(
    db: &Database,
    id: i64,
    progress: &SeriesProgress,
) -> Result<NextEpisode, AppError> {
    // Only the first regular season with an unwatched stored episode is
    // searched; fully watched seasons are skipped without reading them.
    let unfinished = progress
        .regular()
        .find(|season| season.watched < season.known);
    let Some(season) = unfinished else {
        return Ok(match progress.known() {
            0 => NextEpisode::NothingKnown,
            _ if progress.is_complete() => NextEpisode::Complete,
            _ => NextEpisode::CaughtUp,
        });
    };
    let mut stmt = db
        .conn()
        .prepare_cached(
            "SELECT e.season_number, e.episode_number, e.name FROM episodes AS e
             WHERE e.local_media_id = ?1 AND e.season_number = ?2
               AND NOT EXISTS (SELECT 1 FROM episode_tracking AS t
                   WHERE t.local_media_id = e.local_media_id
                     AND t.season_number = e.season_number
                     AND t.episode_number = e.episode_number)
             ORDER BY e.episode_number LIMIT 1",
        )
        .map_err(read_failed)?;
    let next = stmt
        .query_row(params![id, season.number], |row| {
            Ok(NextEpisode::Episode {
                season: row.get(0)?,
                number: row.get(1)?,
                name: row.get(2)?,
            })
        })
        .optional()
        .map_err(read_failed)?;
    // `progress` was read just before; a missing row means it went stale.
    Ok(next.unwrap_or(NextEpisode::CaughtUp))
}

/// Library series in progress: at least one watched and one stored unwatched
/// regular episode, most recently watched first. For the future Home page.
#[cfg_attr(not(test), allow(dead_code))] // ponytail: no Home page yet (ADR-0018)
pub fn continue_watching(db: &Database) -> Result<Vec<i64>, AppError> {
    let mut stmt = db
        .conn()
        .prepare_cached(
            "SELECT t.local_media_id FROM episode_tracking AS t
             JOIN episodes AS e USING (local_media_id, season_number, episode_number)
             JOIN library_entries AS l ON l.local_media_id = t.local_media_id
             WHERE t.season_number >= 1
             GROUP BY t.local_media_id
             HAVING EXISTS (SELECT 1 FROM episodes AS u
                 WHERE u.local_media_id = t.local_media_id AND u.season_number >= 1
                   AND NOT EXISTS (SELECT 1 FROM episode_tracking AS v
                       WHERE v.local_media_id = u.local_media_id
                         AND v.season_number = u.season_number
                         AND v.episode_number = u.episode_number))
             ORDER BY max(t.watched_at) DESC, t.local_media_id DESC",
        )
        .map_err(read_failed)?;
    let rows = stmt.query_map([], |row| row.get(0)).map_err(read_failed)?;
    rows.collect::<rusqlite::Result<_>>().map_err(read_failed)
}

// History

/// One watch, with the names SQLite has now (never copied into the event).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryEntry {
    pub event_id: i64,
    pub local_media_id: i64,
    pub title: String,
    /// Season and episode numbers for an episode watch.
    pub episode: Option<(i64, i64)>,
    /// The episode's name, if the episode is still stored.
    pub episode_name: Option<String>,
    /// Unix seconds, UTC.
    pub watched_at: i64,
    /// `watched_at` in the computer's time zone, "YYYY-MM-DD HH:MM".
    pub local_time: String,
}

/// The `limit` most recent watches, newest first; the event id breaks ties.
pub fn recent_history(db: &Database, limit: usize) -> Result<Vec<HistoryEntry>, AppError> {
    let mut stmt = db
        .conn()
        .prepare_cached(
            "SELECT w.event_id, w.local_media_id, m.title, w.season_number, w.episode_number,
                    e.name, w.watched_at,
                    strftime('%Y-%m-%d %H:%M', w.watched_at, 'unixepoch', 'localtime')
             FROM watch_events AS w
             JOIN media AS m ON m.local_media_id = w.local_media_id
             LEFT JOIN episodes AS e ON e.local_media_id = w.local_media_id
                 AND e.season_number = w.season_number AND e.episode_number = w.episode_number
             ORDER BY w.watched_at DESC, w.event_id DESC
             LIMIT ?1",
        )
        .map_err(read_failed)?;
    let rows = stmt
        .query_map([limit as i64], |row| {
            let season: Option<i64> = row.get(3)?;
            let episode: Option<i64> = row.get(4)?;
            Ok(HistoryEntry {
                event_id: row.get(0)?,
                local_media_id: row.get(1)?,
                title: row.get(2)?,
                episode: season.zip(episode),
                episode_name: row.get(5)?,
                watched_at: row.get(6)?,
                local_time: row.get(7)?,
            })
        })
        .map_err(read_failed)?;
    rows.collect::<rusqlite::Result<_>>().map_err(read_failed)
}

/// Removes one history entry. Watched state is not changed. Returns whether
/// the entry existed.
pub fn delete_event(db: &Database, event_id: i64) -> Result<bool, AppError> {
    db.conn()
        .execute("DELETE FROM watch_events WHERE event_id = ?1", [event_id])
        .map(|deleted| deleted > 0)
        .map_err(write_failed)
}

/// A UTC timestamp in the computer's time zone, "YYYY-MM-DD HH:MM". SQLite
/// applies the operating system's time zone rules.
pub fn local_time(db: &Database, at: i64) -> Result<String, AppError> {
    db.conn()
        .query_row(
            "SELECT strftime('%Y-%m-%d %H:%M', ?1, 'unixepoch', 'localtime')",
            [at],
            |row| row.get(0),
        )
        .map_err(read_failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::Log;
    use crate::library;
    use crate::metadata::tests::{episode, season, series, stored};
    use crate::metadata::{Episode, save, save_episodes};
    use crate::paths::TestDir;

    const NOW: i64 = 1_800_000_000;

    fn rows(db: &Database, table: &str) -> i64 {
        db.conn()
            .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
            .unwrap()
    }

    fn episodes(season: i64, count: i64) -> Vec<Episode> {
        (1..=count).map(|n| episode(season, n)).collect()
    }

    /// A series in the library with `seasons` as (number, provider count,
    /// stored episodes), each stored list downloaded.
    fn show(db: &Database, tmdb: u32, seasons: &[(i64, u32, i64)]) -> i64 {
        let id = stored(db, MediaType::Tv, tmdb, "Show");
        let summaries = seasons
            .iter()
            .map(|(n, count, _)| season(*n, Some(*count)))
            .collect();
        save(db, id, &series("Show", summaries), NOW).unwrap();
        for (n, _, stored_episodes) in seasons {
            save_episodes(db, id, *n, &episodes(*n, *stored_episodes), NOW).unwrap();
        }
        id
    }

    /// (title id, episode, watched at), newest first.
    type Watch = (i64, Option<(i64, i64)>, i64);

    fn history(db: &Database) -> Vec<Watch> {
        recent_history(db, 100)
            .unwrap()
            .into_iter()
            .map(|entry| (entry.local_media_id, entry.episode, entry.watched_at))
            .collect()
    }

    #[test]
    fn a_movie_is_unwatched_until_marked_and_marking_is_idempotent() {
        let dir = TestDir::new("tracking-movie");
        let path = dir.0.join("bingee.db");
        let log = Log::stderr_only();
        let id = {
            let db = Database::open(&path, &log).unwrap();
            let id = stored(&db, MediaType::Movie, 603, "The Matrix");
            assert_eq!(title_state(&db, id).unwrap(), TitleState::default());

            assert!(watch_movie(&db, id, NOW, false).unwrap());
            // Mark watched, mark watched, mark watched: one event.
            assert!(!watch_movie(&db, id, NOW + 60, false).unwrap());
            assert!(!watch_movie(&db, id, NOW + 120, false).unwrap());
            assert_eq!(title_state(&db, id).unwrap().watched_at, Some(NOW));
            assert_eq!(history(&db), [(id, None, NOW)]);
            id
        };
        // Restart.
        let db = Database::open(&path, &log).unwrap();
        assert_eq!(title_state(&db, id).unwrap().watched_at, Some(NOW));

        // Unwatched: state goes, the historical fact stays.
        assert!(unwatch_movie(&db, id).unwrap());
        assert!(!unwatch_movie(&db, id).unwrap());
        assert_eq!(title_state(&db, id).unwrap().watched_at, None);
        assert_eq!(history(&db), [(id, None, NOW)]);

        // Watching again later is a second real event.
        assert!(watch_movie(&db, id, NOW + 500, false).unwrap());
        assert_eq!(history(&db), [(id, None, NOW + 500), (id, None, NOW)]);
    }

    #[test]
    fn watch_again_records_a_rewatch_and_moves_the_watched_date() {
        let db = Database::open_in_memory();
        let id = stored(&db, MediaType::Movie, 603, "The Matrix");
        watch_movie(&db, id, NOW, false).unwrap();
        assert!(watch_movie(&db, id, NOW + 86_400, true).unwrap());
        assert_eq!(title_state(&db, id).unwrap().watched_at, Some(NOW + 86_400));
        assert_eq!(rows(&db, "watch_events"), 2);

        let show = show(&db, 1, &[(1, 2, 2)]);
        watch_episode(&db, show, 1, 1, NOW, false).unwrap();
        assert!(!watch_episode(&db, show, 1, 1, NOW + 1, false).unwrap());
        assert!(watch_episode(&db, show, 1, 1, NOW + 2, true).unwrap());
        let episode_events = history(&db)
            .into_iter()
            .filter(|(title, ..)| *title == show)
            .count();
        assert_eq!(episode_events, 2, "the model supports episode rewatches");
    }

    #[test]
    fn ratings_are_one_to_ten_nullable_and_independent_of_watched_state() {
        let db = Database::open_in_memory();
        let movie = stored(&db, MediaType::Movie, 603, "The Matrix");
        set_rating(&db, movie, Some(9)).unwrap();
        assert_eq!(
            title_state(&db, movie).unwrap(),
            TitleState {
                watched_at: None,
                rating: Some(9)
            },
            "rating does not imply watched"
        );
        assert_eq!(rows(&db, "watch_events"), 0);
        set_rating(&db, movie, Some(7)).unwrap();
        watch_movie(&db, movie, NOW, false).unwrap();
        unwatch_movie(&db, movie).unwrap();
        assert_eq!(title_state(&db, movie).unwrap().rating, Some(7));
        set_rating(&db, movie, None).unwrap();
        assert_eq!(title_state(&db, movie).unwrap(), TitleState::default());

        for bad in [0, 11] {
            let error = set_rating(&db, movie, Some(bad)).unwrap_err();
            assert_eq!(error.kind, ErrorKind::InvalidData);
        }
        // A series can be rated too; an unknown title cannot.
        let show = show(&db, 1, &[]);
        set_rating(&db, show, Some(10)).unwrap();
        assert_eq!(title_state(&db, show).unwrap().rating, Some(10));
        assert!(set_rating(&db, 4242, Some(5)).is_err());
    }

    #[test]
    fn actions_refuse_the_wrong_kind_of_title_or_an_unknown_episode() {
        let db = Database::open_in_memory();
        let movie = stored(&db, MediaType::Movie, 603, "The Matrix");
        let show = show(&db, 1, &[(1, 3, 2)]);
        assert_eq!(
            watch_movie(&db, show, NOW, false).unwrap_err().kind,
            ErrorKind::InvalidData
        );
        assert!(watch_episode(&db, movie, 1, 1, NOW, false).is_err());
        assert!(watch_season(&db, movie, 1, NOW).is_err());
        // Episode 3 is counted by TMDB but not stored: it cannot be marked.
        assert!(watch_episode(&db, show, 1, 3, NOW, false).is_err());
        assert!(watch_movie(&db, 4242, NOW, false).is_err());
        for table in ["media_tracking", "episode_tracking", "watch_events"] {
            assert_eq!(rows(&db, table), 0, "{table}");
        }
    }

    #[test]
    fn episodes_are_marked_one_by_one_and_survive_a_restart() {
        let dir = TestDir::new("tracking-episodes");
        let path = dir.0.join("bingee.db");
        let log = Log::stderr_only();
        let id = {
            let db = Database::open(&path, &log).unwrap();
            let id = show(&db, 1, &[(1, 3, 3)]);
            assert!(watched_episodes(&db, id, 1).unwrap().is_empty());
            assert!(watch_episode(&db, id, 1, 2, NOW, false).unwrap());
            assert!(!watch_episode(&db, id, 1, 2, NOW + 5, false).unwrap());
            assert!(watch_episode(&db, id, 1, 3, NOW + 10, false).unwrap());
            id
        };
        let db = Database::open(&path, &log).unwrap();
        assert_eq!(watched_episodes(&db, id, 1).unwrap(), HashSet::from([2, 3]));
        assert!(unwatch_episode(&db, id, 1, 3).unwrap());
        assert!(!unwatch_episode(&db, id, 1, 3).unwrap());
        assert_eq!(watched_episodes(&db, id, 1).unwrap(), HashSet::from([2]));
        assert_eq!(
            history(&db),
            [(id, Some((1, 3)), NOW + 10), (id, Some((1, 2)), NOW)],
            "unmarking keeps history"
        );
    }

    #[test]
    fn a_season_is_marked_in_one_transaction() {
        let db = Database::open_in_memory();
        let id = show(&db, 1, &[(1, 10, 10), (2, 5, 5)]);
        watch_episode(&db, id, 1, 4, NOW - 100, false).unwrap();

        assert_eq!(watch_season(&db, id, 1, NOW).unwrap(), 9);
        assert_eq!(watched_episodes(&db, id, 1).unwrap().len(), 10);
        assert!(watched_episodes(&db, id, 2).unwrap().is_empty());
        // The episode watched before keeps its date and gets no second event.
        let dates: Vec<i64> = db
            .conn()
            .prepare("SELECT watched_at FROM episode_tracking WHERE episode_number = 4")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(dates, [NOW - 100]);
        assert_eq!(rows(&db, "watch_events"), 10);
        // Same timestamp: the later episode is the newer event.
        let newest = &recent_history(&db, 2).unwrap();
        assert_eq!(
            newest.iter().map(|e| e.episode).collect::<Vec<_>>(),
            [Some((1, 10)), Some((1, 9))]
        );
        // Idempotent.
        assert_eq!(watch_season(&db, id, 1, NOW + 1).unwrap(), 0);
        assert_eq!(rows(&db, "watch_events"), 10);

        assert_eq!(unwatch_season(&db, id, 1).unwrap(), 10);
        assert!(watched_episodes(&db, id, 1).unwrap().is_empty());
        assert_eq!(rows(&db, "watch_events"), 10, "history stays");

        // A failure part-way leaves nothing: no events, no state.
        db.conn()
            .execute_batch(
                "CREATE TEMP TRIGGER fail BEFORE INSERT ON episode_tracking
                 WHEN NEW.episode_number = 3 BEGIN SELECT RAISE(ABORT, 'simulated'); END;",
            )
            .unwrap();
        let error = watch_season(&db, id, 2, NOW).unwrap_err();
        assert_eq!(error.kind, ErrorKind::Database);
        assert!(watched_episodes(&db, id, 2).unwrap().is_empty());
        assert_eq!(rows(&db, "watch_events"), 10);
    }

    #[test]
    fn metadata_refresh_never_touches_personal_state() {
        let db = Database::open_in_memory();
        let movie = stored(&db, MediaType::Movie, 603, "The Matrix");
        watch_movie(&db, movie, NOW, false).unwrap();
        set_rating(&db, movie, Some(9)).unwrap();
        let id = show(&db, 1, &[(0, 2, 2), (1, 3, 3)]);
        watch_episode(&db, id, 1, 1, NOW, false).unwrap();
        watch_episode(&db, id, 0, 2, NOW, false).unwrap();
        let before = (history(&db), title_state(&db, movie).unwrap());

        // Refresh the movie, the series and both seasons, repeatedly.
        for at in [NOW + 10, NOW + 20] {
            save(
                &db,
                movie,
                &crate::metadata::tests::movie("The Matrix", Vec::new()),
                at,
            )
            .unwrap();
            save(
                &db,
                id,
                &series("Show", vec![season(0, Some(2)), season(1, Some(3))]),
                at,
            )
            .unwrap();
            save_episodes(&db, id, 0, &episodes(0, 2), at).unwrap();
            save_episodes(&db, id, 1, &episodes(1, 3), at).unwrap();
        }
        assert_eq!((history(&db), title_state(&db, movie).unwrap()), before);
        assert_eq!(watched_episodes(&db, id, 1).unwrap(), HashSet::from([1]));
        assert_eq!(watched_episodes(&db, id, 0).unwrap(), HashSet::from([2]));
    }

    #[test]
    fn an_episode_the_provider_removes_keeps_its_personal_history() {
        let db = Database::open_in_memory();
        let id = show(&db, 1, &[(1, 3, 3)]);
        watch_episode(&db, id, 1, 3, NOW, false).unwrap();
        watch_episode(&db, id, 1, 1, NOW + 1, false).unwrap();

        // TMDB now lists two episodes; episode 3's metadata row goes.
        save(&db, id, &series("Show", vec![season(1, Some(2))]), NOW + 50).unwrap();
        save_episodes(&db, id, 1, &episodes(1, 2), NOW + 50).unwrap();
        assert_eq!(rows(&db, "episodes"), 2);
        assert_eq!(rows(&db, "episode_tracking"), 2, "tracking kept");
        let entries = recent_history(&db, 10).unwrap();
        assert_eq!(entries.len(), 2, "history kept");
        let orphan = entries.iter().find(|e| e.episode == Some((1, 3))).unwrap();
        assert_eq!(orphan.episode_name, None, "shown by number only");
        assert_eq!(orphan.title, "Show");
        // The orphan counts nowhere.
        let progress = series_progress(&db, id).unwrap();
        assert_eq!((progress.watched(), progress.known()), (1, 2));

        // A whole season dropped by the provider: the same.
        save(&db, id, &series("Show", vec![season(2, Some(1))]), NOW + 60).unwrap();
        assert_eq!(rows(&db, "episode_tracking"), 2);
        assert_eq!(rows(&db, "watch_events"), 2);

        // The episode comes back: it is watched again at once.
        save(&db, id, &series("Show", vec![season(1, Some(3))]), NOW + 70).unwrap();
        save_episodes(&db, id, 1, &episodes(1, 3), NOW + 70).unwrap();
        assert_eq!(watched_episodes(&db, id, 1).unwrap(), HashSet::from([1, 3]));
    }

    #[test]
    fn library_removal_and_re_add_keep_personal_state() {
        let db = Database::open_in_memory();
        let movie = stored(&db, MediaType::Movie, 603, "The Matrix");
        watch_movie(&db, movie, NOW, false).unwrap();
        set_rating(&db, movie, Some(9)).unwrap();
        let id = show(&db, 1, &[(1, 2, 2)]);
        watch_season(&db, id, 1, NOW).unwrap();

        assert!(library::remove(&db, movie).unwrap());
        assert!(library::remove(&db, id).unwrap());
        assert!(!library_status(&db).unwrap().contains_key(&movie));
        assert_eq!(stored(&db, MediaType::Movie, 603, "The Matrix"), movie);
        assert_eq!(stored(&db, MediaType::Tv, 1, "Show"), id);

        assert_eq!(
            title_state(&db, movie).unwrap(),
            TitleState {
                watched_at: Some(NOW),
                rating: Some(9)
            }
        );
        assert_eq!(watched_episodes(&db, id, 1).unwrap().len(), 2);
        assert_eq!(rows(&db, "watch_events"), 3);
        let statuses = library_status(&db).unwrap();
        assert!(matches!(statuses[&movie], Status::Movie(state) if state.rating == Some(9)));
        assert!(matches!(&statuses[&id], Status::Series(p) if p.watched() == 2));
    }

    /// (watched, known, coverage complete, complete, next)
    fn summary(db: &Database, id: i64) -> (u32, u32, bool, bool, NextEpisode) {
        let progress = series_progress(db, id).unwrap();
        let next = next_episode(db, id, &progress).unwrap();
        (
            progress.watched(),
            progress.known(),
            progress.coverage_complete(),
            progress.is_complete(),
            next,
        )
    }

    fn next(season: i64, number: i64) -> NextEpisode {
        NextEpisode::Episode {
            season,
            number,
            name: Some(format!("Episode {number}")),
        }
    }

    #[test]
    fn completion_needs_complete_coverage_not_just_every_known_episode() {
        use NextEpisode::{CaughtUp, Complete, NothingKnown};
        let db = Database::open_in_memory();

        // Details never fetched: nothing known, nothing complete.
        let bare = stored(&db, MediaType::Tv, 50, "Bare");
        assert_eq!(summary(&db, bare), (0, 0, false, false, NothingKnown));

        // Seasons known, no episodes downloaded.
        let id = stored(&db, MediaType::Tv, 51, "Undownloaded");
        save(
            &db,
            id,
            &series("Undownloaded", vec![season(1, Some(8))]),
            NOW,
        )
        .unwrap();
        assert_eq!(summary(&db, id), (0, 0, false, false, NothingKnown));

        // Partial coverage: 6 of 8 stored, all 6 watched. Not complete.
        let partial = show(&db, 52, &[(1, 8, 6)]);
        watch_season(&db, partial, 1, NOW).unwrap();
        assert_eq!(summary(&db, partial), (6, 6, false, false, CaughtUp));

        // Complete coverage, part watched.
        let full = show(&db, 53, &[(1, 3, 3), (2, 2, 2)]);
        watch_episode(&db, full, 1, 1, NOW, false).unwrap();
        assert_eq!(summary(&db, full), (1, 5, true, false, next(1, 2)));
        // Every season watched: complete.
        watch_season(&db, full, 1, NOW).unwrap();
        watch_season(&db, full, 2, NOW).unwrap();
        assert_eq!(summary(&db, full), (5, 5, true, true, Complete));

        // A regular season never downloaded keeps a watched season 1 from
        // counting as complete.
        let unopened = stored(&db, MediaType::Tv, 54, "Unopened");
        save(
            &db,
            unopened,
            &series("Unopened", vec![season(1, Some(2)), season(2, Some(2))]),
            NOW,
        )
        .unwrap();
        save_episodes(&db, unopened, 1, &episodes(1, 2), NOW).unwrap();
        watch_season(&db, unopened, 1, NOW).unwrap();
        assert_eq!(summary(&db, unopened), (2, 2, false, false, CaughtUp));
    }

    #[test]
    fn specials_are_tracked_but_never_count_toward_completion() {
        use NextEpisode::{Complete, NothingKnown};
        let db = Database::open_in_memory();

        // Season 0 only.
        let only = show(&db, 60, &[(0, 3, 3)]);
        watch_episode(&db, only, 0, 1, NOW, false).unwrap();
        let progress = series_progress(&db, only).unwrap();
        assert_eq!(progress.specials(), Some((1, 3)));
        assert_eq!(summary(&db, only), (0, 0, false, false, NothingKnown));

        // Specials and regular seasons: unwatched specials do not block
        // completion, and never become the next episode.
        let id = show(&db, 61, &[(0, 8, 8), (1, 2, 2)]);
        watch_episode(&db, id, 0, 5, NOW, false).unwrap();
        assert_eq!(summary(&db, id), (0, 2, true, false, next(1, 1)));
        watch_season(&db, id, 1, NOW).unwrap();
        assert_eq!(summary(&db, id), (2, 2, true, true, Complete));
        assert_eq!(
            series_progress(&db, id).unwrap().specials(),
            Some((1, 8)),
            "3 / 8 specials watched is reported apart"
        );
        // Undownloaded specials do not make coverage incomplete either.
        let unfetched = stored(&db, MediaType::Tv, 62, "Unfetched specials");
        save(
            &db,
            unfetched,
            &series("U", vec![season(0, Some(4)), season(1, Some(1))]),
            NOW,
        )
        .unwrap();
        save_episodes(&db, unfetched, 1, &episodes(1, 1), NOW).unwrap();
        watch_season(&db, unfetched, 1, NOW).unwrap();
        assert!(series_progress(&db, unfetched).unwrap().is_complete());
    }

    #[test]
    fn a_new_episode_makes_a_complete_series_incomplete_again() {
        let db = Database::open_in_memory();
        let id = show(&db, 70, &[(1, 10, 10)]);
        watch_season(&db, id, 1, NOW).unwrap();
        assert_eq!(
            summary(&db, id),
            (10, 10, true, true, NextEpisode::Complete)
        );

        // A series refresh reports episode 11: coverage is partial at once.
        save(
            &db,
            id,
            &series("Show", vec![season(1, Some(11))]),
            NOW + 100,
        )
        .unwrap();
        assert_eq!(
            summary(&db, id),
            (10, 10, false, false, NextEpisode::CaughtUp)
        );

        // The season refresh brings it: unwatched, the others still watched.
        save_episodes(&db, id, 1, &episodes(1, 11), NOW + 200).unwrap();
        assert_eq!(summary(&db, id), (10, 11, true, false, next(1, 11)));
        assert_eq!(watched_episodes(&db, id, 1).unwrap().len(), 10);
        assert!(!watched_episodes(&db, id, 1).unwrap().contains(&11));
    }

    #[test]
    fn next_episode_follows_season_and_episode_order_with_gaps() {
        let db = Database::open_in_memory();
        let id = show(&db, 80, &[(0, 1, 1), (1, 2, 2), (2, 1, 1)]);
        assert_eq!(summary(&db, id).4, next(1, 1));
        watch_episode(&db, id, 1, 1, NOW, false).unwrap();
        assert_eq!(summary(&db, id).4, next(1, 2));
        // S01E02 skipped: S02E01 watched, the gap is still next.
        watch_episode(&db, id, 2, 1, NOW, false).unwrap();
        assert_eq!(summary(&db, id).4, next(1, 2));
        watch_episode(&db, id, 1, 2, NOW, false).unwrap();
        assert_eq!(
            summary(&db, id).4,
            NextEpisode::Complete,
            "specials ignored"
        );
        // Unmark S01E01: next is back there.
        unwatch_episode(&db, id, 1, 1).unwrap();
        assert_eq!(summary(&db, id).4, next(1, 1));

        // Ordered numerically, not as text: E2 before E10.
        let long = show(&db, 81, &[(1, 12, 12)]);
        watch_episode(&db, long, 1, 1, NOW, false).unwrap();
        assert_eq!(summary(&db, long).4, next(1, 2));
    }

    #[test]
    fn continue_watching_lists_started_unfinished_library_series() {
        let db = Database::open_in_memory();
        let started = show(&db, 90, &[(1, 3, 3)]);
        let finished = show(&db, 91, &[(1, 1, 1)]);
        let untouched = show(&db, 92, &[(1, 2, 2)]);
        let specials_only = show(&db, 93, &[(0, 2, 2), (1, 2, 2)]);
        let later = show(&db, 94, &[(1, 2, 2)]);
        watch_episode(&db, started, 1, 1, NOW, false).unwrap();
        watch_season(&db, finished, 1, NOW).unwrap();
        watch_episode(&db, specials_only, 0, 1, NOW, false).unwrap();
        watch_episode(&db, later, 1, 1, NOW + 10, false).unwrap();
        assert_eq!(continue_watching(&db).unwrap(), [later, started]);
        let _ = untouched;
        library::remove(&db, later).unwrap();
        assert_eq!(continue_watching(&db).unwrap(), [started]);
    }

    #[test]
    fn history_is_newest_first_deterministic_and_deletable() {
        let dir = TestDir::new("tracking-history");
        let path = dir.0.join("bingee.db");
        let log = Log::stderr_only();
        let (movie, id) = {
            let db = Database::open(&path, &log).unwrap();
            let movie = stored(&db, MediaType::Movie, 603, "The Matrix");
            let id = show(&db, 1, &[(1, 2, 2)]);
            watch_episode(&db, id, 1, 1, NOW + 10, false).unwrap();
            watch_movie(&db, movie, NOW, false).unwrap();
            // Same second as the movie, recorded later.
            watch_episode(&db, id, 1, 2, NOW, false).unwrap();
            (movie, id)
        };
        let db = Database::open(&path, &log).unwrap();
        let entries = recent_history(&db, 10).unwrap();
        assert_eq!(
            history(&db),
            [
                (id, Some((1, 1)), NOW + 10),
                (id, Some((1, 2)), NOW),
                (movie, None, NOW)
            ]
        );
        assert_eq!(entries[0].episode_name.as_deref(), Some("Episode 1"));
        assert_eq!(entries[2].title, "The Matrix");
        let shape = entries[0].local_time.as_bytes();
        assert_eq!(
            (shape.len(), shape[4], shape[10], shape[13]),
            (16, b'-', b' ', b':')
        );
        assert_eq!(local_time(&db, NOW + 10).unwrap(), entries[0].local_time);
        assert_eq!(recent_history(&db, 1).unwrap().len(), 1, "limited");

        // Deleting an event removes that event only; state stays.
        assert!(delete_event(&db, entries[2].event_id).unwrap());
        assert!(!delete_event(&db, entries[2].event_id).unwrap());
        assert_eq!(title_state(&db, movie).unwrap().watched_at, Some(NOW));
        assert!(delete_event(&db, entries[0].event_id).unwrap());
        assert!(watched_episodes(&db, id, 1).unwrap().contains(&1));
        assert_eq!(history(&db), [(id, Some((1, 2)), NOW)]);
    }

    #[test]
    fn library_status_covers_every_title_in_one_query() {
        let db = Database::open_in_memory();
        let movie = stored(&db, MediaType::Movie, 603, "The Matrix");
        let unwatched = stored(&db, MediaType::Movie, 604, "Reloaded");
        let id = show(&db, 1, &[(0, 1, 1), (1, 4, 3), (2, 2, 2)]);
        watch_movie(&db, movie, NOW, false).unwrap();
        watch_episode(&db, id, 1, 2, NOW, false).unwrap();
        let cached = stored(&db, MediaType::Movie, 605, "Not in library");
        library::remove(&db, cached).unwrap();

        let statuses = library_status(&db).unwrap();
        assert_eq!(statuses.len(), 3);
        assert!(matches!(statuses[&movie], Status::Movie(s) if s.watched_at == Some(NOW)));
        assert!(matches!(statuses[&unwatched], Status::Movie(s) if s.watched_at.is_none()));
        let Status::Series(progress) = &statuses[&id] else {
            panic!("{statuses:?}")
        };
        assert_eq!(progress, &series_progress(&db, id).unwrap());
        assert_eq!(
            progress.seasons,
            [
                SeasonProgress {
                    number: 0,
                    known: 1,
                    watched: 0,
                    covered: true
                },
                SeasonProgress {
                    number: 1,
                    known: 3,
                    watched: 1,
                    covered: false
                },
                SeasonProgress {
                    number: 2,
                    known: 2,
                    watched: 0,
                    covered: true
                },
            ]
        );
        assert_eq!(
            series_progress(&db, movie).unwrap(),
            SeriesProgress::default()
        );
    }

    /// Informal R10 observation, not a `BENCHMARK_SPEC.md` run: personal
    /// writes and progress reads in a file-backed database (the real commit
    /// cost), with a 250-episode season, a 2,000-episode season and a
    /// 1,000-title library. Run with
    /// `cargo test --release informal_tracking_timings -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn informal_tracking_timings() {
        use std::time::{Duration, Instant};

        let dir = TestDir::new("tracking-timings");
        let db = Database::open(&dir.0.join("bingee.db"), &Log::stderr_only()).unwrap();
        let movie = stored(&db, MediaType::Movie, 603, "The Matrix");
        // 60 seasons of 250 episodes, plus one 2,000-episode season.
        let long = stored(&db, MediaType::Tv, 1, "Long Series");
        let mut summaries: Vec<_> = (1..=60).map(|n| season(n, Some(250))).collect();
        summaries.push(season(61, Some(2_000)));
        save(&db, long, &series("Long Series", summaries), NOW).unwrap();
        for n in 1..=60 {
            save_episodes(&db, long, n, &episodes(n, 250), NOW).unwrap();
        }
        save_episodes(&db, long, 61, &episodes(61, 2_000), NOW).unwrap();
        for n in 1..=30 {
            watch_season(&db, long, n, NOW).unwrap();
        }
        for n in 2..1_000 {
            let id = stored(&db, MediaType::Movie, n, &format!("Movie {n}"));
            if n % 2 == 0 {
                watch_movie(&db, id, NOW, false).unwrap();
            }
        }

        let median = |mut samples: Vec<Duration>| {
            samples.sort();
            samples[samples.len() / 2].as_secs_f64() * 1e3
        };
        let time = |label: &str, runs: usize, f: &mut dyn FnMut(usize)| {
            let samples: Vec<Duration> = (0..runs)
                .map(|run| {
                    let start = Instant::now();
                    f(run);
                    start.elapsed()
                })
                .collect();
            println!("{label:<48} median {:.3} ms ({runs} runs)", median(samples));
        };
        time("toggle one episode watched (commit)", 50, &mut |run| {
            watch_episode(&db, long, 40, run as i64 % 250 + 1, NOW, false).unwrap();
        });
        time("toggle one episode unwatched (commit)", 50, &mut |run| {
            unwatch_episode(&db, long, 40, run as i64 % 250 + 1).unwrap();
        });
        time(
            "toggle movie watched + unwatched (2 commits)",
            50,
            &mut |_| {
                watch_movie(&db, movie, NOW, false).unwrap();
                unwatch_movie(&db, movie).unwrap();
            },
        );
        time("set rating (commit)", 50, &mut |run| {
            set_rating(&db, movie, Some(run as u8 % 10 + 1)).unwrap();
        });
        for (label, number, runs) in [("250", 45, 20), ("2,000", 61, 10)] {
            let (mut marks, mut clears) = (Vec::new(), Vec::new());
            for _ in 0..runs {
                let start = Instant::now();
                assert!(watch_season(&db, long, number, NOW).unwrap() > 0);
                marks.push(start.elapsed());
                let start = Instant::now();
                unwatch_season(&db, long, number).unwrap();
                clears.push(start.elapsed());
            }
            println!(
                "mark {label}-episode season watched / unwatched (commits) median {:.3} / {:.3} ms ({runs} runs)",
                median(marks),
                median(clears)
            );
        }
        time(
            "series progress, 61 seasons / 17,000 episodes",
            100,
            &mut |_| {
                std::hint::black_box(series_progress(&db, long).unwrap());
            },
        );
        let progress = series_progress(&db, long).unwrap();
        time("next episode", 100, &mut |_| {
            std::hint::black_box(next_episode(&db, long, &progress).unwrap());
        });
        time("watched set of a 250-episode season", 100, &mut |_| {
            std::hint::black_box(watched_episodes(&db, long, 10).unwrap());
        });
        time("library status, 1,000 titles", 50, &mut |_| {
            std::hint::black_box(library_status(&db).unwrap());
        });
        println!("watch events: {}", rows(&db, "watch_events"));
        time("recent history, 200 of those", 100, &mut |_| {
            std::hint::black_box(recent_history(&db, 200).unwrap());
        });
    }
}
