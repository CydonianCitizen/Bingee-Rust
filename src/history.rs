//! The History page: recent watches, newest first, read from SQLite when the
//! page opens (ADR-0017). Removing an entry deletes that event only; watched
//! state is not changed. Local only.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

use crate::database::SharedDb;
use crate::diagnostics::Log;
use crate::tracking::{self, HistoryEntry};
use crate::{AppWindow, HistoryRow};

/// How many recent watches the page lists.
// ponytail: one fixed page, no paging; add "Show more" when people need
// history older than this in the app (statistics will query SQLite directly).
pub const RECENT: usize = 200;

struct History {
    window: slint::Weak<AppWindow>,
    db: SharedDb,
    log: Arc<Log>,
    entries: RefCell<Vec<HistoryEntry>>,
    /// The selected event, kept across reloads by id.
    selected: Cell<Option<i64>>,
    notice: RefCell<String>,
}

pub fn start(window: &AppWindow, db: SharedDb, log: Arc<Log>) {
    let history = Rc::new(History {
        window: window.as_weak(),
        db,
        log,
        entries: RefCell::default(),
        selected: Cell::new(None),
        notice: RefCell::default(),
    });
    window.on_history_opened({
        let history = history.clone();
        move || history.load()
    });
    window.on_history_row_selected({
        let history = history.clone();
        move |row| {
            if let Some(entry) = history.entry(row) {
                history.selected.set(Some(entry.event_id));
                history.render();
            }
        }
    });
    window.on_history_delete(move |row| history.delete(row));
}

impl History {
    fn entry(&self, row: i32) -> Option<HistoryEntry> {
        let row = usize::try_from(row).ok()?;
        self.entries.borrow().get(row).cloned()
    }

    fn load(&self) {
        match self.db.with(|db| tracking::recent_history(db, RECENT)) {
            Ok(entries) => {
                self.notice.borrow_mut().clear();
                let selected = self.selected.get();
                if !entries.iter().any(|e| Some(e.event_id) == selected) {
                    self.selected.set(entries.first().map(|e| e.event_id));
                }
                *self.entries.borrow_mut() = entries;
            }
            Err(error) => {
                self.log.error(format_args!("History: not read: {error}"));
                *self.notice.borrow_mut() = error.message;
            }
        }
        self.render();
    }

    fn delete(&self, row: i32) {
        let Some(entry) = self.entry(row) else {
            return;
        };
        match self
            .db
            .with(|db| tracking::delete_event(db, entry.event_id))
        {
            Ok(_) => {
                // The selection moves to the entry that takes its place.
                let entries = self.entries.borrow();
                let next = entries
                    .get(row as usize + 1)
                    .or_else(|| entries.get((row as usize).wrapping_sub(1)));
                self.selected.set(next.map(|e| e.event_id));
            }
            Err(error) => {
                self.log
                    .error(format_args!("History: entry not removed: {error}"));
                *self.notice.borrow_mut() = error.message;
                self.render();
                return;
            }
        }
        self.load();
    }

    fn render(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let entries = self.entries.borrow();
        let rows: Vec<HistoryRow> = entries.iter().map(row).collect();
        let selected = entries
            .iter()
            .position(|e| Some(e.event_id) == self.selected.get());
        window.set_history_rows(ModelRc::from(Rc::new(VecModel::from(rows))));
        window.set_history_selected_row(selected.map_or(-1, |row| row as i32));
        window.set_history_notice(self.notice.borrow().as_str().into());
    }
}

fn row(entry: &HistoryEntry) -> HistoryRow {
    HistoryRow {
        title: entry.title.as_str().into(),
        detail: match (entry.episode, &entry.episode_name) {
            (None, _) => SharedString::from("Movie"),
            (Some((season, number)), name) => {
                let episode = match season {
                    0 => format!("Specials E{number}"),
                    season => format!("S{season} E{number}"),
                };
                match name {
                    Some(name) => format!("{episode} · {name}").into(),
                    None => episode.into(),
                }
            }
        },
        when: entry.local_time.as_str().into(),
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
    use slint::platform::{Key, WindowEvent};

    #[test]
    fn the_history_page_lists_newest_first_and_removes_only_the_entry() {
        let mut ui = Headless::new(1280, 800);
        let app = ui.app.clone_strong();
        let db = SharedDb::default();
        db.set(Database::open_in_memory());
        let (movie, show) = db
            .with(|db| {
                let movie = stored(db, MediaType::Movie, 603, "The Matrix");
                let show = stored(db, MediaType::Tv, 1399, "Game of Thrones");
                save(
                    db,
                    show,
                    &series("Game of Thrones", vec![season(1, Some(2))]),
                    1,
                )?;
                save_episodes(db, show, 1, &[episode(1, 1), episode(1, 2)], 1)?;
                tracking::watch_movie(db, movie, 1_800_000_000, false)?;
                tracking::watch_episode(db, show, 1, 2, 1_800_000_100, false)?;
                Ok((movie, show))
            })
            .unwrap();
        start(&app, db.clone(), Arc::new(Log::stderr_only()));

        // Opening the page reads SQLite and puts focus in the list.
        app.set_page("history".into());
        ui.render();
        let rows = app.get_history_rows();
        let titles: Vec<(String, String)> = rows
            .iter()
            .map(|r| (r.title.into(), r.detail.into()))
            .collect();
        assert_eq!(
            titles,
            [
                ("Game of Thrones".into(), "S1 E2 · Episode 2".into()),
                ("The Matrix".into(), "Movie".into())
            ]
        );
        assert_eq!(rows.row_data(0).unwrap().when.len(), 16);
        assert_eq!(app.get_history_selected_row(), 0);

        // Keyboard: Down, then Delete removes the movie's entry only.
        let press = |key: Key| {
            let text: SharedString = key.into();
            app.window()
                .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
            app.window()
                .dispatch_event(WindowEvent::KeyReleased { text });
        };
        press(Key::DownArrow);
        assert_eq!(app.get_history_selected_row(), 1);
        press(Key::Delete);
        ui.render();
        assert_eq!(app.get_history_rows().row_count(), 1);
        assert_eq!(app.get_history_selected_row(), 0);
        let state = db.with(|db| tracking::title_state(db, movie)).unwrap();
        assert!(state.watched_at.is_some(), "still marked watched");

        // A failed delete says so and keeps the entry.
        db.with(|db| {
            db.conn()
                .execute_batch(
                    "CREATE TEMP TRIGGER fail BEFORE DELETE ON watch_events
                     BEGIN SELECT RAISE(ABORT, 'simulated'); END;",
                )
                .unwrap();
            Ok(())
        })
        .unwrap();
        app.invoke_history_delete(0);
        assert!(!app.get_history_notice().is_empty());
        assert_eq!(app.get_history_rows().row_count(), 1);
        let _ = show;
        ui.render();
    }
}
