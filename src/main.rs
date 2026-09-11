// Slint's recommended setting: no extra console window next to the app window
// in Windows release builds. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;
mod library;

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;

use library::MediaItem;
use rusqlite::Connection;
use slint::{Color, Model, ModelNotify, ModelRc, ModelTracker};

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let window = AppWindow::new()?;
    match open_library() {
        Ok(view) => connect(&window, Rc::new(view)),
        Err(message) => show_error(&window, message),
    }
    window.run()
}

/// Spike database location: next to the executable, so under the git-ignored
/// `target/` directory during development and removed by `cargo clean`. This is
/// spike behavior, not the production data location.
fn database_path() -> std::io::Result<PathBuf> {
    Ok(std::env::current_exe()?.with_file_name("bingee-spike.db"))
}

fn open_library() -> Result<LibraryView, String> {
    let path =
        database_path().map_err(|err| format!("Could not locate the library database: {err}"))?;
    db::open(&path)
        .and_then(LibraryView::new)
        .map_err(|err| format!("Could not load the library from {}: {err}", path.display()))
}

fn connect(window: &AppWindow, view: Rc<LibraryView>) {
    window.set_results(ModelRc::from(view.clone()));
    window.set_total_count(view.row_count() as i32);
    show_selection(window, &view);

    window.on_query_changed({
        let (view, window) = (view.clone(), window.as_weak());
        move |query| {
            let Some(window) = window.upgrade() else {
                return;
            };
            // Synchronous on the UI thread: in release builds even the
            // 1,000-row query takes a few milliseconds, well inside a frame
            // (docs/measurements/R2-sqlite-informal.md).
            match view.set_query(&query) {
                Ok(()) => {
                    window.set_error("".into());
                    show_selection(&window, &view);
                    window.invoke_reveal_row(window.get_selected_row());
                }
                Err(err) => show_error(&window, format!("Search failed: {err}")),
            }
        }
    });
    window.on_row_selected({
        let (view, window) = (view.clone(), window.as_weak());
        move |row| {
            let Some(window) = window.upgrade() else {
                return;
            };
            if let Ok(row) = usize::try_from(row) {
                view.select_row(row);
                show_selection(&window, &view);
            }
        }
    });
}

/// Shows a data failure in the library pane (and on stderr in debug builds),
/// so it never looks like an empty library.
fn show_error(window: &AppWindow, message: String) {
    eprintln!("{message}");
    window.set_error(message.into());
}

/// Library pane state: the connection that owns the data, the records matching
/// the current search, and the selected item's stable id.
///
/// Implements `slint::Model`, so the ListView asks only for the rows it is
/// about to show and `MediaRow`s are built on demand, never for all 1,000.
struct LibraryView {
    conn: Connection,
    results: RefCell<Vec<MediaItem>>,
    selected: Cell<Option<u32>>,
    notify: ModelNotify,
}

impl LibraryView {
    fn new(conn: Connection) -> rusqlite::Result<Self> {
        let results = db::search(&conn, "")?;
        Ok(Self {
            selected: Cell::new(results.first().map(|item| item.id)),
            results: RefCell::new(results),
            conn,
            notify: ModelNotify::default(),
        })
    }

    /// Runs the search. On failure the previous results and selection stay.
    fn set_query(&self, query: &str) -> rusqlite::Result<()> {
        let results = db::search(&self.conn, query)?;
        self.selected
            .set(library::reselect(&results, self.selected.get()));
        *self.results.borrow_mut() = results;
        self.notify.reset();
        Ok(())
    }

    fn select_row(&self, row: usize) {
        if let Some(item) = self.results.borrow().get(row) {
            self.selected.set(Some(item.id));
        }
    }

    /// The selected item's row in the results and its UI data.
    fn selected_row(&self) -> Option<(usize, MediaRow)> {
        let id = self.selected.get()?;
        let results = self.results.borrow();
        // ponytail: linear scan of the results (≤ 1,000); keep a position index if the list grows.
        let row = results.iter().position(|item| item.id == id)?;
        Some((row, to_row(&results[row])))
    }
}

impl Model for LibraryView {
    type Data = MediaRow;

    fn row_count(&self) -> usize {
        self.results.borrow().len()
    }

    fn row_data(&self, row: usize) -> Option<MediaRow> {
        self.results.borrow().get(row).map(to_row)
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}

/// Pushes the selection to the UI. The detail row is rebuilt from the item
/// with the selected id, so it cannot outlive a search that removed it.
fn show_selection(window: &AppWindow, view: &LibraryView) {
    match view.selected_row() {
        Some((row, detail)) => {
            window.set_selected_id(detail.id);
            window.set_selected_row(row as i32);
            window.set_detail(detail);
        }
        None => {
            window.set_selected_id(-1);
            window.set_selected_row(-1);
            window.set_detail(MediaRow::default());
        }
    }
}

/// Muted placeholder tints, picked deterministically per item.
const TINTS: [(u8, u8, u8); 6] = [
    (0x5b, 0x6e, 0xae),
    (0x8a, 0x5a, 0x9e),
    (0x3f, 0x8f, 0x86),
    (0xa8, 0x6a, 0x4a),
    (0x6d, 0x86, 0x4a),
    (0xa0, 0x4e, 0x62),
];

fn to_row(item: &MediaItem) -> MediaRow {
    let (r, g, b) = TINTS[item.id as usize % TINTS.len()];
    MediaRow {
        id: item.id as i32,
        title: item.title.as_str().into(),
        original_title: item.original_title.as_str().into(),
        year: item.year.into(),
        kind: item.kind.label().into(),
        initials: item.initials().into(),
        status: item.progress.label().into(),
        progress: item.progress.fraction(),
        overview: item.overview.as_str().into(),
        tint: Color::from_rgb_u8(r, g, b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Selected (row, id), checked against the row the list would render.
    fn selection(view: &LibraryView) -> Option<(usize, u32)> {
        let (row, detail) = view.selected_row()?;
        assert_eq!(view.row_data(row).map(|r| r.id), Some(detail.id));
        Some((row, detail.id as u32))
    }

    #[test]
    fn selection_tracks_stable_id_and_never_leaves_the_results() {
        let view = LibraryView::new(db::open(Path::new(":memory:")).unwrap()).unwrap();
        assert_eq!(selection(&view), Some((0, 1)));

        view.select_row(999);
        assert_eq!(selection(&view), Some((999, 1000)), "Lost Canyon");

        // Still a match: same id, new row.
        view.set_query("CANYON").unwrap();
        let (row, id) = selection(&view).unwrap();
        assert_eq!((id, row + 1), (1000, view.row_count()));

        // Filtered out: falls back to the first result.
        view.set_query("harbor").unwrap();
        let first = view.row_data(0).unwrap().id as u32;
        assert_eq!(selection(&view), Some((0, first)));

        view.set_query("zzzz").unwrap();
        assert_eq!((view.row_count(), selection(&view)), (0, None));

        view.set_query("").unwrap();
        assert_eq!((view.row_count(), selection(&view)), (1000, Some((0, 1))));
    }

    #[test]
    fn failed_search_keeps_previous_results() {
        let view = LibraryView::new(db::open(Path::new(":memory:")).unwrap()).unwrap();
        view.set_query("harbor").unwrap();
        let before = (view.row_count(), selection(&view));
        view.conn.execute_batch("DROP TABLE media").unwrap();
        assert!(view.set_query("dark").is_err());
        assert_eq!((view.row_count(), selection(&view)), before);
    }
}
