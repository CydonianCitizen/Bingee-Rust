//! The library list and detail pane: a `slint::Model` over the current
//! search results, shared by the production library and the benchmark
//! fixture.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use slint::{Color, ComponentHandle, Image, Model, ModelNotify, ModelRc, ModelTracker};

use crate::database::Database;
use crate::diagnostics::Log;
use crate::error::AppError;
use crate::library::{self, LibraryItem, MediaType};
use crate::{AppWindow, MediaRow};

/// A searchable set of titles and how each one is shown.
pub trait Library {
    type Item: 'static;
    fn search(&self, query: &str) -> Result<Vec<Self::Item>, AppError>;
    fn id(item: &Self::Item) -> i64;
    fn row(&self, item: &Self::Item) -> MediaRow;
}

/// The production library: rows from the user's database. No posters yet.
impl Library for Database {
    type Item = LibraryItem;

    fn search(&self, query: &str) -> Result<Vec<LibraryItem>, AppError> {
        library::search(self, query)
    }

    fn id(item: &LibraryItem) -> i64 {
        item.id
    }

    fn row(&self, item: &LibraryItem) -> MediaRow {
        media_row(
            item.id,
            item.media_type,
            &item.title,
            item.original_title.as_deref(),
            item.year(),
            item.overview.as_deref(),
        )
    }
}

/// A row without progress or poster (the placeholder shows): library titles
/// and remote search results.
pub fn media_row(
    id: i64,
    kind: MediaType,
    title: &str,
    original_title: Option<&str>,
    year: Option<&str>,
    overview: Option<&str>,
) -> MediaRow {
    let kind = kind.label();
    MediaRow {
        id: ui_id(id),
        title: title.into(),
        original_title: original_title.unwrap_or_default().into(),
        meta: match year {
            Some(year) => format!("{kind} · {year}").into(),
            None => kind.into(),
        },
        initials: library::initials(title).into(),
        status: Default::default(),
        progress: 0.0,
        overview: overview.unwrap_or_default().into(),
        tint: tint(id),
        poster: Image::default(),
    }
}

/// Library pane state: the library that owns the data, the records matching
/// the current search, and the selected item's stable id.
///
/// Implements `slint::Model`, so the ListView asks only for the rows it is
/// about to show and `MediaRow`s are built on demand.
pub struct LibraryView<L: Library> {
    pub library: L,
    pub results: RefCell<Vec<L::Item>>,
    pub selected: Cell<Option<i64>>,
    notify: ModelNotify,
}

impl<L: Library> LibraryView<L> {
    pub fn new(library: L) -> Result<Self, AppError> {
        let results = library.search("")?;
        Ok(Self {
            selected: Cell::new(results.first().map(L::id)),
            results: RefCell::new(results),
            library,
            notify: ModelNotify::default(),
        })
    }

    /// Runs the search. On failure the previous results and selection stay.
    pub fn set_query(&self, query: &str) -> Result<(), AppError> {
        let results = self.library.search(query)?;
        self.selected
            .set(reselect(&results, L::id, self.selected.get()));
        *self.results.borrow_mut() = results;
        self.notify.reset();
        Ok(())
    }

    pub fn select_row(&self, row: usize) {
        if let Some(item) = self.results.borrow().get(row) {
            self.selected.set(Some(L::id(item)));
        }
    }

    /// The selected item's row in the results and its UI data.
    pub fn selected_row(&self) -> Option<(usize, MediaRow)> {
        let id = self.selected.get()?;
        let results = self.results.borrow();
        // ponytail: linear scan of the results; keep a position index if
        // libraries grow far beyond thousands of titles.
        let row = results.iter().position(|item| L::id(item) == id)?;
        Some((row, self.library.row(&results[row])))
    }
}

impl<L: Library> Model for LibraryView<L> {
    type Data = MediaRow;

    fn row_count(&self) -> usize {
        self.results.borrow().len()
    }

    fn row_data(&self, row: usize) -> Option<MediaRow> {
        self.results
            .borrow()
            .get(row)
            .map(|item| self.library.row(item))
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }
}

/// Selection policy after a search: keep the selected key if it is still in
/// `results`, otherwise select the first result, or nothing if there are no
/// results. The selection therefore never points outside the visible results.
pub fn reselect<T, K: PartialEq>(
    results: &[T],
    key: impl Fn(&T) -> K,
    selected: Option<K>,
) -> Option<K> {
    match selected {
        Some(selected) if results.iter().any(|item| key(item) == selected) => Some(selected),
        _ => results.first().map(key),
    }
}

/// Binds the library pane to `view`. Search failures keep the previous
/// results, show a user message and go to the log.
pub fn connect<L: Library + 'static>(window: &AppWindow, view: Rc<LibraryView<L>>, log: Arc<Log>) {
    window.set_results(ModelRc::from(view.clone()));
    window.set_total_count(view.row_count() as i32);
    window.set_error("".into());
    show_selection(window, &view);

    window.on_query_changed({
        let (view, window) = (view.clone(), window.as_weak());
        move |query| {
            let Some(window) = window.upgrade() else {
                return;
            };
            // Synchronous on the UI thread: a few milliseconds even for the
            // 1,000-row fixture (docs/measurements/R4-rust-slint/STATUS.md).
            match view.set_query(&query) {
                Ok(()) => {
                    window.set_error("".into());
                    show_selection(&window, &view);
                    window.invoke_reveal_row(window.get_selected_row());
                }
                Err(err) => {
                    log.error(&err);
                    window.set_error(err.message.into());
                }
            }
        }
    });
    window.on_row_selected({
        let window = window.as_weak();
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

/// Pushes the selection to the UI. The detail row is rebuilt from the item
/// with the selected id, so it cannot outlive a search that removed it.
pub fn show_selection<L: Library>(window: &AppWindow, view: &LibraryView<L>) {
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

/// A local id for the UI, where it is informational only (the highlight
/// follows the selected row). Ids beyond `i32` do not occur in practice.
pub fn ui_id(id: i64) -> i32 {
    i32::try_from(id).unwrap_or(-1)
}

/// Muted placeholder tint, picked deterministically per id. The placeholder
/// shows whenever there is no poster.
pub fn tint(id: i64) -> Color {
    const TINTS: [(u8, u8, u8); 6] = [
        (0x5b, 0x6e, 0xae),
        (0x8a, 0x5a, 0x9e),
        (0x3f, 0x8f, 0x86),
        (0xa8, 0x6a, 0x4a),
        (0x6d, 0x86, 0x4a),
        (0xa0, 0x4e, 0x62),
    ];
    let (r, g, b) = TINTS[id.rem_euclid(TINTS.len() as i64) as usize];
    Color::from_rgb_u8(r, g, b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::tests::add;

    #[test]
    fn empty_production_library_has_no_rows_and_no_selection() {
        let view = LibraryView::new(Database::open_in_memory()).unwrap();
        assert_eq!(view.row_count(), 0);
        assert!(view.selected_row().is_none());
        view.set_query("anything").unwrap();
        assert_eq!(view.row_count(), 0);
    }

    #[test]
    fn production_rows_show_database_values_without_progress_or_poster() {
        let db = Database::open_in_memory();
        let id = add(&db, "movie", "Arrival", Some("Premier contact"), true);
        let view = LibraryView::new(db).unwrap();
        let (row, detail) = view.selected_row().unwrap();
        assert_eq!(row, 0);
        assert_eq!(detail.id as i64, id);
        assert_eq!(detail.title.as_str(), "Arrival");
        assert_eq!(detail.original_title.as_str(), "Premier contact");
        assert_eq!(detail.meta.as_str(), "Movie · 2017");
        assert_eq!(
            (detail.status.as_str(), detail.initials.as_str()),
            ("", "A")
        );
        assert_eq!(detail.poster.size().width, 0, "placeholder");
    }

    #[test]
    fn failed_search_keeps_previous_results() {
        let db = Database::open_in_memory();
        add(&db, "tv", "Dark", None, true);
        add(&db, "tv", "Andor", None, true);
        let view = LibraryView::new(db).unwrap();
        view.set_query("dark").unwrap();
        let before = (view.row_count(), view.selected.get());
        view.library
            .conn()
            .execute_batch("DROP TABLE library_entries")
            .unwrap();
        let error = view.set_query("andor").unwrap_err();
        assert_eq!(error.kind, crate::error::ErrorKind::Database);
        assert_eq!((view.row_count(), view.selected.get()), before);
    }
}
