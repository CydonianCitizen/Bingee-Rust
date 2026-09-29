//! Local episode dates. Provider dates stay date strings; watch timestamps are
//! UTC instants and are never used to shift an episode's air date.

use rusqlite::params;

use crate::database::Database;
use crate::error::{AppError, ErrorKind};
use crate::tracking::{self, Status};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub media_id: i64,
    pub series: String,
    pub season: i64,
    pub episode: i64,
    pub name: Option<String>,
    pub date: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Day {
    pub date: String,
    pub in_month: bool,
    pub count: usize,
    pub today: bool,
}

#[derive(Debug, Clone)]
pub struct Month {
    pub label: String,
    pub first: String,
    pub days: Vec<Day>,
    pub events: Vec<Event>,
    pub incomplete_coverage: bool,
}

fn failed(err: rusqlite::Error) -> AppError {
    AppError::database("Calendar could not read local episodes.", err)
}

/// Local date of one UTC instant. Used only to choose today's calendar page.
pub fn local_date(db: &Database, now: i64) -> Result<String, AppError> {
    db.conn()
        .query_row("SELECT date(?1, 'unixepoch', 'localtime')", [now], |r| {
            r.get(0)
        })
        .map_err(failed)
}

/// Read Library episode metadata in a half-open date range. One statement.
pub fn events(db: &Database, start: &str, end: &str) -> Result<Vec<Event>, AppError> {
    let mut stmt = db
        .conn()
        .prepare_cached(
            "SELECT e.local_media_id, m.title, e.season_number, e.episode_number,
                e.name, e.air_date
         FROM episodes AS e
         JOIN library_entries AS l ON l.local_media_id = e.local_media_id
         JOIN media AS m ON m.local_media_id = e.local_media_id AND m.media_type = 'tv'
         WHERE e.air_date >= ?1 AND e.air_date < ?2
         ORDER BY e.air_date, bingee_fold(m.title), e.season_number,
                  e.episode_number, e.local_media_id",
        )
        .map_err(failed)?;
    stmt.query_map(params![start, end], |r| {
        Ok(Event {
            media_id: r.get(0)?,
            series: r.get(1)?,
            season: r.get(2)?,
            episode: r.get(3)?,
            name: r.get(4)?,
            date: r.get(5)?,
        })
    })
    .map_err(failed)?
    .collect::<rusqlite::Result<_>>()
    .map_err(failed)
}

/// Inclusive today through the following 14 days, using provider dates as-is.
pub fn coming_soon(db: &Database, today: &str) -> Result<Vec<Event>, AppError> {
    let end: String = db
        .conn()
        .query_row("SELECT date(?1, '+15 days')", [today], |r| r.get(0))
        .map_err(failed)?;
    events(db, today, &end)
}

pub fn shift_month(db: &Database, first: &str, amount: i32) -> Result<String, AppError> {
    db.conn()
        .query_row(
            "SELECT date(?1, ?2)",
            params![first, format!("{amount:+} months")],
            |r| r.get(0),
        )
        .map_err(failed)
}

/// Monday-first, 42-cell month. One date statement, one event statement, one
/// aggregate progress statement. No query per cell or event.
pub fn month(db: &Database, date: &str, today: &str) -> Result<Month, AppError> {
    let first: String = db
        .conn()
        .query_row("SELECT date(?1, 'start of month')", [date], |r| r.get(0))
        .map_err(failed)?;
    if first.len() != 10 {
        return Err(AppError::new(
            ErrorKind::InvalidData,
            "Invalid calendar date.",
        ));
    }
    let end = shift_month(db, &first, 1)?;
    let label = first[..7].to_owned();
    let events = events(db, &first, &end)?;
    let mut stmt = db
        .conn()
        .prepare_cached(
            "WITH RECURSIVE dates(n, day) AS (
            SELECT 0, date(?1, 'start of month',
                printf('-%d days', (CAST(strftime('%w', ?1) AS INTEGER) + 6) % 7))
            UNION ALL SELECT n + 1, date(day, '+1 day') FROM dates WHERE n < 41
         ) SELECT day FROM dates ORDER BY n",
        )
        .map_err(failed)?;
    let mut days: Vec<Day> = stmt
        .query_map([&first], |r| r.get::<_, String>(0))
        .map_err(failed)?
        .map(|r| {
            r.map(|date| Day {
                in_month: date >= first && date < end,
                today: date == today,
                date,
                count: 0,
            })
        })
        .collect::<rusqlite::Result<_>>()
        .map_err(failed)?;
    for event in &events {
        if let Some(day) = days.iter_mut().find(|day| day.date == event.date) {
            day.count += 1;
        }
    }
    let incomplete_coverage = tracking::library_status(db)?
        .values()
        .any(|status| matches!(status, Status::Series(progress) if !progress.coverage_complete()));
    Ok(Month {
        label,
        first,
        days,
        events,
        incomplete_coverage,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{self, MediaType};
    use crate::metadata::tests::{episode, season, series, stored};
    use crate::metadata::{save, save_episodes};

    #[test]
    fn month_grid_aligns_boundaries_and_leap_day() {
        let db = Database::open_in_memory();
        let feb = month(&db, "2024-02-12", "2024-02-29").unwrap();
        assert_eq!(feb.days.len(), 42);
        assert_eq!(feb.days[0].date, "2024-01-29");
        assert_eq!(feb.days[3].date, "2024-02-01");
        assert!(feb.days[31].today);
        assert_eq!(shift_month(&db, &feb.first, -1).unwrap(), "2024-01-01");
        assert_eq!(shift_month(&db, "2024-12-01", 1).unwrap(), "2025-01-01");
        assert!(feb.events.is_empty());
    }

    #[test]
    fn events_use_provider_dates_and_library_scope() {
        let db = Database::open_in_memory();
        let show = stored(&db, MediaType::Tv, 3, "A show");
        let removed = stored(&db, MediaType::Tv, 4, "Removed");
        for id in [show, removed] {
            save(&db, id, &series("Show", vec![season(1, Some(2))]), 1).unwrap();
            let mut first = episode(1, 1);
            first.air_date = Some("2026-09-30".into());
            let mut second = episode(1, 2);
            second.air_date = Some("2026-09-30".into());
            save_episodes(&db, id, 1, &[first, second], 1).unwrap();
        }
        library::remove(&db, removed).unwrap();
        let sept = month(&db, "2026-09-01", "2026-09-30").unwrap();
        assert_eq!(sept.events.len(), 2);
        assert_eq!(sept.events[0].date, "2026-09-30");
        assert_eq!(
            sept.days
                .iter()
                .find(|d| d.date == "2026-09-30")
                .unwrap()
                .count,
            2
        );
        assert!(!sept.incomplete_coverage);
        assert_eq!(coming_soon(&db, "2026-09-16").unwrap().len(), 2);
        assert!(coming_soon(&db, "2026-09-15").unwrap().is_empty());
        assert!(
            month(&db, "2026-10-01", "2026-09-30")
                .unwrap()
                .events
                .is_empty()
        );
    }

    #[test]
    fn coverage_warning_and_date_only_regression() {
        let db = Database::open_in_memory();
        let show = stored(&db, MediaType::Tv, 5, "Partial");
        save(&db, show, &series("Partial", vec![season(1, Some(2))]), 1).unwrap();
        let mut e = episode(1, 1);
        e.air_date = Some("2026-09-20".into());
        save_episodes(&db, show, 1, &[e], 1).unwrap();
        let sept = month(&db, "2026-09-01", "2026-09-20").unwrap();
        assert!(sept.incomplete_coverage);
        assert_eq!(sept.events[0].date, "2026-09-20");
        assert!(
            sept.days
                .iter()
                .find(|d| d.date == "2026-09-20")
                .unwrap()
                .today
        );
        // Only UTC instants use localtime; provider dates remain exact strings.
        assert_eq!(
            events(&db, "2026-09-20", "2026-09-21").unwrap()[0].date,
            "2026-09-20"
        );
    }
}
