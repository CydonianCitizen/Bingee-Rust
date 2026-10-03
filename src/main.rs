// Slint's recommended setting: no extra console window next to the app window
// in Windows release builds. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod backup;
mod backup_page;
mod calendar;
mod dashboard;
mod database;
mod detail;
mod diagnostics;
mod error;
#[cfg(any(test, feature = "benchmark-fixture"))]
mod fixture;
mod history;
mod home;
mod library;
mod metadata;
mod network;
mod paths;
mod poster;
mod profile_lock;
mod refresh;
mod release;
mod release_page;
mod remote;
mod search;
mod secrets;
mod settings;
mod statistics;
mod statistics_page;
mod tmdb;
mod tracking;
mod view;

#[cfg(test)]
mod r17_validation;

use std::cell::RefCell;
use std::path::Path;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::Arc;

use database::{Database, SharedDb};
use diagnostics::Log;
use error::AppError;
use library::{MediaType, Sort};
use network::Network;
use paths::AppPaths;
use poster::{PosterKey, Posters};
use profile_lock::ProfileLock;
use secrets::KeyringStore;
use settings::Settings;
use slint::{ComponentHandle, Model, SharedString};
use tmdb::TmdbClient;
use view::{LibraryView, UserLibrary};

/// Worker threads for network and credential-store calls (ADR-0008).
const NETWORK_WORKERS: usize = 4;

slint::include_modules!();

/// Display name: window title, About, and the Windows/macOS folder names.
pub const APP_NAME: &str = "Bingee Desktop";
/// Technical id, the Cargo package name: executable, Linux app id and folder
/// names, log file name.
pub const APP_ID: &str = env!("CARGO_PKG_NAME");
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("--benchmark-fixture") => return run_fixture(),
        #[cfg(feature = "r4-measurement")]
        Some("--r4-latency") => {
            return match fixture::r4_latency::run() {
                Ok(()) => ExitCode::SUCCESS,
                Err(error) => {
                    eprintln!("R4 measurement failed: {error}");
                    ExitCode::FAILURE
                }
            };
        }
        _ => {}
    }
    run()
}

/// Normal startup: the user's own library, from the per-user folders.
fn run() -> ExitCode {
    let paths = AppPaths::from_env(&Settings::from_env());
    // The log folder first, so every later failure reaches the log file.
    let log = Arc::new(match &paths {
        Ok(paths) => match std::fs::create_dir_all(&paths.logs) {
            Ok(()) => Log::open(&paths.log_file()),
            Err(err) => {
                eprintln!(
                    "The log folder {} cannot be created: {err}",
                    paths.logs.display()
                );
                Log::stderr_only()
            }
        },
        Err(_) => Log::stderr_only(),
    });
    log.record_panics();
    log.info(format_args!(
        "{APP_NAME} {APP_VERSION} starting ({APP_ID}, {} {})",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));

    let window = match AppWindow::new() {
        Ok(window) => window,
        Err(err) => {
            log.error(format_args!("The window could not be created: {err}"));
            return ExitCode::FAILURE;
        }
    };
    // Wayland app_id / X11 WM_CLASS, matching the executable name. Must be set
    // after the platform exists and before the window is shown. A no-op on
    // Windows and macOS.
    if let Err(err) = slint::set_xdg_app_id(APP_ID) {
        log.error(format_args!("The application id could not be set: {err}"));
    }
    // One HTTP client and one worker pool for search, configuration and
    // posters.
    let network = Network::start(NETWORK_WORKERS);
    let client = TmdbClient::new();
    let poster_dir = paths.as_ref().ok().map(|paths| paths.cache.join("posters"));
    let posters = posters(
        &window,
        poster_dir,
        client.clone(),
        network.clone(),
        log.clone(),
    );
    let db = SharedDb::default();
    let profile_lock = start(&window, paths, log.clone(), db.clone(), posters.clone());
    // Independent of the library: a database failure does not stop remote
    // search, and no network failure reaches the database.
    let token = remote::start(
        &window,
        client.clone(),
        Arc::new(KeyringStore),
        network.clone(),
        log.clone(),
        db.clone(),
        posters.clone(),
    );
    history::start(&window, db.clone(), log.clone());
    backup_page::start(
        &window,
        db.clone(),
        metadata::Clock::system(),
        log.clone(),
        backup_page::NativeDialogs,
    );
    start_maintenance(&window, db.clone(), posters.clone(), log.clone());
    dashboard::start(&window, db.clone(), metadata::Clock::system(), log.clone());
    release_page::start(&window, db.clone(), metadata::Clock::system(), log.clone());
    statistics_page::start(&window, db.clone(), metadata::Clock::system(), log.clone());
    // The Library detail pane: cached details first, TMDB only when stale.
    detail::start(
        &window,
        db.clone(),
        client.clone(),
        token.clone(),
        network.clone(),
        posters.clone(),
        metadata::Clock::system(),
        log.clone(),
    );
    refresh::start(
        &window,
        db,
        token,
        client,
        network,
        metadata::Clock::system(),
        log.clone(),
    );
    let result = window.run();
    profile_lock.borrow_mut().take();
    log.info(posters.stats());
    finish(result, &log)
}

fn start_maintenance(window: &AppWindow, db: SharedDb, posters: Arc<Posters>, log: Arc<Log>) {
    window.on_maintenance_inspect({
        let (window, db, posters, log) = (window.as_weak(), db.clone(), posters.clone(), log.clone());
        move || {
            let result = db.with(Database::quick_check);
            let Some(window) = window.upgrade() else { return };
            match result {
                Ok(()) => {
                    let (count, bytes) = posters.disk_usage();
                    window.set_maintenance_status(format!(
                        "Database check passed. Poster cache: {count} files, {bytes} bytes on disk. Decoded RAM budget: {} MiB.",
                        poster::RAM_BUDGET / (1024 * 1024)
                    ).into());
                }
                Err(error) => {
                    log.error(format_args!("Storage check: {error}"));
                    window.set_maintenance_status(error.message.into());
                }
            }
        }
    });
    window.on_maintenance_clean({
        let window = window.as_weak();
        move || {
            let result = db.with(|db| {
                db.quick_check()?;
                let metadata = db.clean_orphan_metadata()?;
                let (temp, unused) = posters.clean_unused(db)?;
                Ok((metadata, temp, unused))
            });
            let Some(window) = window.upgrade() else { return };
            match result {
                Ok((metadata, temp, unused)) => window.set_maintenance_status(format!(
                    "Removed {metadata} untracked metadata records, {unused} old unreferenced posters and {temp} stale temporary files. Library and personal history retained."
                ).into()),
                Err(error) => {
                    log.error(format_args!("Storage cleanup: {error}"));
                    window.set_maintenance_status(error.message.into());
                }
            }
        }
    });
}

/// The poster service, announcing finished posters to the window.
fn posters(
    window: &AppWindow,
    dir: Option<std::path::PathBuf>,
    client: TmdbClient,
    network: Network,
    log: Arc<Log>,
) -> Arc<Posters> {
    let window = window.as_weak();
    let ready = Box::new(move |key: &PosterKey| {
        if let Some(window) = window.upgrade() {
            poster_ready(&window, key);
        }
    });
    Arc::new(Posters::new(dir, client, network, log, ready))
}

/// A poster is in RAM now: both lists re-read the rows that show it, and the
/// detail panes update if they still show it.
fn poster_ready(window: &AppWindow, key: &PosterKey) {
    let results = window.get_results();
    if let Some(view) = results.as_any().downcast_ref::<LibraryView<UserLibrary>>() {
        view.poster_ready(key);
        if window.get_detail().poster_key == key.name() {
            view::show_selection(window, view);
        }
    }
    if window.get_media_detail().poster_key == key.name() {
        window.invoke_detail_poster_ready();
    }
    remote::poster_ready(window, key);
}

#[cfg(feature = "benchmark-fixture")]
fn run_fixture() -> ExitCode {
    let log = Log::stderr_only();
    let window = match AppWindow::new() {
        Ok(window) => window,
        Err(err) => {
            log.error(format_args!("The window could not be created: {err}"));
            return ExitCode::FAILURE;
        }
    };
    set_app_info(&window);
    fixture::start(&window);
    finish(window.run(), &log)
}

#[cfg(not(feature = "benchmark-fixture"))]
fn run_fixture() -> ExitCode {
    eprintln!("This build has no benchmark fixture. Rebuild with `--features benchmark-fixture`.");
    ExitCode::FAILURE
}

fn finish(result: Result<(), slint::PlatformError>, log: &Log) -> ExitCode {
    match result {
        Ok(()) => {
            log.info(format_args!("{APP_NAME} closed"));
            ExitCode::SUCCESS
        }
        Err(err) => {
            log.error(format_args!("The event loop failed: {err}"));
            ExitCode::FAILURE
        }
    }
}

fn set_app_info(window: &AppWindow) {
    let info = window.global::<AppInfo>();
    info.set_name(APP_NAME.into());
    info.set_version(APP_VERSION.into());
}

/// Fills `window` with the library, or with the startup error page, and
/// wires Try again and Quit. The opened database goes into `db`.
fn start(
    window: &AppWindow,
    paths: Result<AppPaths, AppError>,
    log: Arc<Log>,
    db: SharedDb,
    posters: Arc<Posters>,
) -> Rc<RefCell<Option<ProfileLock>>> {
    set_app_info(window);
    let paths = Rc::new(paths);
    let profile_lock = Rc::new(RefCell::new(None));
    load_library(window, &paths, &log, &db, &posters, &profile_lock);
    window.on_retry({
        let window = window.as_weak();
        let profile_lock = profile_lock.clone();
        move || {
            if let Some(window) = window.upgrade() {
                log.info("Retrying");
                load_library(&window, &paths, &log, &db, &posters, &profile_lock);
            }
        }
    });
    window.on_quit(|| {
        let _ = slint::quit_event_loop();
    });
    profile_lock
}

fn load_library(
    window: &AppWindow,
    paths: &Result<AppPaths, AppError>,
    log: &Arc<Log>,
    db: &SharedDb,
    posters: &Arc<Posters>,
    profile_lock: &RefCell<Option<ProfileLock>>,
) {
    let opened = match paths {
        Ok(paths) => open_production(paths, log, db, posters, profile_lock),
        Err(err) => Err(AppError::new(err.kind, err.message.clone())),
    };
    match opened {
        Ok((view, schema)) => {
            window.set_storage(storage(paths, log, Some(schema)));
            window.set_startup_error(SharedString::new());
            connect_library(window, Rc::new(view), log.clone());
        }
        // Never an empty library, and never a deleted or recreated file:
        // the error page explains and offers Try again.
        Err(err) => {
            log.error(format_args!("Startup failed: {err}"));
            window.set_storage(storage(paths, log, None));
            window.set_startup_error(err.message.into());
        }
    }
}

fn open_production(
    paths: &AppPaths,
    log: &Log,
    db: &SharedDb,
    posters: &Arc<Posters>,
    profile_lock: &RefCell<Option<ProfileLock>>,
) -> Result<(LibraryView<UserLibrary>, u32), AppError> {
    paths.create_dirs()?;
    if profile_lock.borrow().is_none() {
        *profile_lock.borrow_mut() = Some(ProfileLock::acquire(&paths.data)?);
    }
    let path = paths.database();
    log.info(format_args!("Database {}", path.display()));
    let database = Database::open(&path, log)?;
    let schema = database.schema_version()?;
    db.set(database);
    let view = LibraryView::new(UserLibrary::new(db.clone(), posters.clone()))?;
    log.info(format_args!(
        "Library opened: schema version {schema}, {} titles",
        view.row_count()
    ));
    Ok((view, schema))
}

/// The production Library page: `view::connect`, plus the type filter, the
/// sort, Remove from Library, and reloading when Discover adds a title.
fn connect_library(window: &AppWindow, view: Rc<LibraryView<UserLibrary>>, log: Arc<Log>) {
    view::connect(window, view.clone(), log.clone());
    window.set_library_controls(true);
    window.on_open_library_media({
        let (view, window) = (view.clone(), window.as_weak());
        move |id, season, episode| {
            let Some(window) = window.upgrade() else {
                return;
            };
            window.invoke_clear_library_search();
            window.set_page("library".into());
            if let Some(row) = view
                .results
                .borrow()
                .iter()
                .position(|item| item.id == i64::from(id))
            {
                view.select_row(row);
                view::show_selection(&window, &view);
                window.invoke_reveal_row(row as i32);
            } else {
                // History may name a title no longer in Library. Its cached
                // metadata still opens; the list has no selected row.
                view.selected.set(None);
                window.set_selected_row(-1);
                window.set_detail(MediaRow::default());
                window.set_selected_id(id);
            }
            // Slint's selected-id changed handler runs later. Load this title
            // now so season/episode targets use its models, not the old title.
            window.invoke_detail_selected(id);
            if season >= 0
                && let Some(row) = window
                    .get_detail_seasons()
                    .iter()
                    .position(|s| s.number == season)
            {
                window.invoke_detail_season_selected(row as i32);
                if episode >= 0 {
                    let label = format!("E{episode}");
                    if let Some(row) = window
                        .get_detail_episodes()
                        .iter()
                        .position(|e| e.label == label)
                    {
                        window.invoke_detail_episode_selected(row as i32);
                    }
                }
            }
        }
    });
    window.on_library_options_changed({
        let (view, window, log) = (view.clone(), window.as_weak(), log.clone());
        move |filter, sort| {
            view.library.kind.set(match filter.as_str() {
                "movie" => Some(MediaType::Movie),
                "tv" => Some(MediaType::Tv),
                _ => None,
            });
            view.library.sort.set(match sort {
                1 => Sort::Title,
                _ => Sort::RecentlyAdded,
            });
            if let Some(window) = window.upgrade() {
                reload(&window, &view, &log);
            }
        }
    });
    window.on_library_changed({
        let (view, window, log) = (view.clone(), window.as_weak(), log.clone());
        move || {
            if let Some(window) = window.upgrade() {
                reload(&window, &view, &log);
            }
        }
    });
    window.on_library_remove({
        let window = window.as_weak();
        move || {
            let (Some(window), Some(id)) = (window.upgrade(), view.selected.get()) else {
                return;
            };
            // Membership only: metadata, identity and poster stay (ADR-0010).
            match view.library.db.with(|db| library::remove(db, id)) {
                Ok(_) => {
                    log.info(format_args!("Library remove #{id}"));
                    reload(&window, &view, &log);
                    window.invoke_membership_changed();
                }
                Err(err) => {
                    log.error(&err);
                    window.set_error(err.message.into());
                }
            }
        }
    });
}

/// Runs the Library search again and updates the title count.
fn reload(window: &AppWindow, view: &LibraryView<UserLibrary>, log: &Log) {
    // The data changed: personal status is read again with the search.
    view.library.status.take();
    let refreshed = view
        .refresh()
        .and_then(|()| view.library.db.with(library::count));
    match refreshed {
        Ok(total) => {
            window.set_error(SharedString::new());
            window.set_total_count(total as i32);
            view::show_selection(window, view);
        }
        Err(err) => {
            log.error(&err);
            window.set_error(err.message.into());
        }
    }
}

fn storage(paths: &Result<AppPaths, AppError>, log: &Log, schema: Option<u32>) -> Storage {
    let show = |path: &Path| SharedString::from(path.display().to_string());
    let Ok(paths) = paths else {
        let unknown = SharedString::from("Unknown (see the message above)");
        return Storage {
            data: unknown.clone(),
            cache: unknown.clone(),
            log_file: "Not written (no log folder)".into(),
            database: unknown.clone(),
            schema: unknown,
        };
    };
    Storage {
        data: show(&paths.data),
        cache: show(&paths.cache),
        log_file: log.path().map_or_else(
            || "Not written (the log file could not be opened)".into(),
            show,
        ),
        database: show(&paths.database()),
        schema: match schema {
            Some(version) => version.to_string().into(),
            None => "Unknown (the database is not open)".into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::ErrorKind;
    use crate::paths::TestDir;

    /// A real `AppWindow` on Slint's software renderer, without a display.
    /// Time stands still until `advance`. Closures that network jobs post
    /// back (`post`) run on this thread, the UI thread, in `pump`.
    pub struct Headless {
        pub app: AppWindow,
        window: Rc<slint::platform::software_renderer::MinimalSoftwareWindow>,
        clock: Rc<std::cell::Cell<std::time::Duration>>,
        posted: Arc<std::sync::Mutex<Vec<network::Job>>>,
        buffer: Vec<slint::platform::software_renderer::Rgb565Pixel>,
        size: (u32, u32),
    }

    impl Headless {
        pub fn new(width: u32, height: u32) -> Self {
            use slint::platform::software_renderer::{
                MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel,
            };
            use slint::platform::{Platform, WindowAdapter};
            use std::cell::Cell;
            use std::time::Duration;

            struct TestPlatform {
                window: Rc<MinimalSoftwareWindow>,
                clock: Rc<Cell<Duration>>,
            }
            impl Platform for TestPlatform {
                fn create_window_adapter(
                    &self,
                ) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
                    Ok(self.window.clone())
                }
                fn duration_since_start(&self) -> Duration {
                    self.clock.get()
                }
            }

            let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
            let clock = Rc::new(Cell::new(Duration::ZERO));
            // The Slint platform is per thread, and each test runs on its own thread.
            slint::platform::set_platform(Box::new(TestPlatform {
                window: window.clone(),
                clock: clock.clone(),
            }))
            .unwrap();
            window.set_size(slint::PhysicalSize::new(width, height));
            let app = AppWindow::new().unwrap();
            app.show().unwrap();
            Self {
                app,
                window,
                clock,
                posted: Arc::default(),
                buffer: vec![Rgb565Pixel::default(); (width * height) as usize],
                size: (width, height),
            }
        }

        /// A `Network` handoff into this window's queue.
        pub fn post(&self) -> network::Post {
            let posted = self.posted.clone();
            Arc::new(move |job| posted.lock().unwrap().push(job))
        }

        /// Draws a frame if one is needed.
        pub fn render(&mut self) {
            let (buffer, width) = (&mut self.buffer, self.size.0 as usize);
            self.window.draw_if_needed(|renderer| {
                renderer.render(buffer, width);
            });
        }

        pub fn save_png(&mut self, path: &std::path::Path) {
            self.render();
            let (width, height) = self.size;
            let image = image::RgbImage::from_fn(width, height, |x, y| {
                let value = self.buffer[(y * width + x) as usize].0;
                image::Rgb([
                    (((value >> 11) & 31) * 255 / 31) as u8,
                    (((value >> 5) & 63) * 255 / 63) as u8,
                    ((value & 31) * 255 / 31) as u8,
                ])
            });
            image.save(path).unwrap();
        }

        /// Moves the Slint clock forward and fires due timers.
        pub fn advance(&self, by: std::time::Duration) {
            self.clock.set(self.clock.get() + by);
            slint::platform::update_timers_and_animations();
        }

        /// Runs the closures posted to the event loop so far.
        pub fn pump(&self) -> usize {
            let posted = std::mem::take(&mut *self.posted.lock().unwrap());
            let count = posted.len();
            posted.into_iter().for_each(|event| event());
            count
        }

        /// Pumps until `done` holds, for up to 10 s of real time.
        pub fn pump_until(&self, what: &str, done: impl Fn(&AppWindow) -> bool) {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while !done(&self.app) {
                assert!(
                    std::time::Instant::now() < deadline,
                    "timed out waiting for {what}"
                );
                if self.pump() == 0 {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            }
        }
    }

    fn test_paths(dir: &TestDir) -> AppPaths {
        let settings = Settings {
            home_override: Some(dir.0.clone()),
        };
        AppPaths::from_env(&settings).unwrap()
    }

    #[test]
    fn fresh_start_shows_an_empty_library_at_the_latest_schema() {
        let dir = TestDir::new("start-fresh");
        let paths = test_paths(&dir);
        let mut ui = Headless::new(1280, 800);
        let app = ui.app.clone_strong();
        let log = Arc::new(Log::stderr_only());
        start(
            &app,
            Ok(paths.clone()),
            log,
            SharedDb::default(),
            poster::tests::offline(),
        );
        ui.render();
        assert_eq!(app.get_startup_error(), "");
        assert_eq!(
            (app.get_total_count(), app.get_results().row_count()),
            (0, 0)
        );
        assert_eq!(app.get_selected_row(), -1);
        assert_eq!(app.get_page(), "library");
        let storage = app.get_storage();
        assert_eq!(storage.schema, "5");
        assert_eq!(storage.database, paths.database().display().to_string());
        assert!(paths.database().is_file() && paths.cache.is_dir());
        assert_eq!(app.global::<AppInfo>().get_version(), APP_VERSION);

        // Every page renders.
        for page in [
            "home",
            "history",
            "discover",
            "calendar",
            "updates",
            "statistics",
            "settings",
            "about",
            "library",
        ] {
            app.set_page(page.into());
            ui.render();
        }
    }

    /// Release-mode local UI soak over the large synthetic history.
    #[test]
    #[ignore]
    fn r15_large_library_soak() {
        let dir = TestDir::new("r15-soak");
        let paths = test_paths(&dir);
        paths.create_dirs().unwrap();
        crate::statistics::tests::large_history(
            &Database::open(&paths.database(), &Log::stderr_only()).unwrap(),
        );
        let mut ui = Headless::new(1280, 800);
        let app = ui.app.clone_strong();
        let db = SharedDb::default();
        let log = Arc::new(Log::stderr_only());
        let posters = poster::tests::offline();
        let profile_lock = start(&app, Ok(paths), log.clone(), db.clone(), posters);
        let (clock, _) = metadata::Clock::fake(crate::statistics::tests::NOW);
        dashboard::start(&app, db.clone(), clock.clone(), log.clone());
        history::start(&app, db.clone(), log.clone());
        statistics_page::start(&app, db.clone(), clock.clone(), log.clone());
        release_page::start(&app, db, clock, log);
        for cycle in 0..100 {
            for page in [
                "home",
                "library",
                "discover",
                "history",
                "statistics",
                "calendar",
                "updates",
                "settings",
                "about",
            ] {
                app.set_page(page.into());
                match page {
                    "home" => app.invoke_home_opened(),
                    "library" => {
                        app.invoke_query_changed("movie".into());
                        app.invoke_query_changed("".into());
                        app.invoke_row_selected(cycle % 20);
                    }
                    "history" => app.invoke_history_opened(),
                    "statistics" => app.invoke_statistics_opened(),
                    "calendar" => app.invoke_calendar_opened(),
                    "updates" => app.invoke_updates_opened(),
                    _ => {}
                }
                ui.render();
            }
            if cycle % 10 == 0 {
                println!("soak cycle {cycle}");
                std::thread::sleep(std::time::Duration::from_millis(250));
            }
        }
        profile_lock.borrow_mut().take();
    }

    fn offscreen_pages(width: u32, height: u32) {
        use crate::metadata::tests::{episode, season, series, stored};
        let folder = std::path::PathBuf::from("target/offscreen-r18");
        std::fs::create_dir_all(&folder).unwrap();
        let mut ui = Headless::new(width, height);
        let app = ui.app.clone_strong();
        let db = SharedDb::default();
        let dir = TestDir::new(&format!("r18-render-{width}-{height}"));
        let log = Arc::new(Log::stderr_only());
        let posters = poster::tests::offline();
        let _lock = start(
            &app,
            Ok(test_paths(&dir)),
            log.clone(),
            db.clone(),
            posters.clone(),
        );
        let client = TmdbClient::for_tests("http://127.0.0.1:1", std::time::Duration::from_secs(1));
        let token = remote::start(
            &app,
            client.clone(),
            Arc::new(secrets::MemoryStore::default()),
            Network::with_post(1, ui.post()),
            log.clone(),
            db.clone(),
            posters.clone(),
        );
        ui.pump_until("empty credential storage", |app| !app.get_tmdb().busy);
        detail::start(
            &app,
            db.clone(),
            client,
            token,
            Network::with_post(1, ui.post()),
            posters,
            metadata::Clock::system(),
            log.clone(),
        );
        let (clock, _) = metadata::Clock::fake(1_800_000_000);
        dashboard::start(
            &app,
            db.clone(),
            clock.clone(),
            Arc::new(Log::stderr_only()),
        );
        history::start(&app, db.clone(), log.clone());
        statistics_page::start(&app, db.clone(), clock.clone(), log.clone());
        release_page::start(&app, db.clone(), clock, log);
        let picture = |ui: &mut Headless, name: &str| {
            // Let page bindings and stock-widget enabled-state animations
            // settle before capturing, rather than freezing their first frame.
            ui.render();
            ui.advance(std::time::Duration::from_millis(300));
            ui.save_png(&folder.join(format!("{width}x{height}-{name}.png")));
        };
        app.set_page("home".into());
        picture(&mut ui, "home-empty");
        app.set_page("updates".into());
        picture(&mut ui, "updates-empty");
        for page in [
            "library",
            "discover",
            "history",
            "statistics",
            "calendar",
            "settings",
            "about",
        ] {
            app.set_page(page.into());
            picture(&mut ui, &format!("{page}-empty"));
        }
        let (movie, show) = db
            .with(|db| {
                let movie = stored(db, MediaType::Movie, 42, "A journey beyond the familiar");
                metadata::save(
                    db,
                    movie,
                    &metadata::tests::movie("A journey beyond the familiar", vec![]),
                    1,
                )?;
                tracking::watch_movie(db, movie, 1_799_999_900, false)?;
                let id = stored(db, MediaType::Tv, 9, "Offline series");
                metadata::save(
                    db,
                    id,
                    &series("Offline series", vec![season(1, Some(2))]),
                    1,
                )?;
                let today = calendar::local_date(db, 1_800_000_000)?;
                let mut episodes = vec![episode(1, 1), episode(1, 2)];
                episodes[1].air_date = Some(today.clone());
                metadata::save_episodes(db, id, 1, &episodes, 1)?;
                tracking::watch_episode(db, id, 1, 1, 1_799_999_000, false)?;
                db.conn()
                    .execute(
                        "INSERT INTO release_events (local_media_id, media_type,
                season_number, episode_number, event_type, discovered_at, air_date)
                VALUES (?1, 'tv', 1, 2, 'new_episode', ?2, ?3)",
                        rusqlite::params![id, 1_799_999_900, today],
                    )
                    .unwrap();
                Ok((movie, id))
            })
            .unwrap();
        app.invoke_library_changed();
        app.set_page("home".into());
        picture(&mut ui, "home-populated");
        app.set_page("calendar".into());
        picture(&mut ui, "calendar-populated");
        app.set_page("updates".into());
        picture(&mut ui, "updates-populated");
        for page in ["history", "statistics"] {
            app.set_page(page.into());
            picture(&mut ui, &format!("{page}-populated"));
        }
        assert_eq!(app.get_history_rows().row_count(), 2);
        assert_eq!(app.get_statistics().state, "ready");
        app.invoke_open_library_media(movie as i32, -1, -1);
        picture(&mut ui, "movie-detail");
        app.invoke_open_library_media(show as i32, 1, 2);
        picture(&mut ui, "tv-detail");
        picture(&mut ui, "library-populated");
        // Static provider result presentation; request/error behavior has
        // separate fake-server regression coverage.
        let row = view::media_row(
            42,
            MediaType::Movie,
            "A journey beyond the familiar",
            None,
            Some("2026"),
            Some("A cached title ready to open in your Library."),
        );
        app.set_discover_results(slint::ModelRc::new(slint::VecModel::from(vec![
            row.clone(),
        ])));
        app.set_discover_detail(row);
        app.set_discover_selected_row(0);
        app.set_discover_in_library(true);
        app.set_discover(DiscoverView {
            state: "results".into(),
            ready: true,
            summary: "1 result".into(),
            ..Default::default()
        });
        app.set_page("discover".into());
        picture(&mut ui, "discover-populated");
        app.set_backup_status("Backup exported to Documents/Bingee-backup.json".into());
        app.set_backup_file("Documents/Bingee-backup.json".into());
        app.set_backup_preview(
            "Backup checked: 2 titles, 2 watch events. Restore replaces your current saved data after creating a safety backup."
                .into(),
        );
        app.set_backup_ready(true);
        app.set_page("settings".into());
        picture(&mut ui, "settings-backup");
    }

    #[test]
    #[ignore]
    fn offscreen_pages_1280() {
        offscreen_pages(1280, 800);
    }

    #[test]
    #[ignore]
    fn offscreen_pages_1700() {
        offscreen_pages(1700, 1100);
    }

    #[test]
    #[ignore]
    fn offscreen_pages_1920() {
        offscreen_pages(1920, 1080);
    }

    #[test]
    fn home_keyboard_scrolls_to_targets_and_space_opens_them() {
        use slint::platform::{Key, WindowEvent};
        let mut ui = Headless::new(1280, 800);
        let app = ui.app.clone_strong();
        let rows: Vec<_> = (0..32)
            .map(|i| HomeRow {
                title: format!("Series {i}").into(),
                section: "Up Next".into(),
                detail: "S1 E2".into(),
            })
            .collect();
        app.set_home_rows(slint::ModelRc::new(slint::VecModel::from(rows)));
        let opened = Rc::new(std::cell::Cell::new(-1));
        app.on_home_select({
            let opened = opened.clone();
            move |row| opened.set(row)
        });
        app.set_page("home".into());
        ui.render();
        let press = |text: SharedString| {
            app.window()
                .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
            app.window()
                .dispatch_event(WindowEvent::KeyReleased { text });
        };
        press(Key::End.into());
        press(" ".into());
        assert_eq!(opened.get(), 31);
        ui.render();
        let last = ui.buffer.clone();
        press(Key::Home.into());
        press(Key::Return.into());
        assert_eq!(opened.get(), 0);
        ui.render();
        assert!(
            ui.buffer
                .iter()
                .zip(last)
                .filter(|(a, b)| a.0 != b.0)
                .count()
                > 100
        );
        press(Key::PageDown.into());
        press(" ".into());
        assert!(opened.get() > 0 && opened.get() < 31);
        press(Key::PageUp.into());
        press(" ".into());
        assert_eq!(opened.get(), 0);
        app.set_home_rows(Default::default());
        ui.render();
        opened.set(-1);
        press(" ".into());
        assert_eq!(opened.get(), -1, "empty Home must not activate a stale row");
    }

    #[test]
    fn startup_failure_shows_the_error_page_keeps_the_file_and_can_retry() {
        let dir = TestDir::new("start-corrupt");
        let paths = test_paths(&dir);
        paths.create_dirs().unwrap();
        let garbage = vec![0x5a_u8; 8192];
        std::fs::write(paths.database(), &garbage).unwrap();
        let log = Arc::new(Log::open(&paths.log_file()));
        let mut ui = Headless::new(1280, 800);
        let app = ui.app.clone_strong();

        start(
            &app,
            Ok(paths.clone()),
            log,
            SharedDb::default(),
            poster::tests::offline(),
        );
        ui.render();
        let message = app.get_startup_error();
        assert!(message.contains("damaged"), "{message}");
        assert!(!message.to_lowercase().contains("sqlite"), "{message}");
        assert_eq!(
            app.get_storage().database,
            paths.database().display().to_string()
        );
        assert_eq!(
            std::fs::read(paths.database()).unwrap(),
            garbage,
            "file kept"
        );
        let logged = std::fs::read_to_string(paths.log_file()).unwrap();
        assert!(
            logged.contains("ERROR Startup failed: InvalidData error"),
            "{logged}"
        );
        assert!(logged.contains("Cause: file is not a database"), "{logged}");

        // Retrying changes nothing while the file is still damaged...
        app.invoke_retry();
        assert_eq!(app.get_startup_error(), message);
        assert_eq!(std::fs::read(paths.database()).unwrap(), garbage);
        // ...and opens the library once the user has dealt with it.
        std::fs::rename(paths.database(), dir.0.join("damaged.db")).unwrap();
        app.invoke_retry();
        ui.render();
        assert_eq!(app.get_startup_error(), "");
        assert_eq!(app.get_storage().schema, "5");
    }

    #[test]
    fn configuration_error_is_shown_without_touching_any_folder() {
        let mut ui = Headless::new(1280, 800);
        let app = ui.app.clone_strong();
        let error = AppError::new(ErrorKind::Configuration, "No data folder.");
        start(
            &app,
            Err(error),
            Arc::new(Log::stderr_only()),
            SharedDb::default(),
            poster::tests::offline(),
        );
        ui.render();
        assert_eq!(app.get_startup_error(), "No data folder.");
        assert!(app.get_storage().database.starts_with("Unknown"));
        app.set_page("about".into());
        ui.render();
    }

    /// A library of `titles` in an in-memory database, on the production
    /// Library page, with posters from `server`.
    fn library_page(
        ui: &Headless,
        server: &tmdb::fake::FakeServer,
        titles: &[(u32, &str, &str)],
    ) -> (Rc<LibraryView<UserLibrary>>, Arc<Posters>) {
        use crate::search::tests::item;
        let db = SharedDb::default();
        db.set(Database::open_in_memory());
        db.with(|db| {
            for (n, (id, title, poster)) in titles.iter().enumerate() {
                let mut result = item(MediaType::Movie, *id, title);
                result.poster_path = Some((*poster).to_owned());
                library::add(db, &result, n as i64)?;
            }
            Ok(())
        })
        .unwrap();
        let log = Arc::new(Log::stderr_only());
        let client = TmdbClient::for_tests(&server.base, std::time::Duration::from_secs(5));
        let network = Network::with_post(4, ui.post());
        let posters = posters(&ui.app, None, client, network, log.clone());
        posters.set_base_url(format!("{}/t/p/", server.base));
        let view = Rc::new(LibraryView::new(UserLibrary::new(db, posters.clone())).unwrap());
        connect_library(&ui.app, view.clone(), log);
        (view, posters)
    }

    #[test]
    fn a_late_poster_never_lands_on_a_row_that_shows_another_title() {
        use crate::poster::tests::jpeg;
        use tmdb::fake::{FakeServer, Reply};
        let (slow, fast) = (jpeg(185, 278, 10), jpeg(92, 138, 200));
        let server = FakeServer::start(move |seen| match seen.target.as_str() {
            "/t/p/w185/alpha.jpg" => {
                Reply::bytes(200, slow.clone()).after(std::time::Duration::from_millis(400))
            }
            "/t/p/w185/bravo.jpg" => Reply::bytes(200, fast.clone()),
            _ => Reply::json(404, "{}"),
        });
        let mut ui = Headless::new(1280, 800);
        let app = ui.app.clone_strong();
        let (view, posters) = library_page(
            &ui,
            &server,
            &[(1, "Alpha", "/alpha.jpg"), (2, "Bravo", "/bravo.jpg")],
        );

        // Row 0 shows Alpha; its poster starts downloading (slowly).
        app.invoke_query_changed("alpha".into());
        ui.render();
        assert_eq!(app.get_results().row_data(0).unwrap().title, "Alpha");
        // Before it arrives, row 0 is reused for Bravo, whose poster is quick.
        app.invoke_query_changed("bravo".into());
        ui.render();
        ui.pump_until("Bravo's poster", |app| {
            app.get_results().row_data(0).unwrap().poster.size().width == 92
        });
        // Alpha's poster arrives late: row 0 and the detail pane keep Bravo's.
        ui.pump_until("Alpha's poster", |_| {
            posters.idle() && posters.stats().downloads == 2
        });
        ui.render();
        let row = app.get_results().row_data(0).unwrap();
        assert_eq!(
            (row.title.as_str(), row.poster_key.as_str()),
            ("Bravo", "tmdb-w185-bravo.jpg")
        );
        assert_eq!(row.poster.size().width, 92);
        let detail = app.get_detail();
        assert_eq!(
            (detail.title.as_str(), detail.poster.size().width),
            ("Bravo", 92)
        );
        // Alpha's poster is cached for Alpha's row.
        app.invoke_query_changed("".into());
        let alpha = (0..view.row_count())
            .map(|r| view.row_data(r).unwrap())
            .find(|row| row.title == "Alpha")
            .unwrap();
        assert_eq!(alpha.poster.size().width, 185);
    }

    /// 1,000 titles: only the rows on screen load posters, and a long scroll
    /// keeps the decoded cache within its budget.
    #[test]
    fn a_large_library_loads_visible_posters_only_within_budget() {
        use crate::poster::{RAM_BUDGET, tests::jpeg};
        use tmdb::fake::{FakeServer, Reply};
        let image = jpeg(185, 278, 60);
        let server = FakeServer::start(move |_| Reply::bytes(200, image.clone()));
        let names: Vec<(String, String)> = (1..=1000)
            .map(|n| (format!("Title {n:04}"), format!("/p{n}.jpg")))
            .collect();
        let titles: Vec<(u32, &str, &str)> = names
            .iter()
            .enumerate()
            .map(|(i, (title, poster))| (i as u32 + 1, title.as_str(), poster.as_str()))
            .collect();
        let mut ui = Headless::new(1280, 800);
        let app = ui.app.clone_strong();
        let (_view, posters) = library_page(&ui, &server, &titles);
        assert_eq!(app.get_total_count(), 1000);

        ui.render();
        ui.pump_until("the first screen", |_| posters.idle());
        let first = server.seen().len();
        assert!(
            (6..=14).contains(&first),
            "{first} downloads for the first screen"
        );

        // Scroll 240 rows, a screen at a time.
        for row in (0..240).step_by(8) {
            app.invoke_reveal_row(row);
            ui.render();
            ui.pump_until("a screen of posters", |_| posters.idle());
            assert!(posters.ram_bytes() <= RAM_BUDGET);
        }
        let stats = posters.stats();
        let downloads = server.seen().len();
        assert!(
            downloads < 300,
            "{downloads} downloads for ~250 visible rows, not 1,000"
        );
        assert!(stats.evictions > 100, "{stats:?}");
        assert_eq!(stats.failures, 0);
        println!(
            "{stats}; {downloads} downloads; {} bytes in RAM",
            posters.ram_bytes()
        );
    }

    #[test]
    fn identity_comes_from_cargo() {
        assert_eq!(APP_ID, "bingee-desktop");
        assert_eq!(APP_VERSION, env!("CARGO_PKG_VERSION"));
        assert!(!APP_VERSION.is_empty());
    }

    #[test]
    fn episode_list_focus_is_visible_before_a_row_is_selected() {
        use slint::platform::{Key, WindowEvent};
        let mut ui = Headless::new(1280, 800);
        let app = ui.app.clone_strong();
        app.set_library_controls(true);
        app.set_media_detail(MediaDetail {
            has_item: true,
            is_tv: true,
            title: "Focus test series".into(),
            ..Default::default()
        });
        app.set_detail_season(SeasonPanel {
            state: "episodes".into(),
            heading: "Season 1".into(),
            ..Default::default()
        });
        app.set_detail_episodes(slint::ModelRc::new(slint::VecModel::from(vec![
            EpisodeRow {
                label: "E1".into(),
                title: "First episode".into(),
                ..Default::default()
            },
        ])));
        app.set_detail_episode_row(-1);
        ui.render();
        let before = ui.buffer.clone();
        let press = |key: Key| {
            let text: SharedString = key.into();
            app.window()
                .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
            app.window()
                .dispatch_event(WindowEvent::KeyReleased { text });
        };
        // Initial Tab reaches Home; reverse Tab wraps to the last control,
        // the episode list. No pointer or selection establishes focus.
        press(Key::Tab);
        press(Key::Backtab);
        ui.render();
        assert_eq!(app.get_detail_episode_row(), -1);
        let changed = ui
            .buffer
            .iter()
            .zip(&before)
            .enumerate()
            .filter(|(index, (after, before))| {
                index % 1280 > 860 && index / 1280 > 600 && after.0 != before.0
            })
            .count();
        let selected = Rc::new(std::cell::Cell::new(-1));
        app.on_detail_episode_selected({
            let selected = selected.clone();
            move |row| selected.set(row)
        });
        press(Key::DownArrow);
        assert_eq!(selected.get(), 0, "reverse Tab did not reach episodes");
        assert!(
            changed > 100,
            "episode focus has no visible indication before selection: {changed} changed pixels"
        );
    }

    #[test]
    fn updates_list_focus_has_a_visible_indicator() {
        use slint::platform::{Key, WindowEvent};
        let mut ui = Headless::new(1280, 800);
        let app = ui.app.clone_strong();
        app.set_updates_rows(slint::ModelRc::new(slint::VecModel::from(vec![
            UpdateRow {
                title: "A series".into(),
                detail: "New episode S1 E2".into(),
                ..Default::default()
            },
        ])));
        app.set_updates_selected_row(0);
        app.set_page("updates".into());
        ui.render();
        let focused = ui.buffer.clone();
        let text: SharedString = Key::Tab.into();
        app.window()
            .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
        app.window()
            .dispatch_event(WindowEvent::KeyReleased { text });
        ui.render();
        assert_eq!(app.get_updates_selected_row(), 0);
        let changed = ui
            .buffer
            .iter()
            .zip(&focused)
            .enumerate()
            .filter(|(index, (after, before))| {
                index % 1280 > 230 && index / 1280 > 120 && after.0 != before.0
            })
            .count();
        assert!(
            changed > 100,
            "Updates focus is invisible: {changed} changed pixels"
        );
    }

    #[test]
    fn opening_an_episode_target_loads_its_title_before_selecting_the_season() {
        use crate::metadata::tests::{episode, season, series, stored};
        let mut ui = Headless::new(1280, 800);
        let app = ui.app.clone_strong();
        let db = SharedDb::default();
        db.set(Database::open_in_memory());
        let (first, target) = db
            .with(|db| {
                let first = stored(db, MediaType::Tv, 1, "First series");
                metadata::save(
                    db,
                    first,
                    &series("First series", vec![season(1, Some(2))]),
                    1,
                )?;
                let target = stored(db, MediaType::Tv, 2, "Target series");
                metadata::save(
                    db,
                    target,
                    &series(
                        "Target series",
                        vec![season(0, Some(2)), season(1, Some(2))],
                    ),
                    1,
                )?;
                metadata::save_episodes(db, target, 0, &[episode(0, 1), episode(0, 2)], 1)?;
                Ok((first, target))
            })
            .unwrap();
        let log = Arc::new(Log::stderr_only());
        let posters = poster::tests::offline();
        let view =
            Rc::new(LibraryView::new(UserLibrary::new(db.clone(), posters.clone())).unwrap());
        connect_library(&app, view, log.clone());
        app.set_selected_id(first as i32);
        detail::start(
            &app,
            db.clone(),
            TmdbClient::for_tests("http://127.0.0.1:1", std::time::Duration::from_secs(1)),
            secrets::SharedToken::default(),
            Network::with_post(1, ui.post()),
            posters,
            metadata::Clock::system(),
            log,
        );
        ui.render();
        app.invoke_open_library_media(target as i32, 0, 2);
        ui.render();
        assert_eq!(app.get_media_detail().title, "Target series");
        assert_eq!(app.get_detail_season().heading, "Specials");
        assert_eq!(app.get_detail_episode_row(), 1);
        assert_eq!(app.get_detail_episodes().row_data(1).unwrap().label, "E2");
        db.with(|db| library::remove(db, target)).unwrap();
        app.invoke_library_changed();
        app.invoke_open_library_media(first as i32, -1, -1);
        app.invoke_open_library_media(target as i32, 0, 2);
        ui.render();
        assert_eq!(
            app.get_selected_row(),
            -1,
            "removed title kept a stale selection"
        );
        assert_eq!(app.get_media_detail().title, "Target series");
        assert_eq!(app.get_detail_episode_row(), 1);
    }

    /// Page navigation must be reachable through Tab, without a pointer or
    /// a screen-reader-only default action.
    fn sidebar_keyboard_activation(activation: SharedString) {
        use slint::platform::{Key, WindowEvent};
        let mut ui = Headless::new(1280, 800);
        let app = ui.app.clone_strong();
        app.set_page("discover".into());
        ui.render();
        app.set_page("library".into());
        ui.render();
        let press = |text: SharedString| {
            app.window()
                .dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
            app.window()
                .dispatch_event(WindowEvent::KeyReleased { text });
        };
        let reached_home = (0..60).any(|_| {
            press(Key::Tab.into());
            press(activation.clone());
            ui.render();
            app.get_page() == "home"
        });
        assert!(
            reached_home,
            "Tab and activation never reached sidebar Home"
        );
        // Continue from actual keyboard focus, without setting a page or
        // invoking a navigation callback. Each destination must be reachable
        // again after its page has taken focus.
        for destination in [
            "library",
            "discover",
            "history",
            "statistics",
            "calendar",
            "updates",
            "settings",
            "about",
            "home",
        ] {
            let reached = (1..60).any(|tabs| {
                // Do not activate every intermediate control: Home activation
                // deliberately moves focus into its page. Try Tab-only paths
                // between activations, as a keyboard user does.
                for _ in 0..tabs {
                    press(Key::Tab.into());
                }
                press(activation.clone());
                ui.render();
                if app.get_page() == destination {
                    println!("sidebar destination={destination} tabs={tabs}");
                }
                app.get_page() == destination
            });
            assert!(reached, "keyboard navigation trapped before {destination}");
        }
    }

    #[test]
    fn sidebar_navigation_is_reachable_with_tab_and_enter() {
        sidebar_keyboard_activation(slint::platform::Key::Return.into());
    }

    #[test]
    fn sidebar_navigation_is_reachable_with_tab_and_space() {
        sidebar_keyboard_activation(" ".into());
    }
}
