//! The R4 benchmark fixture: the deterministic 1,000-title library, the spike
//! database and the synthetic posters, kept so the R0–R5 workload stays
//! reproducible. Compiled only for tests and the `benchmark-fixture` feature;
//! run with `--benchmark-fixture`. It never touches the production database
//! or the per-user folders.
//!
//! It keeps the R5 portable layout (ADR-0005), relative to the executable:
//!
//! ```text
//! <exe dir>/data/bingee-spike.db        (created and seeded on first launch)
//! <exe dir>/assets/posters/poster-001.jpg …   (or the checkout's benchmark/)
//! ```

mod db;
pub(crate) mod library;
mod poster;

#[cfg(feature = "r4-measurement")]
#[path = "../../benchmark/r4_latency.rs"]
pub mod r4_latency;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use library::MediaItem;
use poster::PosterCache;
use rusqlite::Connection;
use slint::Image;

use crate::diagnostics::Log;
use crate::error::AppError;
use crate::view::{self, Library};
use crate::{AppWindow, MediaRow};

/// The fixture's view type (the R4 harness names it this way).
type LibraryView = view::LibraryView<FixtureLibrary>;

/// The spike database and the poster cache shared by the list rows and the
/// detail pane.
pub struct FixtureLibrary {
    conn: Connection,
    posters: RefCell<PosterCache<Image>>,
}

impl FixtureLibrary {
    fn new(conn: Connection) -> Self {
        Self {
            conn,
            posters: RefCell::new(PosterCache::new(
                poster::BUDGET_BYTES,
                slint_poster_loader(poster_dir()),
            )),
        }
    }
}

impl Library for FixtureLibrary {
    type Item = MediaItem;

    fn search(&self, query: &str) -> Result<Vec<MediaItem>, AppError> {
        db::search(&self.conn, query)
            .map_err(|err| AppError::database("The benchmark fixture could not be searched.", err))
    }

    fn id(item: &MediaItem) -> i64 {
        item.id.into()
    }

    /// Posters are loaded only for the rows the ListView asks for and the
    /// selection.
    fn row(&self, item: &MediaItem) -> MediaRow {
        let poster = self
            .posters
            .borrow_mut()
            .get(poster::poster_number(item.id));
        to_row(item, poster)
    }
}

/// Starts the fixture in `window`, with stderr as its only log.
#[cfg(feature = "benchmark-fixture")]
pub fn start(window: &AppWindow) {
    use slint::ComponentHandle;
    window
        .global::<crate::AppInfo>()
        .set_mode("Benchmark fixture".into());
    let database = database_path().map_or_else(|_| "unknown".into(), |p| p.display().to_string());
    window.set_storage(crate::Storage {
        data: database.as_str().into(),
        cache: "none (posters are read from the benchmark assets)".into(),
        log_file: "stderr only".into(),
        database: database.into(),
        schema: "unversioned R2 spike schema".into(),
    });
    // No network, threads or credential store in the benchmark workload.
    let off = "Not available in the benchmark fixture";
    window.set_tmdb(crate::TmdbView {
        status: off.into(),
        ..Default::default()
    });
    window.set_discover(crate::DiscoverView {
        state: "message".into(),
        title: off.into(),
        ..Default::default()
    });
    match open_library() {
        Ok(view) => connect(window, Rc::new(view)),
        Err(err) => {
            eprintln!("{err}");
            window.set_startup_error(err.message.into());
        }
    }
}

/// The R5 package root: the directory holding the executable. Never the
/// current working directory.
fn package_root() -> std::io::Result<PathBuf> {
    let exe = std::env::current_exe()?;
    exe.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| std::io::Error::other("the executable has no parent directory"))
}

/// `<root>/data/bingee-spike.db`, creating `data/` if needed. During
/// development the root is `target/debug/` or `target/release/`.
#[cfg(feature = "benchmark-fixture")]
fn database_path() -> std::io::Result<PathBuf> {
    database_path_in(&package_root()?)
}

fn database_path_in(root: &Path) -> std::io::Result<PathBuf> {
    let dir = root.join("data");
    std::fs::create_dir_all(&dir)?;
    Ok(dir.join("bingee-spike.db"))
}

/// The package's `assets/posters`. A build-tree binary (`cargo run`, tests)
/// has none, so it falls back to the benchmark posters in the checkout it was
/// built from.
fn poster_dir() -> PathBuf {
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR")).join("benchmark/assets/posters");
    match package_root() {
        Ok(root) => poster_dir_in(&root, checkout),
        Err(_) => checkout,
    }
}

fn poster_dir_in(root: &Path, fallback: PathBuf) -> PathBuf {
    let packaged = root.join("assets").join("posters");
    if packaged.is_dir() {
        packaged
    } else {
        fallback
    }
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

#[cfg(feature = "benchmark-fixture")]
fn open_library() -> Result<LibraryView, AppError> {
    let path = database_path().map_err(|err| {
        AppError::filesystem("The benchmark fixture folder could not be prepared.", err)
    })?;
    let conn = db::open(&path)
        .map_err(|err| AppError::database("The benchmark fixture could not be loaded.", err))?;
    LibraryView::new(FixtureLibrary::new(conn))
}

/// `view::connect`, plus F12 in the list to print the poster cache counters
/// (R3 debug aid; no polling, nothing leaves the process).
fn connect(window: &AppWindow, view: Rc<LibraryView>) {
    view::connect(window, view.clone(), Arc::new(Log::stderr_only()));
    window.on_debug_dump(move || eprintln!("{}", view.library.posters.borrow()));
}

/// UI data for one item. `poster` comes from the poster cache; `None` (not
/// loadable) leaves the image empty, so the placeholder shows.
fn to_row(item: &MediaItem, poster: Option<Image>) -> MediaRow {
    MediaRow {
        id: item.id as i32,
        title: item.title.as_str().into(),
        original_title: item.original_title.as_str().into(),
        meta: format!("{} · {}", item.kind.label(), item.year).into(),
        initials: item.initials().into(),
        status: item.progress.label().into(),
        progress: item.progress.fraction(),
        overview: item.overview.as_str().into(),
        tint: view::tint(item.id.into()),
        poster: poster.unwrap_or_default(),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use crate::error::ErrorKind;
    use crate::paths::{AppPaths, TestDir};
    use crate::settings::Settings;
    use slint::{ComponentHandle, Model};

    fn memory_view() -> LibraryView {
        LibraryView::new(FixtureLibrary::new(
            db::open(Path::new(":memory:")).unwrap(),
        ))
        .unwrap()
    }

    /// Selected (row, id), checked against the row the list would render.
    fn selection(view: &LibraryView) -> Option<(usize, u32)> {
        let (row, detail) = view.selected_row()?;
        assert_eq!(view.row_data(row).map(|r| r.id), Some(detail.id));
        Some((row, detail.id as u32))
    }

    #[test]
    fn selection_tracks_stable_id_and_never_leaves_the_results() {
        let view = memory_view();
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
        let view = memory_view();
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
        let stats = view.library.posters.borrow().stats;
        // Posters 1, 10, 100, 37: ids 101 and 1000 reuse 1 and 100.
        assert_eq!((stats.misses, stats.failures), (4, 0));
    }

    #[test]
    fn missing_or_corrupt_poster_falls_back_to_the_placeholder() {
        let dir = TestDir::new("posters");
        std::fs::write(dir.0.join(poster::file_name(2)), b"not a jpeg").unwrap();
        let mut cache = PosterCache::new(poster::BUDGET_BYTES, slint_poster_loader(dir.0.clone()));
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
    }

    /// Renders the real `AppWindow` headlessly with Slint's software renderer
    /// at the default 1280×800 size, and counts poster loads through the
    /// shared cache.
    #[test]
    fn only_visible_posters_load_and_the_cache_stays_bounded() {
        let mut ui = crate::tests::Headless::new(1280, 800);
        let app = ui.app.clone_strong();
        let view = Rc::new(memory_view());
        connect(&app, view.clone());
        let mut render = || {
            ui.render();
            view.library.posters.borrow().stats
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
            let cache = view.library.posters.borrow();
            assert!(cache.used_bytes() <= poster::BUDGET_BYTES);
            assert!(cache.len() <= poster::BUDGET_BYTES / (240 * 360 * 4));
        }
        let scrolled = view.library.posters.borrow().stats;
        assert!(scrolled.evictions > 350, "{scrolled:?}");

        // Back to the top: posters 1-10 were evicted by the scroll, so they
        // reload.
        app.invoke_reveal_row(0);
        let back = render();
        assert!(back.misses > scrolled.misses, "{back:?}");
        assert!(view.library.posters.borrow().used_bytes() <= poster::BUDGET_BYTES);
        for (label, s) in [("start", start), ("reset", reset), ("harbor", harbor)] {
            println!("{label}: misses={} hits={}", s.misses, s.hits);
        }
        println!(
            "scrolled: {scrolled:?}\nback: {}",
            view.library.posters.borrow()
        );
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
    fn package_paths_derive_from_the_executable_directory() {
        // Under `cargo test` the working directory is the checkout, while the
        // executable sits in target/*/deps: the root must follow the latter.
        let exe = std::env::current_exe().unwrap();
        assert_eq!(package_root().unwrap(), exe.parent().unwrap());
        assert_ne!(package_root().unwrap(), std::env::current_dir().unwrap());

        let root = TestDir::new("package");
        let checkout = PathBuf::from("checkout/benchmark/assets/posters");

        // No packaged assets (a build tree): the checkout fallback.
        assert_eq!(poster_dir_in(&root.0, checkout.clone()), checkout);
        // Packaged assets win.
        let packaged = root.0.join("assets").join("posters");
        std::fs::create_dir_all(&packaged).unwrap();
        assert_eq!(poster_dir_in(&root.0, checkout), packaged);

        // The database lives in <root>/data, created on demand, and opens.
        let path = database_path_in(&root.0).unwrap();
        assert_eq!(path, root.0.join("data").join("bingee-spike.db"));
        drop(db::open(&path).unwrap());
        assert!(path.is_file());
    }

    #[test]
    fn fixture_data_stays_separate_from_production() {
        // Production folders come from the per-user environment; the fixture
        // database sits beside the executable. Neither contains the other.
        let production = AppPaths::from_env(&Settings::default()).unwrap();
        let fixture = package_root().unwrap().join("data").join("bingee-spike.db");
        for dir in [&production.data, &production.cache, &production.logs] {
            assert!(!fixture.starts_with(dir), "{}", dir.display());
            assert!(
                !dir.starts_with(fixture.parent().unwrap()),
                "{}",
                dir.display()
            );
        }
        assert_ne!(production.database().file_name(), fixture.file_name());

        // A seeded fixture database is refused by the production code and
        // left byte-identical, so the two can never be mixed up.
        let dir = TestDir::new("fixture-vs-production");
        let path = dir.0.join("bingee-spike.db");
        drop(db::open(&path).unwrap());
        let before = std::fs::read(&path).unwrap();
        let error = Database::open(&path, &Log::stderr_only()).err().unwrap();
        assert_eq!(error.kind, ErrorKind::InvalidData);
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}
