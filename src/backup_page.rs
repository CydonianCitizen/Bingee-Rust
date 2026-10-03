//! Settings backup controls. Dialogs are an adapter; tests supply paths and
//! never open interactive native dialogs.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use slint::ComponentHandle;

use crate::AppWindow;
use crate::backup::{self, BackupV1};
use crate::database::SharedDb;
use crate::diagnostics::Log;
use crate::metadata::Clock;

pub trait FileChooser {
    fn save_backup(&self) -> Option<PathBuf>;
    fn open_backup(&self) -> Option<PathBuf>;
}

pub struct NativeDialogs;

impl FileChooser for NativeDialogs {
    fn save_backup(&self) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .add_filter("Bingee backup", &["json"])
            .set_file_name("bingee-backup.json")
            .save_file()
    }

    fn open_backup(&self) -> Option<PathBuf> {
        rfd::FileDialog::new()
            .add_filter("Bingee backup", &["json"])
            .pick_file()
    }
}

struct BackupPage<D: FileChooser> {
    window: slint::Weak<AppWindow>,
    db: SharedDb,
    clock: Clock,
    log: Arc<Log>,
    dialogs: D,
    pending: RefCell<Option<(PathBuf, BackupV1)>>,
}

pub fn start<D: FileChooser + 'static>(
    window: &AppWindow,
    db: SharedDb,
    clock: Clock,
    log: Arc<Log>,
    dialogs: D,
) {
    let page = Rc::new(BackupPage {
        window: window.as_weak(),
        db,
        clock,
        log,
        dialogs,
        pending: RefCell::default(),
    });
    window.on_backup_export({
        let page = page.clone();
        move || page.export()
    });
    window.on_backup_select({
        let page = page.clone();
        move || page.select()
    });
    window.on_backup_confirm(move || page.confirm());
}

impl<D: FileChooser> BackupPage<D> {
    fn export(&self) {
        let Some(path) = self.dialogs.save_backup() else {
            return;
        };
        let result = self
            .db
            .with(|db| backup::export_to_path(db, &path, self.clock.now()));
        let Some(window) = self.window.upgrade() else {
            return;
        };
        match result {
            Ok(()) => {
                window.set_backup_status(format!("Backup exported to {}.", path.display()).into())
            }
            Err(error) => {
                self.log.error(format_args!("Backup export: {error}"));
                window.set_backup_status(error.message.into());
            }
        }
    }

    fn select(&self) {
        let Some(path) = self.dialogs.open_backup() else {
            return;
        };
        let Some(window) = self.window.upgrade() else {
            return;
        };
        *self.pending.borrow_mut() = None;
        window.set_backup_ready(false);
        window.set_backup_file(path.display().to_string().into());
        match backup::read_from_path(&path) {
            Ok(value) => {
                window.set_backup_preview(format!(
                "Backup checked: {} titles, {} watch events. Restore replaces your current saved data after creating a safety backup.",
                    value.data.media.len(), value.data.watch_events.len(),
                ).into());
                window.set_backup_status("".into());
                window.set_backup_ready(true);
                *self.pending.borrow_mut() = Some((path, value));
            }
            Err(error) => {
                self.log.error(format_args!("Backup validation: {error}"));
                window.set_backup_preview("".into());
                window.set_backup_status(error.message.into());
            }
        }
    }

    fn confirm(&self) {
        let Some((_, value)) = self.pending.borrow().as_ref().cloned() else {
            return;
        };
        let result = self.db.with(|db| {
            let safety = backup::pre_restore_safety(db, self.clock.now())?;
            backup::restore(db, &value)?;
            Ok(safety)
        });
        let Some(window) = self.window.upgrade() else {
            return;
        };
        match result {
            Ok(safety) => {
                *self.pending.borrow_mut() = None;
                window.set_backup_ready(false);
                window.set_backup_preview("".into());
                let saved = safety.map_or(String::new(), |path| {
                    format!(" Previous data: {}.", path.display())
                });
                window.set_backup_status(
                    format!("Restore complete. Restart Bingee Desktop to reload all pages.{saved}")
                        .into(),
                );
                window.invoke_library_changed();
            }
            Err(error) => {
                self.log.error(format_args!("Backup restore: {error}"));
                window.set_backup_status(error.message.into());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use crate::library::MediaType;
    use crate::metadata::tests::stored;
    use crate::paths::TestDir;
    use crate::tests::Headless;

    #[derive(Clone)]
    struct Paths {
        export: PathBuf,
        restore: PathBuf,
    }

    impl FileChooser for Paths {
        fn save_backup(&self) -> Option<PathBuf> {
            Some(self.export.clone())
        }
        fn open_backup(&self) -> Option<PathBuf> {
            Some(self.restore.clone())
        }
    }

    #[test]
    fn settings_requires_validation_and_second_restore_action() {
        let mut ui = Headless::new(1280, 800);
        let app = ui.app.clone_strong();
        let dir = TestDir::new("backup-page");
        let path = dir.0.join("backup.json");
        let db = SharedDb::default();
        db.set(Database::open_in_memory());
        db.with(|db| {
            stored(db, MediaType::Movie, 603, "Keep");
            Ok(())
        })
        .unwrap();
        let (clock, _) = Clock::fake(1_800_000_000);
        start(
            &app,
            db.clone(),
            clock,
            Arc::new(Log::stderr_only()),
            Paths {
                export: path.clone(),
                restore: path,
            },
        );
        app.set_page("settings".into());
        ui.render();
        app.invoke_backup_export();
        assert!(app.get_backup_status().contains("exported"));
        db.with(|db| {
            stored(db, MediaType::Movie, 604, "Remove on restore");
            Ok(())
        })
        .unwrap();
        app.invoke_backup_select();
        assert!(app.get_backup_ready());
        assert!(app.get_backup_preview().contains("1 titles"));
        assert_eq!(db.with(crate::library::count).unwrap(), 2);
        app.invoke_backup_confirm();
        ui.render();
        assert_eq!(db.with(crate::library::count).unwrap(), 1);
        assert!(!app.get_backup_ready());
    }
}
