//! Home and Calendar presentation. All reads are local; these callbacks never
//! ask TMDB for data.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::calendar::{self, Month};
use crate::database::SharedDb;
use crate::diagnostics::Log;
use crate::home::{self, Entry};
use crate::metadata::Clock;
use crate::{AppWindow, CalendarDay, CalendarEventRow, HomeRow};

struct Dashboard {
    window: slint::Weak<AppWindow>,
    db: SharedDb,
    clock: Clock,
    log: Arc<Log>,
    home: RefCell<Vec<Entry>>,
    month: RefCell<Option<Month>>,
    selected_day: Cell<usize>,
}

pub fn start(window: &AppWindow, db: SharedDb, clock: Clock, log: Arc<Log>) {
    let page = Rc::new(Dashboard {
        window: window.as_weak(),
        db,
        clock,
        log,
        home: RefCell::default(),
        month: RefCell::default(),
        selected_day: Cell::new(0),
    });
    window.on_home_opened({
        let page = page.clone();
        move || page.load_home()
    });
    window.on_home_select({
        let page = page.clone();
        move |row| page.open_home(row)
    });
    window.on_calendar_opened({
        let page = page.clone();
        move || page.load_current_month()
    });
    window.on_calendar_shift({
        let page = page.clone();
        move |amount| page.shift(amount)
    });
    window.on_calendar_select_day({
        let page = page.clone();
        move |day| page.select_day(day)
    });
    window.on_calendar_open_event({
        let page = page.clone();
        move |row| page.open_event(row)
    });
    window.on_metadata_changed({
        let page = page.clone();
        move || {
            if let Some(window) = page.window.upgrade() {
                match window.get_page().as_str() {
                    "home" => page.load_home(),
                    "calendar" => page.load_current_month(),
                    _ => {}
                }
            }
        }
    });
}

impl Dashboard {
    fn load_home(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        match self.db.with(|db| home::load(db, self.clock.now())) {
            Ok(snapshot) => {
                window.set_home_summary(snapshot.summary.into());
                window.set_home_notice(if snapshot.incomplete_coverage {
                    "Some series have incomplete episode metadata.".into()
                } else {
                    "".into()
                });
                let rows: Vec<_> = snapshot
                    .entries
                    .iter()
                    .map(|e| HomeRow {
                        section: e.section.into(),
                        title: e.title.as_str().into(),
                        detail: e.detail.as_str().into(),
                    })
                    .collect();
                *self.home.borrow_mut() = snapshot.entries;
                window.set_home_rows(ModelRc::from(Rc::new(VecModel::from(rows))));
            }
            Err(error) => {
                self.log.error(format_args!("Home: {error}"));
                window.set_home_notice(error.message.into());
            }
        }
    }

    fn open_home(&self, row: i32) {
        let Some(entry) = usize::try_from(row)
            .ok()
            .and_then(|row| self.home.borrow().get(row).cloned())
        else {
            return;
        };
        if let Some(window) = self.window.upgrade() {
            window.invoke_open_library_media(
                entry.media_id as i32,
                entry.season.unwrap_or(-1) as i32,
                entry.episode.unwrap_or(-1) as i32,
            );
        }
    }

    fn load_current_month(&self) {
        let result = self.db.with(|db| {
            let today = calendar::local_date(db, self.clock.now())?;
            let first = self
                .month
                .borrow()
                .as_ref()
                .map_or(today.clone(), |m| m.first.clone());
            Ok((calendar::month(db, &first, &today)?, today))
        });
        self.show_month(result);
    }

    fn shift(&self, amount: i32) {
        let result = self.db.with(|db| {
            let today = calendar::local_date(db, self.clock.now())?;
            let first = self
                .month
                .borrow()
                .as_ref()
                .map_or(today.clone(), |m| m.first.clone());
            let first = calendar::shift_month(db, &first, amount)?;
            Ok((calendar::month(db, &first, &today)?, today))
        });
        self.show_month(result);
    }

    fn show_month(&self, result: Result<(Month, String), crate::error::AppError>) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        match result {
            Ok((month, today)) => {
                let previous_date = self.month.borrow().as_ref().and_then(|old| {
                    old.days
                        .get(self.selected_day.get())
                        .map(|d| d.date.clone())
                });
                let selected = month
                    .days
                    .iter()
                    .position(|d| {
                        previous_date.as_ref().is_some_and(|date| &d.date == date) && d.in_month
                    })
                    .or_else(|| {
                        month
                            .days
                            .iter()
                            .position(|d| d.date == today && d.in_month)
                    })
                    .or_else(|| month.days.iter().position(|d| d.in_month))
                    .unwrap_or(0);
                self.selected_day.set(selected);
                window.set_calendar_month(month.label.as_str().into());
                window.set_calendar_notice(if month.incomplete_coverage {
                    "Some series have incomplete episode metadata. Calendar may miss upcoming episodes.".into()
                } else { "".into() });
                let days: Vec<_> = month
                    .days
                    .iter()
                    .map(|d| CalendarDay {
                        number: d.date[8..].trim_start_matches('0').into(),
                        date: d.date.as_str().into(),
                        count: d.count as i32,
                        in_month: d.in_month,
                        today: d.today,
                    })
                    .collect();
                window.set_calendar_days(ModelRc::from(Rc::new(VecModel::from(days))));
                *self.month.borrow_mut() = Some(month);
                self.render_day();
            }
            Err(error) => {
                self.log.error(format_args!("Calendar: {error}"));
                window.set_calendar_notice(error.message.into());
            }
        }
    }

    fn select_day(&self, day: i32) {
        let Ok(day) = usize::try_from(day) else {
            return;
        };
        if day >= 42 {
            return;
        }
        self.selected_day.set(day);
        self.render_day();
    }

    fn render_day(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let month = self.month.borrow();
        let Some(month) = month.as_ref() else { return };
        let Some(day) = month.days.get(self.selected_day.get()) else {
            return;
        };
        window.set_calendar_selected_day(self.selected_day.get() as i32);
        window.set_calendar_date(day.date.as_str().into());
        let rows: Vec<_> = month
            .events
            .iter()
            .filter(|e| e.date == day.date)
            .map(|e| CalendarEventRow {
                title: e.series.as_str().into(),
                detail: format!(
                    "S{} E{}{}",
                    e.season,
                    e.episode,
                    e.name.as_ref().map_or(String::new(), |n| format!(" · {n}"))
                )
                .into(),
            })
            .collect();
        window.set_calendar_events(ModelRc::from(Rc::new(VecModel::from(rows))));
    }

    fn open_event(&self, row: i32) {
        let Some(month) = self.month.borrow().as_ref().cloned() else {
            return;
        };
        let Some(day) = month.days.get(self.selected_day.get()) else {
            return;
        };
        let Some(event) = usize::try_from(row)
            .ok()
            .and_then(|row| month.events.iter().filter(|e| e.date == day.date).nth(row))
        else {
            return;
        };
        if let Some(window) = self.window.upgrade() {
            window.invoke_open_library_media(
                event.media_id as i32,
                event.season as i32,
                event.episode as i32,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use crate::library::MediaType;
    use crate::metadata::tests::{episode, season, series, stored};
    use crate::metadata::{save, save_episodes};
    use crate::tests::Headless;
    use slint::Model;

    #[test]
    fn home_and_calendar_open_offline_and_reload_from_sqlite() {
        let mut ui = Headless::new(1280, 800);
        let app = ui.app.clone_strong();
        let db = SharedDb::default();
        db.set(Database::open_in_memory());
        let (clock, _) = Clock::fake(1_800_000_000);
        start(&app, db.clone(), clock, Arc::new(Log::stderr_only()));
        app.set_page("home".into());
        ui.render();
        assert_eq!(app.get_home_summary(), "0 movies · 0 series in Library");
        assert_eq!(app.get_home_rows().row_count(), 0);
        let today = db
            .with(|db| calendar::local_date(db, 1_800_000_000))
            .unwrap();
        let id = db
            .with(|db| {
                let id = stored(db, MediaType::Tv, 9, "Offline series");
                save(
                    db,
                    id,
                    &series("Offline series", vec![season(1, Some(1))]),
                    1,
                )?;
                let mut e = episode(1, 1);
                e.air_date = Some(today.clone());
                save_episodes(db, id, 1, &[e], 1)?;
                Ok(id)
            })
            .unwrap();
        app.set_page("calendar".into());
        ui.render();
        assert_eq!(app.get_calendar_days().row_count(), 42);
        assert_eq!(app.get_calendar_date(), today);
        assert_eq!(app.get_calendar_events().row_count(), 1);
        let opened = Rc::new(Cell::new(None));
        app.on_open_library_media({
            let opened = opened.clone();
            move |id, season, episode| opened.set(Some((id, season, episode)))
        });
        app.invoke_calendar_open_event(0);
        assert_eq!(opened.get(), Some((id as i32, 1, 1)));
        app.invoke_calendar_shift(1);
        ui.render();
        assert_eq!(app.get_calendar_events().row_count(), 0);
        app.invoke_calendar_shift(-1);
        ui.render();
        assert_eq!(app.get_calendar_events().row_count(), 1);
        app.set_page("home".into());
        ui.render();
        assert!(
            app.get_home_rows()
                .iter()
                .any(|r| r.section == "Coming Soon")
        );
        let upcoming = app
            .get_home_rows()
            .iter()
            .position(|r| r.section == "Coming Soon")
            .unwrap();
        app.invoke_home_select(upcoming as i32);
        assert_eq!(opened.get(), Some((id as i32, 1, 1)));
        let movie = db
            .with(|db| {
                let movie = stored(db, MediaType::Movie, 11, "Watched movie");
                crate::tracking::watch_movie(db, movie, 1_800_000_000, false)?;
                Ok(movie)
            })
            .unwrap();
        app.invoke_home_opened();
        let recent = app
            .get_home_rows()
            .iter()
            .position(|r| r.title == "Watched movie")
            .unwrap();
        app.invoke_home_select(recent as i32);
        assert_eq!(opened.get(), Some((movie as i32, -1, -1)));
    }
}
