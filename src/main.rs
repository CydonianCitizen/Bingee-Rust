// Slint's recommended setting: no extra console window next to the app window
// in Windows release builds. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod library;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use library::MediaItem;
use slint::{Color, Model, ModelNotify, ModelRc, ModelTracker};

slint::include_modules!();

fn main() -> Result<(), slint::PlatformError> {
    let window = AppWindow::new()?;
    let view = Rc::new(LibraryView::new(library::generate_library()));

    window.set_results(ModelRc::from(view.clone()));
    window.set_total_count(view.items.len() as i32);
    show_selection(&window, &view);

    window.on_query_changed({
        let (view, window) = (view.clone(), window.as_weak());
        move |query| {
            let Some(window) = window.upgrade() else {
                return;
            };
            view.set_query(&query);
            show_selection(&window, &view);
            window.invoke_reveal_row(window.get_selected_row());
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

    window.run()
}

/// Library pane state: the full dataset, the current search result as
/// indices into it, and the selected item's stable id.
///
/// Implements `slint::Model`, so the ListView asks only for the rows it is
/// about to show and `MediaRow`s are built on demand, never for all 1,000.
struct LibraryView {
    items: Vec<MediaItem>,
    results: RefCell<Vec<usize>>,
    selected: Cell<Option<u32>>,
    notify: ModelNotify,
}

impl LibraryView {
    fn new(items: Vec<MediaItem>) -> Self {
        Self {
            results: RefCell::new((0..items.len()).collect()),
            selected: Cell::new(items.first().map(|item| item.id)),
            items,
            notify: ModelNotify::default(),
        }
    }

    fn set_query(&self, query: &str) {
        let results = library::search(&self.items, query);
        self.selected.set(library::reselect(
            &self.items,
            &results,
            self.selected.get(),
        ));
        *self.results.borrow_mut() = results;
        self.notify.reset();
    }

    fn select_row(&self, row: usize) {
        if let Some(&index) = self.results.borrow().get(row) {
            self.selected.set(Some(self.items[index].id));
        }
    }

    fn selected_item(&self) -> Option<(usize, &MediaItem)> {
        let id = self.selected.get()?;
        let results = self.results.borrow();
        // ponytail: linear scan of the results (≤ 1,000); keep a position index if the list grows.
        let row = results
            .iter()
            .position(|&index| self.items[index].id == id)?;
        Some((row, &self.items[results[row]]))
    }
}

impl Model for LibraryView {
    type Data = MediaRow;

    fn row_count(&self) -> usize {
        self.results.borrow().len()
    }

    fn row_data(&self, row: usize) -> Option<MediaRow> {
        let index = *self.results.borrow().get(row)?;
        Some(to_row(&self.items[index]))
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}

/// Pushes the selection to the UI. The detail row is rebuilt from the item
/// with the selected id, so it cannot outlive a search that removed it.
fn show_selection(window: &AppWindow, view: &LibraryView) {
    match view.selected_item() {
        Some((row, item)) => {
            window.set_selected_id(item.id as i32);
            window.set_selected_row(row as i32);
            window.set_detail(to_row(item));
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

    /// Selected (row, id), checked against the row the list would render.
    fn selection(view: &LibraryView) -> Option<(usize, u32)> {
        let (row, item) = view.selected_item()?;
        assert_eq!(view.row_data(row).map(|r| r.id), Some(item.id as i32));
        Some((row, item.id))
    }

    #[test]
    fn selection_tracks_stable_id_and_never_leaves_the_results() {
        let view = LibraryView::new(library::generate_library());
        assert_eq!(selection(&view), Some((0, 1)));

        view.select_row(999);
        assert_eq!(selection(&view), Some((999, 1000)), "Lost Canyon");

        // Still a match: same id, new row.
        view.set_query("CANYON");
        let (row, id) = selection(&view).unwrap();
        assert_eq!((id, row + 1), (1000, view.row_count()));

        // Filtered out: falls back to the first result.
        view.set_query("harbor");
        let first = view.row_data(0).unwrap().id as u32;
        assert_eq!(selection(&view), Some((0, first)));

        view.set_query("zzzz");
        assert_eq!((view.row_count(), selection(&view)), (0, None));

        view.set_query("");
        assert_eq!((view.row_count(), selection(&view)), (1000, Some((0, 1))));
    }
}
