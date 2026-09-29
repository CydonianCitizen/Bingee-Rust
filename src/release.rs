//! Persistent in-app release events. Provider refresh only inserts; reading
//! is personal state and never changes during a metadata save.

use rusqlite::params;

use crate::database::Database;
use crate::error::AppError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseEvent {
    pub id: i64,
    pub media_id: i64,
    pub series: String,
    pub season: i64,
    pub episode: i64,
    pub name: Option<String>,
    pub kind: String,
    pub discovered_at: i64,
    pub local_time: String,
    pub air_date: Option<String>,
    pub read_at: Option<i64>,
}

fn read_error(error: rusqlite::Error) -> AppError {
    AppError::database("Updates could not be read.", error)
}

fn write_error(error: rusqlite::Error) -> AppError {
    AppError::database("Updates could not be marked read.", error)
}

pub fn recent(db: &Database, limit: usize) -> Result<Vec<ReleaseEvent>, AppError> {
    let mut stmt = db
        .conn()
        .prepare_cached(
            "SELECT r.event_id, r.local_media_id, m.title, r.season_number,
                r.episode_number, e.name, r.event_type, r.discovered_at,
                strftime('%Y-%m-%d %H:%M', r.discovered_at, 'unixepoch', 'localtime'),
                r.air_date, r.read_at
         FROM release_events AS r JOIN media AS m ON m.local_media_id = r.local_media_id
         LEFT JOIN episodes AS e ON e.local_media_id = r.local_media_id
             AND e.season_number = r.season_number AND e.episode_number = r.episode_number
         ORDER BY r.discovered_at DESC, r.event_id DESC LIMIT ?1",
        )
        .map_err(read_error)?;
    stmt.query_map([limit as i64], |r| {
        Ok(ReleaseEvent {
            id: r.get(0)?,
            media_id: r.get(1)?,
            series: r.get(2)?,
            season: r.get(3)?,
            episode: r.get(4)?,
            name: r.get(5)?,
            kind: r.get(6)?,
            discovered_at: r.get(7)?,
            local_time: r.get(8)?,
            air_date: r.get(9)?,
            read_at: r.get(10)?,
        })
    })
    .map_err(read_error)?
    .collect::<rusqlite::Result<_>>()
    .map_err(read_error)
}

pub fn unread_count(db: &Database) -> Result<u32, AppError> {
    db.conn()
        .query_row(
            "SELECT count(*) FROM release_events WHERE read_at IS NULL",
            [],
            |r| r.get(0),
        )
        .map_err(read_error)
}

pub fn mark_read(db: &Database, event_id: i64, now: i64) -> Result<bool, AppError> {
    db.conn()
        .execute(
            "UPDATE release_events SET read_at = ?2
        WHERE event_id = ?1 AND read_at IS NULL",
            params![event_id, now],
        )
        .map(|n| n > 0)
        .map_err(write_error)
}

pub fn mark_all_read(db: &Database, now: i64) -> Result<usize, AppError> {
    db.conn()
        .execute(
            "UPDATE release_events SET read_at = ?1 WHERE read_at IS NULL",
            [now],
        )
        .map_err(write_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use crate::library::MediaType;
    use crate::metadata::tests::{episode, season, series, stored};
    use crate::metadata::{save, save_episodes};
    use crate::paths::TestDir;
    use crate::tracking;

    #[test]
    fn new_episode_is_one_event_and_read_state_survives_refresh_and_restart() {
        let dir = TestDir::new("release-events");
        let path = dir.0.join("bingee.db");
        let db = Database::open(&path, &crate::diagnostics::Log::stderr_only()).unwrap();
        let show = stored(&db, MediaType::Tv, 12, "A show");
        save(&db, show, &series("A show", vec![season(1, Some(10))]), 1).unwrap();
        let mut episodes: Vec<_> = (1..=10).map(|n| episode(1, n)).collect();
        save_episodes(&db, show, 1, &episodes, 1).unwrap();
        assert!(
            recent(&db, 10).unwrap().is_empty(),
            "initial download is not a release"
        );
        for n in 1..=10 {
            tracking::watch_episode(&db, show, 1, n, 2, false).unwrap();
        }
        assert!(tracking::series_progress(&db, show).unwrap().is_complete());
        save(&db, show, &series("A show", vec![season(1, Some(11))]), 3).unwrap();
        let mut next = episode(1, 11);
        next.air_date = Some("2026-09-30".into());
        episodes.push(next);
        save_episodes(&db, show, 1, &episodes, 4).unwrap();
        assert_eq!(unread_count(&db).unwrap(), 1);
        let event = recent(&db, 10).unwrap().remove(0);
        assert_eq!((event.season, event.episode), (1, 11));
        assert_eq!(event.air_date.as_deref(), Some("2026-09-30"));
        assert_eq!(
            tracking::next_episode(&db, show, &tracking::series_progress(&db, show).unwrap())
                .unwrap(),
            tracking::NextEpisode::Episode {
                season: 1,
                number: 11,
                name: Some("Episode 11".into())
            }
        );
        assert_eq!(
            crate::calendar::events(&db, "2026-09-30", "2026-10-01")
                .unwrap()
                .last()
                .unwrap()
                .episode,
            11
        );
        assert!(mark_read(&db, event.id, 5).unwrap());
        save_episodes(&db, show, 1, &episodes, 6).unwrap();
        assert_eq!(unread_count(&db).unwrap(), 0);
        assert_eq!(recent(&db, 10).unwrap().len(), 1);
        drop(db);
        let reopened = Database::open(&path, &crate::diagnostics::Log::stderr_only()).unwrap();
        assert_eq!(recent(&reopened, 10).unwrap()[0].read_at, Some(5));
    }
}
