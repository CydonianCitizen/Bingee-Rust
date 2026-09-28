//! Statistics: aggregates computed from the local database each time they are
//! shown (ADR-0020, ADR-0021). Nothing here is stored, sent or logged.
//!
//! Two kinds of numbers, never mixed:
//!
//! * **Viewing history** comes from `watch_events` only: watches, watch time,
//!   rewatches, activity and genres. It covers every event whatever the
//!   title's Library membership or current watched state, and the time range
//!   applies to it.
//! * **Your Library now** comes from current state (`tracking::library_status`,
//!   the same aggregate the Library rows use): watched movies and episodes,
//!   series progress and ratings of titles currently in the Library. No time
//!   range applies.
//!
//! Plain Rust types, no Slint.

use rusqlite::{Connection, params};

use crate::database::Database;
use crate::error::AppError;
use crate::tracking::{self, RATING_MAX, Status};

fn read_failed(err: rusqlite::Error) -> AppError {
    AppError::database("Your statistics could not be read.", err)
}

/// Which watch events count. Boundaries are local calendar days and months
/// (SQLite `localtime`, as History uses), so the range and the activity
/// buckets always agree.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Range {
    /// Today and the 29 local days before it: 30 daily buckets.
    Days30,
    /// This local month and the 11 before it: 12 monthly buckets.
    #[default]
    Months12,
    /// Everything: one bucket per local year since the first watch.
    AllTime,
}

impl Range {
    pub const ALL: [Range; 3] = [Range::Days30, Range::Months12, Range::AllTime];

    /// Bucket key format, the start-of-bucket modifier and the step unit.
    fn buckets(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Range::Days30 => ("%Y-%m-%d", "start of day", " days"),
            Range::Months12 => ("%Y-%m", "start of month", " months"),
            Range::AllTime => ("%Y", "start of year", " years"),
        }
    }
}

/// Watch events of one kind (movies or episodes) within a range.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Viewing {
    /// Watch events.
    pub watches: u32,
    /// Different movies or episodes those events are of.
    pub titles: u32,
    /// Events that are not the first recorded watch of their movie or episode.
    /// The first watch may lie before the range.
    pub rewatches: u32,
    /// Sum of the runtimes that are known.
    pub known_minutes: u64,
    /// Events whose runtime is unknown; they add nothing to `known_minutes`.
    pub unknown_runtime: u32,
}

impl Viewing {
    fn plus(self, other: Viewing) -> Viewing {
        Viewing {
            watches: self.watches + other.watches,
            titles: self.titles + other.titles,
            rewatches: self.rewatches + other.rewatches,
            known_minutes: self.known_minutes + other.known_minutes,
            unknown_runtime: self.unknown_runtime + other.unknown_runtime,
        }
    }
}

/// Watches in one local day, month or year.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bucket {
    /// "YYYY-MM-DD", "YYYY-MM" or "YYYY".
    pub key: String,
    pub watches: u32,
}

/// Watches of titles with one genre. A watch counts toward every genre of
/// its title, so genres add up to more than the total.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenreCount {
    /// `None`: the title has no genre stored.
    pub name: Option<String>,
    pub watches: u32,
    pub known_minutes: u64,
}

/// Current state of the titles in the Library.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LibraryNow {
    pub movies: u32,
    pub movies_watched: u32,
    pub series: u32,
    /// `SeriesProgress::is_complete`: complete coverage, all regular watched.
    pub series_complete: u32,
    /// Some regular episode watched, not complete.
    pub series_in_progress: u32,
    /// No regular episode watched (specials do not start a series).
    pub series_not_started: u32,
    /// Series whose regular episode lists are not all downloaded.
    pub series_incomplete_coverage: u32,
    /// Watched stored episodes, specials included.
    pub episodes_watched: u32,
    /// `ratings[n - 1]`: titles rated `n`.
    pub ratings: [u32; RATING_MAX as usize],
}

impl LibraryNow {
    pub fn rated(&self) -> u32 {
        self.ratings.iter().sum()
    }

    /// Over rated titles only; `None` when nothing is rated.
    pub fn average_rating(&self) -> Option<f64> {
        let weighted: u32 = (1..).zip(self.ratings).map(|(n, count)| n * count).sum();
        (self.rated() > 0).then(|| f64::from(weighted) / f64::from(self.rated()))
    }

    pub fn is_empty(&self) -> bool {
        self.movies + self.series == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Statistics {
    pub range: Range,
    /// Any watch event at all, whatever the range.
    pub has_history: bool,
    pub movies: Viewing,
    pub episodes: Viewing,
    /// Oldest first, empty buckets included.
    pub activity: Vec<Bucket>,
    /// Most watched first; "no genre" last.
    pub genres: Vec<GenreCount>,
    pub library: LibraryNow,
}

impl Statistics {
    pub fn total(&self) -> Viewing {
        self.movies.plus(self.episodes)
    }
}

/// Every watch event with its runtime: the one recorded with the event, or,
/// only for an event recorded while none was known, the runtime stored now
/// (ADR-0021). The lookup runs inside the statement and only for those.
const EVENTS: &str = "
    events AS (
        SELECT w.event_id, w.local_media_id, w.media_type, w.season_number, w.episode_number,
               w.watched_at,
               coalesce(w.runtime_minutes, CASE w.media_type
                   WHEN 'movie' THEN (SELECT m.runtime_minutes FROM media AS m
                                      WHERE m.local_media_id = w.local_media_id)
                   ELSE (SELECT e.runtime_minutes FROM episodes AS e
                         WHERE e.local_media_id = w.local_media_id
                           AND e.season_number = w.season_number
                           AND e.episode_number = w.episode_number)
               END) AS minutes
        FROM watch_events AS w
    )";

/// Everything the Statistics page shows, as of `now` (Unix seconds, UTC), in
/// a fixed number of statements whatever the size of the history.
pub fn load(db: &Database, range: Range, now: i64) -> Result<Statistics, AppError> {
    let conn = db.conn();
    let start = range_start(conn, range, now)?;
    let (movies, episodes) = viewing(conn, start)?;
    Ok(Statistics {
        range,
        has_history: conn
            .query_row("SELECT EXISTS (SELECT 1 FROM watch_events)", [], |row| {
                row.get(0)
            })
            .map_err(read_failed)?,
        movies,
        episodes,
        activity: activity(conn, range, now)?,
        genres: genres(conn, start)?,
        library: library_now(db)?,
    })
}

/// The first second of the range: the start of the local day 29 days ago or
/// of the local month 11 months ago.
fn range_start(conn: &Connection, range: Range, now: i64) -> Result<i64, AppError> {
    let back = match range {
        Range::Days30 => "-29 days",
        Range::Months12 => "-11 months",
        Range::AllTime => return Ok(i64::MIN),
    };
    conn.query_row(
        "SELECT CAST(strftime('%s', ?1, 'unixepoch', 'localtime', ?2, ?3, 'utc') AS INTEGER)",
        params![now, range.buckets().1, back],
        |row| row.get(0),
    )
    .map_err(read_failed)
}

/// (movies, episodes) watched since `start`. Rewatches are judged against the
/// whole history, so the first watch may lie before `start`.
fn viewing(conn: &Connection, start: i64) -> Result<(Viewing, Viewing), AppError> {
    let mut stmt = conn
        .prepare_cached(&format!(
            "WITH {EVENTS},
             -- One row per movie or episode watched in the range. Its first
             -- watch ever is not a rewatch; every other watch is.
             targets AS (
                 SELECT media_type, sum(watched_at >= ?1) AS watches,
                        min(watched_at) >= ?1 AS first_in_range,
                        sum(CASE WHEN watched_at >= ?1 THEN minutes END) AS minutes,
                        sum(watched_at >= ?1 AND minutes IS NULL) AS unknown
                 FROM events
                 GROUP BY local_media_id, season_number, episode_number)
             SELECT media_type = 'movie', sum(watches), count(*), sum(watches - first_in_range),
                    ifnull(sum(minutes), 0), sum(unknown)
             FROM targets WHERE watches > 0
             GROUP BY media_type"
        ))
        .map_err(read_failed)?;
    let mut rows = stmt.query([start]).map_err(read_failed)?;
    let (mut movies, mut episodes) = (Viewing::default(), Viewing::default());
    while let Some(row) = rows.next().map_err(read_failed)? {
        let read = || -> rusqlite::Result<(bool, Viewing)> {
            Ok((
                row.get(0)?,
                Viewing {
                    watches: row.get(1)?,
                    titles: row.get(2)?,
                    rewatches: row.get(3)?,
                    known_minutes: row.get::<_, i64>(4)?.max(0) as u64,
                    unknown_runtime: row.get(5)?,
                },
            ))
        };
        match read().map_err(read_failed)? {
            (true, viewing) => movies = viewing,
            (false, viewing) => episodes = viewing,
        }
    }
    Ok((movies, episodes))
}

/// Watches per local day, month or year of the range, oldest first, empty
/// buckets included. All time starts at the year of the first watch. Each
/// bucket counts the events between its first second and the next bucket's
/// (the newest bucket has no end), using the time index: the time zone is
/// applied per bucket, not per event.
fn activity(conn: &Connection, range: Range, now: i64) -> Result<Vec<Bucket>, AppError> {
    let (format, anchor, unit) = range.buckets();
    let mut stmt = conn
        .prepare_cached(
            "WITH RECURSIVE
             size(n) AS (SELECT CASE WHEN ?5 > 0 THEN ?5 ELSE
                 ifnull(strftime('%Y', ?1, 'unixepoch', 'localtime')
                        - strftime('%Y', min(watched_at), 'unixepoch', 'localtime') + 1, 1)
                 END FROM watch_events),
             n(i) AS (SELECT 0 UNION ALL SELECT i + 1 FROM n, size WHERE i + 1 < min(size.n, 100)),
             buckets AS MATERIALIZED (
                 SELECT i, strftime(?2, ?1, 'unixepoch', 'localtime', ?3, '-' || i || ?4) AS key,
                        CAST(strftime('%s', ?1, 'unixepoch', 'localtime', ?3, '-' || i || ?4, 'utc')
                             AS INTEGER) AS first
                 FROM n)
             SELECT b.key,
                    (SELECT count(*) FROM watch_events
                     WHERE watched_at >= b.first
                       AND watched_at < ifnull(next.first, 9223372036854775807))
             FROM buckets AS b LEFT JOIN buckets AS next ON next.i = b.i - 1
             ORDER BY b.i DESC",
        )
        .map_err(read_failed)?;
    // ponytail: at most 100 yearly bars.
    let buckets = match range {
        Range::Days30 => 30,
        Range::Months12 => 12,
        Range::AllTime => 0, // one per year since the first watch
    };
    stmt.query_map(params![now, format, anchor, unit, buckets], |row| {
        Ok(Bucket {
            key: row.get(0)?,
            watches: row.get(1)?,
        })
    })
    .and_then(Iterator::collect)
    .map_err(read_failed)
}

/// Watches per genre since `start`, most watched first, "no genre" last.
fn genres(conn: &Connection, start: i64) -> Result<Vec<GenreCount>, AppError> {
    let mut stmt = conn
        .prepare_cached(&format!(
            "WITH {EVENTS},
             -- Aggregated per title first, then spread over its genres: a
             -- title's genre list is read once, not once per watch.
             titles AS (
                 SELECT local_media_id, count(*) AS watches, sum(minutes) AS minutes
                 FROM events WHERE watched_at >= ?1 GROUP BY local_media_id),
             names AS (
                 SELECT DISTINCT mg.local_media_id, g.name
                 FROM media_genres AS mg
                 JOIN genres AS g ON g.source = mg.source AND g.external_id = mg.external_id)
             SELECT n.name, sum(t.watches), ifnull(sum(t.minutes), 0)
             FROM titles AS t LEFT JOIN names AS n USING (local_media_id)
             GROUP BY n.name
             ORDER BY n.name IS NULL, sum(t.watches) DESC, sum(t.minutes) DESC, n.name"
        ))
        .map_err(read_failed)?;
    stmt.query_map([start], |row| {
        Ok(GenreCount {
            name: row.get(0)?,
            watches: row.get(1)?,
            known_minutes: row.get::<_, i64>(2)?.max(0) as u64,
        })
    })
    .and_then(Iterator::collect)
    .map_err(read_failed)
}

/// Current state of the Library, from the one-query status R10 already uses
/// for Library rows, so completion follows exactly the same rules, plus one
/// aggregate for ratings.
fn library_now(db: &Database) -> Result<LibraryNow, AppError> {
    let mut now = LibraryNow::default();
    for status in tracking::library_status(db)?.values() {
        match status {
            Status::Movie(state) => {
                now.movies += 1;
                now.movies_watched += u32::from(state.watched_at.is_some());
            }
            Status::Series(progress) => {
                now.series += 1;
                now.episodes_watched += progress.seasons.iter().map(|s| s.watched).sum::<u32>();
                if progress.is_complete() {
                    now.series_complete += 1;
                } else if progress.watched() > 0 {
                    now.series_in_progress += 1;
                } else {
                    now.series_not_started += 1;
                }
                now.series_incomplete_coverage += u32::from(!progress.coverage_complete());
            }
        }
    }
    let mut stmt = db
        .conn()
        .prepare_cached(
            "SELECT t.rating, count(*) FROM media_tracking AS t
             JOIN library_entries USING (local_media_id)
             WHERE t.rating IS NOT NULL GROUP BY t.rating",
        )
        .map_err(read_failed)?;
    let mut rows = stmt.query([]).map_err(read_failed)?;
    while let Some(row) = rows.next().map_err(read_failed)? {
        let rating: u8 = row.get(0).map_err(read_failed)?;
        if let Some(slot) = now.ratings.get_mut(usize::from(rating).wrapping_sub(1)) {
            *slot = row.get(1).map_err(read_failed)?;
        }
    }
    Ok(now)
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::library::{self, MediaType};
    use crate::metadata::tests::{episode, genre, movie, season, series, stored};
    use crate::metadata::{Episode, Genre, save, save_episodes};
    use crate::tracking::{
        delete_event, recent_history, set_rating, title_state, unwatch_episode, unwatch_movie,
        watch_episode, watch_movie, watch_season,
    };

    /// 2027-01-15 08:00 UTC: no daylight-saving change in the month before,
    /// so the day boundaries below hold in every time zone.
    pub const NOW: i64 = 1_800_000_000;
    pub const DAY: i64 = 86_400;

    fn stats(db: &Database, range: Range) -> Statistics {
        load(db, range, NOW).unwrap()
    }

    /// A movie with details: 136 minutes and `genres`.
    pub fn film(db: &Database, tmdb: u32, genres: Vec<Genre>) -> i64 {
        let id = stored(db, MediaType::Movie, tmdb, &format!("Movie {tmdb}"));
        save(db, id, &movie("Movie", genres), NOW - 400 * DAY).unwrap();
        id
    }

    /// A series with `seasons` as (number, provider count, stored episodes),
    /// every stored episode 62 minutes long.
    pub fn show(db: &Database, tmdb: u32, seasons: &[(i64, u32, i64)], genres: Vec<Genre>) -> i64 {
        let id = stored(db, MediaType::Tv, tmdb, "Show");
        let mut details = series(
            "Show",
            seasons
                .iter()
                .map(|(n, c, _)| season(*n, Some(*c)))
                .collect(),
        );
        details.genres = genres;
        save(db, id, &details, NOW - 400 * DAY).unwrap();
        for (n, _, count) in seasons {
            let list: Vec<Episode> = (1..=*count).map(|e| episode(*n, e)).collect();
            save_episodes(db, id, *n, &list, NOW - 400 * DAY).unwrap();
        }
        id
    }

    fn viewing(watches: u32, titles: u32, rewatches: u32, minutes: u64, unknown: u32) -> Viewing {
        Viewing {
            watches,
            titles,
            rewatches,
            known_minutes: minutes,
            unknown_runtime: unknown,
        }
    }

    #[test]
    fn an_empty_database_has_no_history_and_no_invented_numbers() {
        let db = Database::open_in_memory();
        for range in Range::ALL {
            let s = stats(&db, range);
            assert!(!s.has_history);
            assert_eq!(s.total(), Viewing::default());
            assert!(s.genres.is_empty());
            assert!(s.activity.iter().all(|b| b.watches == 0));
            assert!(s.library.is_empty());
            assert_eq!(s.library.average_rating(), None);
        }
        assert_eq!(stats(&db, Range::Days30).activity.len(), 30);
        assert_eq!(stats(&db, Range::Months12).activity.len(), 12);
        assert_eq!(stats(&db, Range::AllTime).activity.len(), 1);
    }

    #[test]
    fn rewatches_unmarking_and_deleting_follow_history_not_state() {
        let db = Database::open_in_memory();
        let id = film(&db, 603, vec![]);
        watch_movie(&db, id, NOW - 3 * DAY, false).unwrap();
        // Repeated "Mark watched" is not a rewatch.
        watch_movie(&db, id, NOW - 2 * DAY, false).unwrap();
        let s = stats(&db, Range::AllTime);
        assert_eq!(s.movies, viewing(1, 1, 0, 136, 0));
        assert_eq!(s.library.movies_watched, 1);

        // Watch again: one movie, two watches, one rewatch, twice the runtime.
        watch_movie(&db, id, NOW - DAY, true).unwrap();
        let s = stats(&db, Range::AllTime);
        assert_eq!(s.movies, viewing(2, 1, 1, 272, 0));
        assert_eq!(s.total(), s.movies);

        // Unmarking changes current state only.
        unwatch_movie(&db, id).unwrap();
        let s = stats(&db, Range::AllTime);
        assert_eq!(s.movies, viewing(2, 1, 1, 272, 0));
        assert_eq!((s.library.movies, s.library.movies_watched), (1, 0));

        // Deleting the first event makes the second one the first watch.
        let first = recent_history(&db, 10).unwrap().pop().unwrap();
        delete_event(&db, first.event_id).unwrap();
        assert_eq!(stats(&db, Range::AllTime).movies, viewing(1, 1, 0, 136, 0));
    }

    #[test]
    fn deleting_the_only_event_empties_history_but_keeps_the_movie_watched() {
        let db = Database::open_in_memory();
        let id = film(&db, 603, vec![]);
        watch_movie(&db, id, NOW, false).unwrap();
        assert_eq!(stats(&db, Range::AllTime).movies.watches, 1);
        let event = recent_history(&db, 1).unwrap().remove(0);
        delete_event(&db, event.event_id).unwrap();
        let s = stats(&db, Range::AllTime);
        assert!(title_state(&db, id).unwrap().watched_at.is_some());
        assert!(!s.has_history);
        assert_eq!(s.movies, Viewing::default());
        assert_eq!(s.library.movies_watched, 1, "current state unchanged");
    }

    #[test]
    fn episodes_count_one_by_one_with_specials_and_rewatches() {
        let db = Database::open_in_memory();
        let id = show(&db, 1, &[(0, 2, 2), (1, 3, 3)], vec![]);
        watch_season(&db, id, 1, NOW).unwrap();
        watch_episode(&db, id, 0, 1, NOW, false).unwrap();
        watch_episode(&db, id, 1, 2, NOW, true).unwrap();
        let movie = film(&db, 603, vec![]);
        watch_movie(&db, movie, NOW, false).unwrap();

        let s = stats(&db, Range::Days30);
        // Specials are watches and watch time like any episode.
        assert_eq!(s.episodes, viewing(5, 4, 1, 5 * 62, 0));
        assert_eq!(s.movies, viewing(1, 1, 0, 136, 0));
        assert_eq!(s.total(), viewing(6, 5, 1, 5 * 62 + 136, 0));
        assert_eq!(s.library.episodes_watched, 4);

        // Unmarking an episode keeps its history.
        unwatch_episode(&db, id, 1, 2).unwrap();
        let s = stats(&db, Range::Days30);
        assert_eq!(s.episodes.watches, 5);
        assert_eq!(s.library.episodes_watched, 3);
    }

    #[test]
    fn runtime_is_kept_per_event_and_unknown_runtime_is_counted_apart() {
        let db = Database::open_in_memory();
        let id = film(&db, 603, vec![]);
        watch_movie(&db, id, NOW, false).unwrap();
        // TMDB changes the runtime: the recorded watch keeps 136.
        let mut changed = movie("Movie", vec![]);
        changed.runtime_minutes = Some(150);
        save(&db, id, &changed, NOW).unwrap();
        assert_eq!(stats(&db, Range::AllTime).movies.known_minutes, 136);
        // A rewatch records the runtime known now.
        watch_movie(&db, id, NOW, true).unwrap();
        assert_eq!(stats(&db, Range::AllTime).movies.known_minutes, 286);

        // Watched before any runtime was known: disclosed, not zero minutes.
        let bare = stored(&db, MediaType::Movie, 604, "No details");
        watch_movie(&db, bare, NOW, false).unwrap();
        assert_eq!(stats(&db, Range::AllTime).movies, viewing(3, 2, 1, 286, 1));
        // Its details arrive later: that event uses the runtime now stored.
        save(&db, bare, &movie("No details", vec![]), NOW).unwrap();
        assert_eq!(stats(&db, Range::AllTime).movies, viewing(3, 2, 1, 422, 0));

        // An episode without its own runtime is unknown; the series' typical
        // episode length is not used.
        let show = stored(&db, MediaType::Tv, 1, "Show");
        save(&db, show, &series("Show", vec![season(1, Some(1))]), NOW).unwrap();
        let mut untimed = episode(1, 1);
        untimed.runtime_minutes = None;
        save_episodes(&db, show, 1, &[untimed], NOW).unwrap();
        watch_episode(&db, show, 1, 1, NOW, false).unwrap();
        assert_eq!(stats(&db, Range::AllTime).episodes, viewing(1, 1, 0, 0, 1));
    }

    #[test]
    fn history_ignores_library_membership_and_current_state_does_not() {
        let db = Database::open_in_memory();
        let a = film(&db, 1, vec![genre("18", "Drama")]);
        let _b = film(&db, 2, vec![]);
        watch_movie(&db, a, NOW, false).unwrap();
        set_rating(&db, a, Some(9)).unwrap();
        library::remove(&db, a).unwrap();

        let s = stats(&db, Range::AllTime);
        assert_eq!(s.movies.watches, 1, "A still counts in history");
        assert_eq!(s.genres[0].name.as_deref(), Some("Drama"));
        assert_eq!(
            (
                s.library.movies,
                s.library.movies_watched,
                s.library.rated()
            ),
            (1, 0, 0),
            "only B is in the Library now"
        );
        // Re-adding A brings its current state back.
        stored(&db, MediaType::Movie, 1, "Movie 1");
        let s = stats(&db, Range::AllTime);
        assert_eq!((s.library.movies_watched, s.library.rated()), (1, 1));
        assert_eq!(s.movies.watches, 1);
    }

    #[test]
    fn a_watch_counts_toward_every_genre_its_title_has_now() {
        let db = Database::open_in_memory();
        let drama = genre("18", "Drama");
        let m = film(&db, 1, vec![drama.clone(), genre("80", "Crime")]);
        let tv = show(
            &db,
            2,
            &[(1, 2, 2)],
            vec![drama, genre("10765", "Sci-Fi & Fantasy")],
        );
        let plain = film(&db, 3, vec![]);
        watch_movie(&db, m, NOW, false).unwrap();
        watch_episode(&db, tv, 1, 1, NOW, false).unwrap();
        watch_movie(&db, plain, NOW, false).unwrap();

        let genres = |db: &Database| -> Vec<(Option<String>, u32, u64)> {
            stats(db, Range::AllTime)
                .genres
                .into_iter()
                .map(|g| (g.name, g.watches, g.known_minutes))
                .collect()
        };
        assert_eq!(
            genres(&db),
            [
                (Some("Drama".into()), 2, 136 + 62),
                (Some("Crime".into()), 1, 136),
                (Some("Sci-Fi & Fantasy".into()), 1, 62),
                (None, 1, 136),
            ],
            "3 watches, 5 genre counts"
        );
        // A refresh that changes genres re-categorises past watches.
        save(&db, m, &movie("Movie", vec![genre("35", "Comedy")]), NOW).unwrap();
        assert_eq!(
            genres(&db),
            [
                (Some("Comedy".into()), 1, 136),
                (Some("Drama".into()), 1, 62),
                (Some("Sci-Fi & Fantasy".into()), 1, 62),
                (None, 1, 136),
            ]
        );
    }

    #[test]
    fn ratings_average_rated_titles_only_movies_and_series_alike() {
        let db = Database::open_in_memory();
        let (a, b) = (film(&db, 1, vec![]), film(&db, 2, vec![]));
        let _unrated = film(&db, 3, vec![]);
        set_rating(&db, a, Some(10)).unwrap();
        set_rating(&db, b, Some(8)).unwrap();
        let s = stats(&db, Range::AllTime);
        assert_eq!(s.library.rated(), 2);
        assert_eq!(s.library.average_rating(), Some(9.0), "not 6.0");

        // A rewatch does not rate twice; a series counts once too.
        watch_movie(&db, a, NOW, false).unwrap();
        watch_movie(&db, a, NOW, true).unwrap();
        let tv = show(&db, 4, &[], vec![]);
        set_rating(&db, tv, Some(6)).unwrap();
        let s = stats(&db, Range::AllTime);
        assert_eq!(s.library.ratings, [0, 0, 0, 0, 0, 1, 0, 1, 0, 1]);
        assert_eq!(s.library.average_rating(), Some(8.0));
        set_rating(&db, tv, None).unwrap();
        assert_eq!(stats(&db, Range::AllTime).library.rated(), 2);
    }

    #[test]
    fn series_progress_reuses_coverage_aware_completion() {
        let db = Database::open_in_memory();
        let _not_started = show(&db, 1, &[(1, 2, 2)], vec![]);
        let specials_only = show(&db, 2, &[(0, 1, 1), (1, 2, 2)], vec![]);
        watch_episode(&db, specials_only, 0, 1, NOW, false).unwrap();
        let in_progress = show(&db, 3, &[(1, 2, 2)], vec![]);
        watch_episode(&db, in_progress, 1, 1, NOW, false).unwrap();
        // Every known episode watched, but TMDB counts 5: not complete.
        let partial = show(&db, 4, &[(1, 5, 3)], vec![]);
        watch_season(&db, partial, 1, NOW).unwrap();
        // Complete; its unwatched specials do not matter.
        let complete = show(&db, 5, &[(0, 3, 3), (1, 2, 2)], vec![]);
        watch_season(&db, complete, 1, NOW).unwrap();

        let library = |db: &Database| {
            let l = stats(db, Range::AllTime).library;
            (
                l.series,
                l.series_complete,
                l.series_in_progress,
                l.series_not_started,
                l.series_incomplete_coverage,
            )
        };
        assert_eq!(library(&db), (5, 1, 2, 2, 1));

        // A new episode reopens the complete series.
        save(
            &db,
            complete,
            &series("Show", vec![season(0, Some(3)), season(1, Some(3))]),
            NOW,
        )
        .unwrap();
        assert_eq!(library(&db), (5, 0, 3, 2, 2));
    }

    #[test]
    fn time_ranges_use_local_calendar_boundaries() {
        let db = Database::open_in_memory();
        let ago = [0, 29 * DAY, 31 * DAY, 335 * DAY, 395 * DAY];
        for (n, before) in (1..).zip(ago) {
            let id = film(&db, n, vec![]);
            watch_movie(&db, id, NOW - before, false).unwrap();
        }
        for (range, expected, buckets) in [
            (Range::Days30, 2, 30),
            (Range::Months12, 4, 12),
            (Range::AllTime, 5, 3),
        ] {
            let s = stats(&db, range);
            assert_eq!(s.movies.watches, expected, "{range:?}");
            assert_eq!(s.activity.len(), buckets, "{range:?}");
            let charted: u32 = s.activity.iter().map(|b| b.watches).sum();
            assert_eq!(charted, expected, "{range:?}: every watch has a bar");
        }
        // The last bar is today, this month, this year, in local time.
        let today = tracking::local_time(&db, NOW).unwrap();
        let days = stats(&db, Range::Days30).activity;
        assert_eq!((days[29].key.as_str(), days[29].watches), (&today[..10], 1));
        assert_eq!(stats(&db, Range::Months12).activity[11].key, today[..7]);
        assert_eq!(stats(&db, Range::AllTime).activity[2].key, today[..4]);

        // A rewatch in range whose first watch is older is still a rewatch.
        let old = film(&db, 99, vec![]);
        watch_movie(&db, old, NOW - 40 * DAY, false).unwrap();
        watch_movie(&db, old, NOW - DAY, true).unwrap();
        let s = stats(&db, Range::Days30);
        assert_eq!((s.movies.watches, s.movies.rewatches), (3, 1));
    }

    #[test]
    fn statistics_cover_the_whole_history_not_the_history_page() {
        let db = Database::open_in_memory();
        let id = film(&db, 603, vec![]);
        watch_movie(&db, id, NOW - 500, false).unwrap();
        let again = crate::history::RECENT as u32 + 50;
        for n in 0..again {
            watch_movie(&db, id, NOW - i64::from(n), true).unwrap();
        }
        assert_eq!(
            stats(&db, Range::AllTime).movies,
            viewing(again + 1, 1, again, 136 * u64::from(again + 1), 0)
        );
    }

    thread_local! {
        static STATEMENTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    fn count_statement(_: rusqlite::trace::TraceEvent<'_>) {
        STATEMENTS.with(|n| n.set(n.get() + 1));
    }

    /// The SQL statements one `load` runs.
    fn statements(db: &Database) -> usize {
        use rusqlite::trace::TraceEventCodes;
        db.conn()
            .trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, Some(count_statement));
        STATEMENTS.with(|n| n.set(0));
        stats(db, Range::AllTime);
        db.conn().trace_v2(TraceEventCodes::empty(), None);
        STATEMENTS.with(|n| n.get())
    }

    #[test]
    fn the_number_of_queries_does_not_grow_with_the_data() {
        let db = Database::open_in_memory();
        let empty = statements(&db);
        for n in 1..=40 {
            let genres = vec![
                genre("18", "Drama"),
                genre(&n.to_string(), &format!("G{n}")),
            ];
            let m = film(&db, n, genres.clone());
            watch_movie(&db, m, NOW - i64::from(n) * DAY, false).unwrap();
            set_rating(&db, m, Some((n % 10 + 1) as u8)).unwrap();
            let tv = show(&db, 1000 + n, &[(0, 2, 2), (1, 5, 5), (2, 5, 3)], genres);
            watch_season(&db, tv, 1, NOW - i64::from(n) * 30 * DAY).unwrap();
        }
        assert_eq!(statements(&db), empty, "no statement per title or event");
        assert!((5..=7).contains(&empty), "{empty} statements traced");
    }

    /// A test-only synthetic history in `db`: 1,000 titles (850 movies, 150
    /// series), 17,550 episodes, 19 genres, ratings, rewatches and just over
    /// 10,000 watch events spread over three years. Never seeded in the app.
    pub fn large_history(db: &Database) {
        let genres: Vec<Genre> = (1..=19)
            .map(|n| genre(&n.to_string(), &format!("Genre {n}")))
            .collect();
        let pick = |n: u32| {
            vec![
                genres[n as usize % 19].clone(),
                genres[(n as usize * 7 + 3) % 19].clone(),
            ]
        };
        let spread = |n: i64| NOW - (n * 7_919) % (3 * 365 * DAY);
        db.conn().pragma_update(None, "synchronous", "OFF").unwrap();
        let mut n = 0;
        for tmdb in 1..=850 {
            let id = film(db, tmdb, pick(tmdb));
            watch_movie(db, id, spread(n), false).unwrap();
            n += 1;
            if tmdb % 2 == 0 {
                watch_movie(db, id, spread(n), true).unwrap();
                n += 1;
            }
            if tmdb % 3 == 0 {
                set_rating(db, id, Some((tmdb % 10 + 1) as u8)).unwrap();
            }
        }
        for s in 0..150 {
            // 5 regular seasons of 23 episodes and 2 specials; every tenth
            // series has one season not fully downloaded.
            let last = if s % 10 == 0 { 20 } else { 23 };
            let id = show(
                db,
                10_000 + s,
                &[
                    (0, 2, 2),
                    (1, 23, 23),
                    (2, 23, 23),
                    (3, 23, 23),
                    (4, 23, 23),
                    (5, 23, last),
                ],
                pick(s),
            );
            for season in 1..=(s % 6) as i64 {
                n += 1;
                watch_season(db, id, season, spread(n)).unwrap();
            }
            if s % 5 == 0 {
                set_rating(db, id, Some((s % 10 + 1) as u8)).unwrap();
            }
        }
        let mut again = 0;
        while rows(db) < 10_000 {
            let id = 851 + i64::from(again % 150);
            watch_episode(db, id, 1, i64::from(again % 23) + 1, spread(n), true).unwrap();
            (n, again) = (n + 1, again + 1);
        }
        db.conn()
            .pragma_update(None, "synchronous", "FULL")
            .unwrap();
    }

    fn rows(db: &Database) -> i64 {
        db.conn()
            .query_row("SELECT count(*) FROM watch_events", [], |r| r.get(0))
            .unwrap()
    }

    /// Informal R11 observation, not a `BENCHMARK_SPEC.md` run. Run with
    /// `cargo test --release informal_statistics_timings -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn informal_statistics_timings() {
        use crate::diagnostics::Log;
        use crate::paths::TestDir;
        use std::time::{Duration, Instant};

        let dir = TestDir::new("statistics-timings");
        let path = dir.0.join("bingee.db");
        let started = Instant::now();
        large_history(&Database::open(&path, &Log::stderr_only()).unwrap());
        println!("fixture built in {:.1} s", started.elapsed().as_secs_f64());
        let db = Database::open(&path, &Log::stderr_only()).unwrap();
        let count = |table: &str| -> i64 {
            db.conn()
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
                .unwrap()
        };
        println!(
            "media {} · episodes {} · watch events {} · genres {} · rated {}",
            count("media"),
            count("episodes"),
            count("watch_events"),
            count("genres"),
            count("media_tracking WHERE rating IS NOT NULL")
        );

        let time = |label: &str, f: &mut dyn FnMut()| {
            f(); // warm the statement cache and the page cache
            let mut samples: Vec<Duration> = (0..30)
                .map(|_| {
                    let start = Instant::now();
                    f();
                    start.elapsed()
                })
                .collect();
            samples.sort();
            println!(
                "{label:<40} median {:>7.3} ms  max {:>7.3} ms (30 runs)",
                samples[15].as_secs_f64() * 1e3,
                samples[29].as_secs_f64() * 1e3
            );
        };
        let conn = db.conn();
        for range in Range::ALL {
            time(&format!("load, {range:?}"), &mut || {
                std::hint::black_box(load(&db, range, NOW).unwrap());
            });
        }
        let start = range_start(conn, Range::Months12, NOW).unwrap();
        time("viewing totals, 12 months", &mut || {
            std::hint::black_box(super::viewing(conn, start).unwrap());
        });
        time("viewing totals, all time", &mut || {
            std::hint::black_box(super::viewing(conn, i64::MIN).unwrap());
        });
        time("activity, 12 months", &mut || {
            std::hint::black_box(activity(conn, Range::Months12, NOW).unwrap());
        });
        time("genres, all time", &mut || {
            std::hint::black_box(genres(conn, i64::MIN).unwrap());
        });
        time("library now (progress + ratings)", &mut || {
            std::hint::black_box(library_now(&db).unwrap());
        });
        let s = load(&db, Range::AllTime, NOW).unwrap();
        println!("{:?}\n{:?}\n{:?}", s.movies, s.episodes, s.library);
        // Query plans: `R11_PLANS=1`.
        if std::env::var_os("R11_PLANS").is_some() {
            conn.trace_v2(
                rusqlite::trace::TraceEventCodes::SQLITE_TRACE_STMT,
                Some(|event| {
                    if let rusqlite::trace::TraceEvent::Stmt(_, sql) = event {
                        PLANS.with(|p| p.borrow_mut().push(sql.to_owned()));
                    }
                }),
            );
            load(&db, Range::Months12, NOW).unwrap();
            conn.trace_v2(rusqlite::trace::TraceEventCodes::empty(), None);
            for sql in PLANS.with(|p| p.take()) {
                println!(
                    "--- {}",
                    sql.split_whitespace().collect::<Vec<_>>().join(" ")
                );
                let mut stmt = conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}")).unwrap();
                let params = std::iter::repeat_n(1_i64, stmt.parameter_count());
                let mut rows = stmt.query(rusqlite::params_from_iter(params)).unwrap();
                while let Some(row) = rows.next().unwrap() {
                    println!("    {}", row.get::<_, String>(3).unwrap());
                }
            }
        }
    }

    thread_local! {
        static PLANS: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
    }
}
