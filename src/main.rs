// Slint's recommended setting: no extra console window next to the app window
// in Windows release builds. Ignored on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod database;
mod diagnostics;
mod error;
#[cfg(any(test, feature = "benchmark-fixture"))]
mod fixture;
mod library;
mod paths;
mod settings;
mod view;

use std::path::Path;
use std::process::ExitCode;
use std::rc::Rc;

use database::Database;
use diagnostics::Log;
use error::AppError;
use paths::AppPaths;
use settings::Settings;
use slint::{ComponentHandle, Model, SharedString};
use view::LibraryView;

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
    let log = Rc::new(match &paths {
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
    start(&window, paths, log.clone());
    finish(window.run(), &log)
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
/// wires Try again and Quit.
fn start(window: &AppWindow, paths: Result<AppPaths, AppError>, log: Rc<Log>) {
    set_app_info(window);
    let paths = Rc::new(paths);
    load_library(window, &paths, &log);
    window.on_retry({
        let window = window.as_weak();
        move || {
            if let Some(window) = window.upgrade() {
                log.info("Retrying");
                load_library(&window, &paths, &log);
            }
        }
    });
    window.on_quit(|| {
        let _ = slint::quit_event_loop();
    });
}

fn load_library(window: &AppWindow, paths: &Result<AppPaths, AppError>, log: &Rc<Log>) {
    let opened = match paths {
        Ok(paths) => open_production(paths, log),
        Err(err) => Err(AppError::new(err.kind, err.message.clone())),
    };
    match opened {
        Ok((view, schema)) => {
            window.set_storage(storage(paths, log, Some(schema)));
            window.set_startup_error(SharedString::new());
            view::connect(window, Rc::new(view), log.clone());
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

fn open_production(paths: &AppPaths, log: &Log) -> Result<(LibraryView<Database>, u32), AppError> {
    paths.create_dirs()?;
    let path = paths.database();
    log.info(format_args!("Database {}", path.display()));
    let db = Database::open(&path, log)?;
    let schema = db.schema_version()?;
    let view = LibraryView::new(db)?;
    log.info(format_args!(
        "Library opened: schema version {schema}, {} titles",
        view.row_count()
    ));
    Ok((view, schema))
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
    /// The closure draws a frame if one is needed.
    pub fn headless(width: u32, height: u32) -> (AppWindow, impl FnMut()) {
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

        let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        // The Slint platform is per thread, and each test runs on its own thread.
        slint::platform::set_platform(Box::new(Headless(window.clone()))).unwrap();
        window.set_size(slint::PhysicalSize::new(width, height));
        let app = AppWindow::new().unwrap();
        app.show().unwrap();
        let mut buffer = vec![Rgb565Pixel::default(); (width * height) as usize];
        let render = move || {
            window.draw_if_needed(|renderer| {
                renderer.render(&mut buffer, width as usize);
            });
        };
        (app, render)
    }

    fn test_paths(dir: &TestDir) -> AppPaths {
        let settings = Settings {
            home_override: Some(dir.0.clone()),
        };
        AppPaths::from_env(&settings).unwrap()
    }

    #[test]
    fn fresh_start_shows_an_empty_library_and_creates_v1() {
        let dir = TestDir::new("start-fresh");
        let paths = test_paths(&dir);
        let (app, mut render) = headless(1280, 800);
        let log = Rc::new(Log::stderr_only());
        start(&app, Ok(paths.clone()), log);
        render();
        assert_eq!(app.get_startup_error(), "");
        assert_eq!(
            (app.get_total_count(), app.get_results().row_count()),
            (0, 0)
        );
        assert_eq!(app.get_selected_row(), -1);
        assert_eq!(app.get_page(), "library");
        let storage = app.get_storage();
        assert_eq!(storage.schema, "1");
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
            render();
        }
    }

    #[test]
    fn startup_failure_shows_the_error_page_keeps_the_file_and_can_retry() {
        let dir = TestDir::new("start-corrupt");
        let paths = test_paths(&dir);
        paths.create_dirs().unwrap();
        let garbage = vec![0x5a_u8; 8192];
        std::fs::write(paths.database(), &garbage).unwrap();
        let log = Rc::new(Log::open(&paths.log_file()));
        let (app, mut render) = headless(1280, 800);

        start(&app, Ok(paths.clone()), log);
        render();
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
        render();
        assert_eq!(app.get_startup_error(), "");
        assert_eq!(app.get_storage().schema, "1");
    }

    #[test]
    fn configuration_error_is_shown_without_touching_any_folder() {
        let (app, mut render) = headless(1280, 800);
        let error = AppError::new(ErrorKind::Configuration, "No data folder.");
        start(&app, Err(error), Rc::new(Log::stderr_only()));
        render();
        assert_eq!(app.get_startup_error(), "No data folder.");
        assert!(app.get_storage().database.starts_with("Unknown"));
        app.set_page("about".into());
        render();
    }

    #[test]
    fn identity_comes_from_cargo() {
        assert_eq!(APP_ID, "bingee-desktop");
        assert_eq!(APP_VERSION, env!("CARGO_PKG_VERSION"));
        assert!(!APP_VERSION.is_empty());
    }
}
