// Slint's recommended setting: no extra console window next to the app window
// in Windows release builds. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod database;
mod detail;
mod diagnostics;
mod error;
#[cfg(any(test, feature = "benchmark-fixture"))]
mod fixture;
mod library;
mod metadata;
mod network;
mod paths;
mod poster;
mod remote;
mod search;
mod secrets;
mod settings;
mod tmdb;
mod view;

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
    start(&window, paths, log.clone(), db.clone(), posters.clone());
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
    // The Library detail pane: cached details first, TMDB only when stale.
    detail::start(
        &window,
        db,
        client,
        token,
        network,
        posters.clone(),
        metadata::Clock::system(),
        log.clone(),
    );
    let result = window.run();
    log.info(posters.stats());
    finish(result, &log)
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
) {
    set_app_info(window);
    let paths = Rc::new(paths);
    load_library(window, &paths, &log, &db, &posters);
    window.on_retry({
        let window = window.as_weak();
        move || {
            if let Some(window) = window.upgrade() {
                log.info("Retrying");
                load_library(&window, &paths, &log, &db, &posters);
            }
        }
    });
    window.on_quit(|| {
        let _ = slint::quit_event_loop();
    });
}

fn load_library(
    window: &AppWindow,
    paths: &Result<AppPaths, AppError>,
    log: &Arc<Log>,
    db: &SharedDb,
    posters: &Arc<Posters>,
) {
    let opened = match paths {
        Ok(paths) => open_production(paths, log, db, posters),
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
) -> Result<(LibraryView<UserLibrary>, u32), AppError> {
    paths.create_dirs()?;
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
        assert_eq!(storage.schema, "2");
        assert_eq!(storage.database, paths.database().display().to_string());
        assert!(paths.database().is_file() && paths.cache.is_dir());
        assert_eq!(app.global::<AppInfo>().get_version(), APP_VERSION);

        // Every page renders.
        for page in [
            "home",
            "discover",
            "calendar",
            "statistics",
            "settings",
            "about",
            "library",
        ] {
            app.set_page(page.into());
            ui.render();
        }
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
        assert_eq!(app.get_storage().schema, "2");
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
}
