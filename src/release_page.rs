//! Local in-app Updates page. Opening an item marks it read, then navigates
//! through the existing Library detail path.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use slint::{ComponentHandle, ModelRc, VecModel};

use crate::database::SharedDb;
use crate::diagnostics::Log;
use crate::metadata::Clock;
use crate::release::{self, ReleaseEvent};
use crate::{AppWindow, UpdateRow};

struct Page {
    window: slint::Weak<AppWindow>,
    db: SharedDb,
    clock: Clock,
    log: Arc<Log>,
    events: RefCell<Vec<ReleaseEvent>>,
}

pub fn start(window: &AppWindow, db: SharedDb, clock: Clock, log: Arc<Log>) {
    let page = Rc::new(Page {
        window: window.as_weak(),
        db,
        clock,
        log,
        events: RefCell::default(),
    });
    window.on_updates_opened({
        let p = page.clone();
        move || p.load()
    });
    window.on_release_events_changed({
        let p = page.clone();
        move || p.load()
    });
    window.on_updates_select({
        let p = page.clone();
        move |row| p.select(row)
    });
    window.on_updates_open({
        let p = page.clone();
        move |row| p.open(row)
    });
    window.on_updates_mark_all_read(move || page.mark_all());
}

impl Page {
    fn load(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        match self
            .db
            .with(|db| Ok((release::recent(db, 100)?, release::unread_count(db)?)))
        {
            Ok((events, unread)) => {
                let rows: Vec<_> = events
                    .iter()
                    .map(|e| UpdateRow {
                        title: e.series.as_str().into(),
                        detail: format!(
                            "New episode S{} E{}{}{}",
                            e.season,
                            e.episode,
                            e.name.as_ref().map_or(String::new(), |n| format!(" · {n}")),
                            e.air_date
                                .as_ref()
                                .map_or(String::new(), |d| format!(" · Airs {d}"))
                        )
                        .into(),
                        when: e.local_time.as_str().into(),
                        unread: e.read_at.is_none(),
                    })
                    .collect();
                *self.events.borrow_mut() = events;
                window.set_updates_rows(ModelRc::from(Rc::new(VecModel::from(rows))));
                window.set_updates_unread(unread as i32);
                window.set_updates_selected_row(if self.events.borrow().is_empty() {
                    -1
                } else {
                    0
                });
                window.set_updates_notice("".into());
            }
            Err(error) => {
                self.log.error(format_args!("Updates: {error}"));
                window.set_updates_notice(error.message.into());
            }
        }
    }

    fn select(&self, row: i32) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        if usize::try_from(row)
            .ok()
            .is_some_and(|i| i < self.events.borrow().len())
        {
            window.set_updates_selected_row(row);
        }
    }

    fn open(&self, row: i32) {
        let Some(event) = usize::try_from(row)
            .ok()
            .and_then(|i| self.events.borrow().get(i).cloned())
        else {
            return;
        };
        let result = self
            .db
            .with(|db| release::mark_read(db, event.id, self.clock.now()));
        if let Err(error) = result {
            self.log.error(format_args!("Updates: {error}"));
            if let Some(window) = self.window.upgrade() {
                window.set_updates_notice(error.message.into());
            }
            return;
        }
        self.load();
        if let Some(window) = self.window.upgrade() {
            window.invoke_open_library_media(
                event.media_id as i32,
                event.season as i32,
                event.episode as i32,
            );
        }
    }

    fn mark_all(&self) {
        match self
            .db
            .with(|db| release::mark_all_read(db, self.clock.now()).map(drop))
        {
            Ok(()) => self.load(),
            Err(error) => {
                self.log.error(format_args!("Updates: {error}"));
                if let Some(window) = self.window.upgrade() {
                    window.set_updates_notice(error.message.into());
                }
            }
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
    fn updates_page_loads_marks_read_and_stays_local() {
        let mut ui = Headless::new(1280, 800);
        let db = SharedDb::default();
        db.set(Database::open_in_memory());
        let show = db
            .with(|db| {
                let id = stored(db, MediaType::Tv, 9, "Show");
                save(db, id, &series("Show", vec![season(1, Some(2))]), 1)?;
                save_episodes(db, id, 1, &[episode(1, 1)], 1)?;
                save_episodes(db, id, 1, &[episode(1, 1), episode(1, 2)], 2)?;
                Ok(id)
            })
            .unwrap();
        let (clock, _) = Clock::fake(10);
        start(&ui.app, db.clone(), clock, Arc::new(Log::stderr_only()));
        ui.app.set_page("updates".into());
        ui.render();
        assert_eq!(ui.app.get_updates_rows().row_count(), 1);
        assert_eq!(ui.app.get_updates_unread(), 1);
        ui.app.invoke_updates_open(0);
        assert_eq!(db.with(release::unread_count).unwrap(), 0);
        db.with(|db| {
            save_episodes(
                db,
                show,
                1,
                &[episode(1, 1), episode(1, 2), episode(1, 3)],
                3,
            )
        })
        .unwrap();
        ui.app.invoke_release_events_changed();
        assert_eq!(ui.app.get_updates_unread(), 1);
        ui.app.invoke_updates_mark_all_read();
        assert_eq!(ui.app.get_updates_unread(), 0);
    }
}
