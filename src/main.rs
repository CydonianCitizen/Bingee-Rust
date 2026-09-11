// Slint's recommended setting: no extra console window next to the app window
// in Windows release builds. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod db;
mod library;
mod poster;

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use library::MediaItem;
use poster::PosterCache;
use rusqlite::Connection;
use slint::{Color, Image, Model, ModelNotify, ModelRc, ModelTracker};

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

/// Spike poster location: the shared benchmark assets in the checkout this
/// binary was built from. Not a packaging decision (R5).
fn poster_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("benchmark/assets/posters")
}

/// Loads a poster with Slint's own loader, which reads and decodes the file
/// immediately (JPEG → RGB8). A failure is logged here once; the cache never
/// retries it, and the row shows the placeholder instead.
fn slint_poster_loader(dir: PathBuf) -> poster::Loader<Image> {
    Box::new(move |number| {
        let path = dir.join(poster::file_name(number));
        match Image::load_from_path(&path) {
            Ok(image) => {
                let size = image.size();
                // Budget estimate at 4 bytes/pixel; JPEGs actually decode to
                // 3 (RGB8), so this over-counts.
                Some((image, size.width as usize * size.height as usize * 4))
            }
            Err(_) => {
                eprintln!(
                    "Poster could not be loaded, showing placeholder: {}",
                    path.display()
                );
                None
            }
        }
    })
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
    // Debug aid for the R3 memory observation: print the cache counters on
    // request. No polling, and nothing leaves the process.
    window.on_debug_dump(move || eprintln!("{}", view.posters.borrow()));
}

/// Shows a data failure in the library pane (and on stderr in debug builds),
/// so it never looks like an empty library.
fn show_error(window: &AppWindow, message: String) {
    eprintln!("{message}");
    window.set_error(message.into());
}

/// Library pane state: the connection that owns the data, the records matching
/// the current search, the selected item's stable id, and the poster cache
/// shared by the list rows and the detail pane.
///
/// Implements `slint::Model`, so the ListView asks only for the rows it is
/// about to show and `MediaRow`s are built on demand, never for all 1,000.
/// Posters are therefore loaded only for those rows and the selection.
struct LibraryView {
    conn: Connection,
    results: RefCell<Vec<MediaItem>>,
    selected: Cell<Option<u32>>,
    notify: ModelNotify,
    posters: RefCell<PosterCache<Image>>,
}

impl LibraryView {
    fn new(conn: Connection) -> rusqlite::Result<Self> {
        let results = db::search(&conn, "")?;
        Ok(Self {
            selected: Cell::new(results.first().map(|item| item.id)),
            results: RefCell::new(results),
            conn,
            notify: ModelNotify::default(),
            posters: RefCell::new(PosterCache::new(
                poster::BUDGET_BYTES,
                slint_poster_loader(poster_dir()),
            )),
        })
    }

    fn to_row(&self, item: &MediaItem) -> MediaRow {
        let poster = self
            .posters
            .borrow_mut()
            .get(poster::poster_number(item.id));
        to_row(item, poster)
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
        Some((row, self.to_row(&results[row])))
    }
}

impl Model for LibraryView {
    type Data = MediaRow;

    fn row_count(&self) -> usize {
        self.results.borrow().len()
    }

    fn row_data(&self, row: usize) -> Option<MediaRow> {
        self.results.borrow().get(row).map(|item| self.to_row(item))
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

/// Muted placeholder tints, picked deterministically per item. The
/// placeholder shows only when the poster cannot be loaded.
const TINTS: [(u8, u8, u8); 6] = [
    (0x5b, 0x6e, 0xae),
    (0x8a, 0x5a, 0x9e),
    (0x3f, 0x8f, 0x86),
    (0xa8, 0x6a, 0x4a),
    (0x6d, 0x86, 0x4a),
    (0xa0, 0x4e, 0x62),
];

/// UI data for one item. `poster` comes from the poster cache; `None` (not
/// loadable) leaves the image empty, so the placeholder shows.
fn to_row(item: &MediaItem, poster: Option<Image>) -> MediaRow {
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
        poster: poster.unwrap_or_default(),
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
    fn rows_and_detail_show_the_poster_mapped_from_the_id() {
        let view = LibraryView::new(db::open(Path::new(":memory:")).unwrap()).unwrap();
        let expected = |id: u32| poster_dir().join(poster::file_name(poster::poster_number(id)));
        for row in [0, 9, 99, 100, 236, 999] {
            let data = view.row_data(row).unwrap();
            assert_eq!(data.poster.path(), Some(expected(data.id as u32).as_path()));
            let size = data.poster.size();
            assert_eq!((size.width, size.height), (240, 360));
        }
        view.select_row(236);
        let (_, detail) = view.selected_row().unwrap();
        assert_eq!(detail.id, 237);
        assert_eq!(
            detail.poster.path(),
            Some(expected(237).as_path()),
            "poster-037"
        );
        // The detail pane went through the same cache: poster 37 was a hit.
        let stats = view.posters.borrow().stats;
        // Posters 1, 10, 100, 37: ids 101 and 1000 reuse 1 and 100.
        assert_eq!((stats.misses, stats.failures), (4, 0));
    }

    #[test]
    fn missing_or_corrupt_poster_falls_back_to_the_placeholder() {
        let dir = std::env::temp_dir().join(format!("bingee-posters-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(poster::file_name(2)), b"not a jpeg").unwrap();
        let mut cache = PosterCache::new(poster::BUDGET_BYTES, slint_poster_loader(dir.clone()));
        let item = |id| library::generate_library().swap_remove(id as usize - 1);

        // Poster 1 is missing, poster 2 is corrupt: empty image, row intact.
        for id in [1, 2, 101] {
            let row = to_row(&item(id), cache.get(poster::poster_number(id)));
            assert_eq!(row.poster.size().width, 0);
            assert_eq!(row.id, id as i32);
            assert!(!row.title.is_empty() && !row.initials.is_empty());
        }
        assert_eq!(
            (cache.stats.misses, cache.stats.failures, cache.len()),
            (2, 2, 0)
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Renders the real `AppWindow` headlessly with Slint's software renderer
    /// at the default 1280×800 size, and counts poster loads through the
    /// shared cache.
    #[test]
    fn only_visible_posters_load_and_the_cache_stays_bounded() {
        use slint::platform::software_renderer::{
            MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel,
        };
        use slint::platform::{Platform, WindowAdapter};

        struct Headless(Rc<MinimalSoftwareWindow>);
        impl Platform for Headless {
            fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
                Ok(self.0.clone())
            }
        }

        let (width, height) = (1280, 800);
        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        // The Slint platform is per thread, and each test runs on its own thread.
        slint::platform::set_platform(Box::new(Headless(window.clone()))).unwrap();
        window.set_size(slint::PhysicalSize::new(width, height));
        let app = AppWindow::new().unwrap();
        let view = Rc::new(LibraryView::new(db::open(Path::new(":memory:")).unwrap()).unwrap());
        connect(&app, view.clone());
        app.show().unwrap();
        let mut buffer = vec![Rgb565Pixel::default(); (width * height) as usize];
        let mut render = || {
            window.draw_if_needed(|renderer| {
                renderer.render(&mut buffer, width as usize);
            });
            view.posters.borrow().stats
        };

        // Startup: the detail poster plus the rows in view, not 1,000.
        let start = render();
        assert!((8..=12).contains(&start.misses), "{start:?}");
        assert_eq!(start.failures, 0);

        // Model reset showing the same rows: served from the cache.
        app.invoke_query_changed("".into());
        let reset = render();
        assert_eq!(reset.misses, start.misses, "no reload after a reset");
        assert!(reset.hits > start.hits);

        // A search and clearing it: only the harbor rows' posters are new.
        app.invoke_query_changed("harbor".into());
        let harbor = render();
        assert!(harbor.misses - reset.misses <= 12, "{harbor:?}");
        // Clearing keeps the selected harbor title and scrolls to it, so at
        // most one screen of its neighbours is new.
        app.invoke_query_changed("".into());
        let cleared = render();
        assert!(cleared.misses - harbor.misses <= 12, "{cleared:?}");
        // Same search again, same rows: every poster is a hit.
        app.invoke_query_changed("harbor".into());
        let again = render();
        assert_eq!(again.misses, cleared.misses, "no reload after a reset");
        assert!(again.hits > cleared.hits);
        assert_eq!(again.evictions, 0, "{again:?}");

        // Long scroll, one screen at a time: each screen loads at most its own
        // rows' posters, the cache evicts, and it never exceeds the budget.
        // 450 rows: ~12× the cache capacity, and quick in a debug build.
        app.invoke_query_changed("".into());
        let mut previous = render().misses;
        for row in (0..450).step_by(9) {
            app.invoke_reveal_row(row);
            let misses = render().misses;
            assert!(
                misses - previous <= 12,
                "row {row}: {} loads",
                misses - previous
            );
            previous = misses;
            let cache = view.posters.borrow();
            assert!(cache.used_bytes() <= poster::BUDGET_BYTES);
            assert!(cache.len() <= poster::BUDGET_BYTES / (240 * 360 * 4));
        }
        let scrolled = view.posters.borrow().stats;
        assert!(scrolled.evictions > 350, "{scrolled:?}");

        // Back to the top: posters 1-10 were evicted by the scroll, so they
        // reload.
        app.invoke_reveal_row(0);
        let back = render();
        assert!(back.misses > scrolled.misses, "{back:?}");
        assert!(view.posters.borrow().used_bytes() <= poster::BUDGET_BYTES);
        for (label, s) in [("start", start), ("reset", reset), ("harbor", harbor)] {
            println!("{label}: misses={} hits={}", s.misses, s.hits);
        }
        println!("scrolled: {scrolled:?}\nback: {}", view.posters.borrow());
    }

    /// Informal R3 observation, not the R4 benchmark. Run with
    /// `cargo test --release informal_poster_timings -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn informal_poster_timings() {
        use std::time::{Duration, Instant};
        let dir = poster_dir();
        let path = |n| dir.join(poster::file_name(n));
        let time = |f: &mut dyn FnMut()| {
            let start = Instant::now();
            f();
            start.elapsed()
        };
        let report = |label: &str, mut samples: Vec<Duration>| {
            samples.sort();
            let us = |d: Duration| d.as_secs_f64() * 1e6;
            let n = samples.len();
            println!(
                "{label:<34} n={n:4} min={:7.1} median={:7.1} p95={:7.1} max={:7.1} us",
                us(samples[0]),
                us(samples[n / 2]),
                us(samples[n * 95 / 100]),
                us(samples[n - 1])
            );
        };

        // First load of each file in this process: read + JPEG decode.
        let cold = (1..=poster::POOL_SIZE)
            .map(|n| time(&mut || drop(Image::load_from_path(&path(n)).unwrap())))
            .collect();
        report("cold load_from_path (decode)", cold);
        // Cycling 100 posters (~25 MB of RGB8) through Slint's 5 MiB
        // internal cache misses every time: a forced re-decode.
        let forced = (1..=poster::POOL_SIZE)
            .cycle()
            .take(500)
            .map(|n| time(&mut || drop(Image::load_from_path(&path(n)).unwrap())))
            .collect();
        report("repeated load_from_path (decode)", forced);
        // The same path again straight away: Slint's internal cache (stat).
        let slint_hit = (0..500)
            .map(|_| time(&mut || drop(Image::load_from_path(&path(1)).unwrap())))
            .collect();
        report("load_from_path, Slint cache hit", slint_hit);
        // Our cache, full (36 entries), hit on the oldest entry: worst scan.
        let mut cache = PosterCache::new(poster::BUDGET_BYTES, slint_poster_loader(dir.clone()));
        for n in 1..=36 {
            cache.get(n);
        }
        let ours = (0..500)
            .map(|i| time(&mut || drop(cache.get(1 + i % 36).unwrap())))
            .collect();
        report("PosterCache hit (36 entries)", ours);
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
