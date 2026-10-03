//! Offline Home snapshot. Each collection uses a fixed number of SQL
//! statements, independent of Library size.

use std::collections::HashMap;

use crate::calendar;
use crate::database::Database;
use crate::error::AppError;
use crate::library::{self, Query};
use crate::tracking::{self, Status};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub section: &'static str,
    pub title: String,
    pub detail: String,
    pub media_id: i64,
    pub season: Option<i64>,
    pub episode: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct Snapshot {
    pub summary: String,
    pub entries: Vec<Entry>,
    pub incomplete_coverage: bool,
}

type NextMap = HashMap<i64, (i64, i64, Option<String>)>;

/// First known unwatched regular episode per Library series, in one query.
fn next_known(db: &Database) -> Result<NextMap, AppError> {
    let mut stmt = db
        .conn()
        .prepare_cached(
            "WITH unwatched AS (
            SELECT e.local_media_id, e.season_number, e.episode_number, e.name,
                   row_number() OVER (PARTITION BY e.local_media_id
                       ORDER BY e.season_number, e.episode_number) AS position
            FROM episodes AS e
            JOIN library_entries AS l ON l.local_media_id = e.local_media_id
            WHERE e.season_number >= 1 AND NOT EXISTS (
                SELECT 1 FROM episode_tracking AS t
                WHERE t.local_media_id = e.local_media_id
                  AND t.season_number = e.season_number
                  AND t.episode_number = e.episode_number)
         ) SELECT local_media_id, season_number, episode_number, name
           FROM unwatched WHERE position = 1",
        )
        .map_err(|e| AppError::database("Home could not read next episodes.", e))?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, (r.get(1)?, r.get(2)?, r.get(3)?))))
        .map_err(|e| AppError::database("Home could not read next episodes.", e))?;
    rows.collect::<rusqlite::Result<_>>()
        .map_err(|e| AppError::database("Home could not read next episodes.", e))
}

pub fn load(db: &Database, now: i64) -> Result<Snapshot, AppError> {
    let titles = library::search(db, &Query::default())?;
    let status = tracking::library_status(db)?;
    let next = next_known(db)?;
    let names: HashMap<_, _> = titles.iter().map(|m| (m.id, m.title.as_str())).collect();
    let continue_ids = tracking::continue_watching(db)?;
    let today = calendar::local_date(db, now)?;
    let upcoming = calendar::coming_soon(db, &today)?;
    let history = tracking::recent_history(db, 8)?;
    let movies = titles
        .iter()
        .filter(|m| m.media_type == library::MediaType::Movie)
        .count();
    let series = titles.len() - movies;
    let incomplete_coverage = status
        .values()
        .any(|s| matches!(s, Status::Series(p) if !p.coverage_complete()));
    let mut entries = Vec::new();
    for id in continue_ids.iter().take(8) {
        if let (Some(title), Some(Status::Series(progress))) = (names.get(id), status.get(id)) {
            entries.push(Entry {
                section: "Continue Watching",
                title: (*title).to_owned(),
                detail: format!(
                    "{} / {} known regular episodes watched{}",
                    progress.watched(),
                    progress.known(),
                    if progress.coverage_complete() {
                        ""
                    } else {
                        " · metadata incomplete"
                    }
                ),
                media_id: *id,
                season: None,
                episode: None,
            });
        }
    }
    for id in continue_ids.iter().take(8) {
        if let (Some(title), Some((season, episode, name))) = (names.get(id), next.get(id)) {
            entries.push(Entry {
                section: "Up Next",
                title: (*title).to_owned(),
                detail: format!(
                    "S{season} E{episode}{}",
                    name.as_ref().map_or(String::new(), |n| format!(" · {n}"))
                ),
                media_id: *id,
                season: Some(*season),
                episode: Some(*episode),
            });
        }
    }
    // Caught-up series remain visible with their coverage uncertainty.
    for (id, progress) in status
        .iter()
        .filter_map(|(id, s)| match s {
            Status::Series(p)
                if p.watched() > 0 && p.known() == p.watched() && !p.coverage_complete() =>
            {
                Some((id, p))
            }
            _ => None,
        })
        .take(8)
    {
        if let Some(title) = names.get(id) {
            entries.push(Entry {
                section: "Up Next",
                title: (*title).to_owned(),
                detail: format!(
                    "Caught up on {} known episodes · metadata incomplete",
                    progress.known()
                ),
                media_id: *id,
                season: None,
                episode: None,
            });
        }
    }
    for event in history {
        let detail = match event.episode {
            Some((season, episode)) => format!("S{season} E{episode} · {}", event.local_time),
            None => format!("Movie · {}", event.local_time),
        };
        entries.push(Entry {
            section: "Recently Watched",
            title: event.title,
            detail,
            media_id: event.local_media_id,
            season: event.episode.map(|e| e.0),
            episode: event.episode.map(|e| e.1),
        });
    }
    for event in upcoming.into_iter().take(8) {
        entries.push(Entry {
            section: "Coming Soon",
            title: event.series,
            detail: format!(
                "{} · S{} E{}{}",
                event.date,
                event.season,
                event.episode,
                event
                    .name
                    .as_ref()
                    .map_or(String::new(), |n| format!(" · {n}"))
            ),
            media_id: event.media_id,
            season: Some(event.season),
            episode: Some(event.episode),
        });
    }
    Ok(Snapshot {
        summary: format!(
            "{movies} {} · {series} series in Library",
            if movies == 1 { "movie" } else { "movies" }
        ),
        entries,
        incomplete_coverage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::MediaType;
    use crate::metadata::tests::{episode, season, series, stored};
    use crate::metadata::{save, save_episodes};
    use rusqlite::trace::{TraceEvent, TraceEventCodes};
    use std::cell::Cell;

    thread_local! { static QUERIES: Cell<usize> = const { Cell::new(0) }; }

    fn count(event: TraceEvent<'_>) {
        if let TraceEvent::Stmt(_, _) = event {
            QUERIES.with(|n| n.set(n.get() + 1));
        }
    }

    fn query_count(db: &Database, now: i64) -> usize {
        db.conn()
            .trace_v2(TraceEventCodes::SQLITE_TRACE_STMT, Some(count));
        QUERIES.with(|n| n.set(0));
        load(db, now).unwrap();
        db.conn().trace_v2(TraceEventCodes::empty(), None);
        QUERIES.with(Cell::get)
    }

    #[test]
    fn empty_home_is_local_and_clear() {
        let db = Database::open_in_memory();
        let home = load(&db, 1_800_000_000).unwrap();
        assert!(home.entries.is_empty());
        assert_eq!(home.summary, "0 movies · 0 series in Library");
        assert!(!home.incomplete_coverage);
    }

    #[test]
    fn home_shows_progress_next_history_upcoming_and_uncertainty() {
        let db = Database::open_in_memory();
        let id = stored(&db, MediaType::Tv, 30, "A show");
        save(&db, id, &series("A show", vec![season(1, Some(3))]), 1).unwrap();
        let mut episodes: Vec<_> = (1..=3).map(|n| episode(1, n)).collect();
        episodes[2].air_date = Some("2026-09-29".into());
        save_episodes(&db, id, 1, &episodes, 1).unwrap();
        tracking::watch_episode(&db, id, 1, 1, 1_800_000_000, false).unwrap();
        let now: i64 = db
            .conn()
            .query_row(
                "SELECT CAST(strftime('%s', '2026-09-20 12:00:00') AS INTEGER)",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let home = load(&db, now).unwrap();
        assert!(
            home.entries
                .iter()
                .any(|e| e.section == "Continue Watching" && e.media_id == id)
        );
        assert!(
            home.entries
                .iter()
                .any(|e| e.section == "Up Next" && e.episode == Some(2))
        );
        assert!(home.entries.iter().any(|e| e.section == "Recently Watched"));
        assert!(
            home.entries
                .iter()
                .any(|e| e.section == "Coming Soon" && e.episode == Some(3))
        );
        assert!(!home.incomplete_coverage);
        tracking::watch_episode(&db, id, 1, 2, now, false).unwrap();
        tracking::watch_episode(&db, id, 1, 3, now, false).unwrap();
        let complete = load(&db, now).unwrap();
        assert!(
            !complete
                .entries
                .iter()
                .any(|e| e.section == "Continue Watching")
        );
        assert!(
            !complete
                .entries
                .iter()
                .any(|e| e.section == "Up Next" && e.media_id == id)
        );
        let mut fourth = episode(1, 4);
        fourth.air_date = Some("2026-09-30".into());
        episodes.push(fourth);
        save(&db, id, &series("A show", vec![season(1, Some(4))]), 2).unwrap();
        save_episodes(&db, id, 1, &episodes, 2).unwrap();
        let reopened = load(&db, now).unwrap();
        assert!(
            reopened
                .entries
                .iter()
                .any(|e| e.section == "Up Next" && e.episode == Some(4))
        );
    }

    #[test]
    fn partial_coverage_never_claims_complete() {
        let db = Database::open_in_memory();
        let id = stored(&db, MediaType::Tv, 40, "Partial");
        save(&db, id, &series("Partial", vec![season(1, Some(2))]), 1).unwrap();
        save_episodes(&db, id, 1, &[episode(1, 1)], 1).unwrap();
        tracking::watch_episode(&db, id, 1, 1, 1_800_000_000, false).unwrap();
        let home = load(&db, 1_800_000_000).unwrap();
        assert!(home.incomplete_coverage);
        assert!(
            !home
                .entries
                .iter()
                .any(|e| e.section == "Continue Watching")
        );
        assert!(
            home.entries
                .iter()
                .any(|e| e.section == "Up Next" && e.detail.contains("metadata incomplete"))
        );
    }

    #[test]
    fn home_query_count_does_not_grow_with_titles() {
        let db = Database::open_in_memory();
        let empty = query_count(&db, 1_800_000_000);
        for n in 1..=20 {
            let id = stored(&db, MediaType::Tv, 1000 + n, &format!("Show {n}"));
            save(&db, id, &series("Show", vec![season(1, Some(2))]), 1).unwrap();
            save_episodes(&db, id, 1, &[episode(1, 1), episode(1, 2)], 1).unwrap();
            tracking::watch_episode(&db, id, 1, 1, 1_800_000_000, false).unwrap();
        }
        assert_eq!(query_count(&db, 1_800_000_000), empty);
        assert!(empty <= 10, "{empty} statements");
    }

    #[test]
    #[ignore]
    fn informal_large_home_and_calendar_timings() {
        use crate::statistics::tests::large_history;
        use std::time::Instant;
        let db = Database::open_in_memory();
        large_history(&db);
        let now = 1_800_000_000;
        let today = calendar::local_date(&db, now).unwrap();
        for (name, operation) in [
            ("Home", load(&db, now).map(|_| ())),
            ("Calendar", calendar::month(&db, &today, &today).map(|_| ())),
        ] {
            operation.unwrap();
            let start = Instant::now();
            for _ in 0..20 {
                if name == "Home" {
                    load(&db, now).unwrap();
                } else {
                    calendar::month(&db, &today, &today).unwrap();
                }
            }
            println!(
                "{name}: {:.2} ms/operation, 20 warm runs",
                start.elapsed().as_secs_f64() * 50.0
            );
        }
    }
}
