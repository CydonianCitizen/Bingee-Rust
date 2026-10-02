//! Release validation only; no production instrumentation or network service.

use super::*;
use crate::tests::Headless;
use std::time::{Duration, Instant};

/// A real native window with deterministic TMDB responses for manual keyboard
/// validation. Requires an explicit isolated profile; never reads a credential.
#[test]
#[ignore = "manual native desktop keyboard validation"]
fn r17_native_keyboard_fixture() {
    use crate::tmdb::fake::{FakeServer, Reply, movie_details, season_details, tv_details};
    let server = FakeServer::start(|seen| match seen.target.split('?').next().unwrap() {
        "/3/authentication" => Reply::json(200, r#"{"success":true}"#),
        "/3/search/movie" => Reply::json(
            200,
            r#"{"page":1,"total_pages":1,"total_results":1,"results":[{"id":603,"title":"Native Movie"}]}"#,
        ),
        "/3/search/tv" => Reply::json(
            200,
            r#"{"page":1,"total_pages":1,"total_results":1,"results":[{"id":603,"name":"Native Series"}]}"#,
        ),
        "/3/movie/603" => Reply::json(200, &movie_details(603, "Native Movie")),
        "/3/tv/603" => Reply::json(200, &tv_details(603, "Native Series", &[(0, 2), (1, 3)])),
        "/3/tv/603/season/1" => Reply::json(200, &season_details(1, 3)),
        _ => Reply::json(404, "{}"),
    });
    let root = std::path::PathBuf::from(
        std::env::var_os("BINGEE_R17_NATIVE_HOME").expect("explicit isolated native profile"),
    );
    assert!(root.is_absolute(), "native profile must be absolute");
    let paths = AppPaths::from_env(&Settings {
        home_override: Some(root),
    })
    .unwrap();
    paths.create_dirs().unwrap();
    let app = AppWindow::new().unwrap();
    let log = Arc::new(Log::stderr_only());
    let db = SharedDb::default();
    let network = Network::start(NETWORK_WORKERS);
    let client = TmdbClient::for_tests(&server.base, Duration::from_secs(1));
    let posters = poster::tests::offline();
    let lock = start(&app, Ok(paths), log.clone(), db.clone(), posters.clone());
    let token = remote::start(
        &app,
        client.clone(),
        Arc::new(secrets::MemoryStore::with("r17-fake-token")),
        network.clone(),
        log.clone(),
        db.clone(),
        posters.clone(),
    );
    let clock = metadata::Clock::system();
    detail::start(
        &app,
        db.clone(),
        client.clone(),
        token.clone(),
        network.clone(),
        posters.clone(),
        clock.clone(),
        log.clone(),
    );
    history::start(&app, db.clone(), log.clone());
    dashboard::start(&app, db.clone(), clock.clone(), log.clone());
    statistics_page::start(&app, db.clone(), clock.clone(), log.clone());
    release_page::start(&app, db.clone(), clock.clone(), log.clone());
    backup_page::start(
        &app,
        db.clone(),
        clock.clone(),
        log.clone(),
        backup_page::NativeDialogs,
    );
    refresh::start(&app, db.clone(), token, client, network, clock, log);
    app.run().unwrap();
    lock.borrow_mut().take();
    db.with(Database::quick_check).unwrap();
    assert!(
        db.with(library::count).unwrap() >= 1,
        "native journey must add a title"
    );
    eprintln!("Native keyboard fixture closed; database check passed.");
}

/// One continuous product journey, followed by a new offline UI session.
#[test]
fn r17_search_track_refresh_backup_restore_offline() {
    use crate::tmdb::fake::{FakeServer, Reply, movie_details, season_details, tv_details};
    #[derive(Clone)]
    struct BackupPath(std::path::PathBuf);
    impl backup_page::FileChooser for BackupPath {
        fn save_backup(&self) -> Option<std::path::PathBuf> {
            Some(self.0.clone())
        }
        fn open_backup(&self) -> Option<std::path::PathBuf> {
            Some(self.0.clone())
        }
    }
    let server = FakeServer::start(|seen| {
        let path = seen.target.split('?').next().unwrap();
        match path {
            "/3/authentication" => Reply::json(200, r#"{"success":true}"#),
            "/3/search/movie" => Reply::json(
                200,
                r#"{"page":1,"total_pages":1,"total_results":1,"results":[{"id":603,"title":"Journey Movie"}]}"#,
            ),
            "/3/search/tv" => Reply::json(
                200,
                r#"{"page":1,"total_pages":1,"total_results":1,"results":[{"id":603,"name":"Journey Series"}]}"#,
            ),
            "/3/movie/603" => Reply::json(200, &movie_details(603, "Journey Movie")),
            "/3/tv/603" => Reply::json(200, &tv_details(603, "Journey Series", &[(0, 2), (1, 3)])),
            "/3/tv/603/season/1" => Reply::json(200, &season_details(1, 3)),
            _ => Reply::json(404, "{}"),
        }
    });
    let dir = paths::TestDir::new("r17-complete-journey");
    let paths = AppPaths::from_env(&Settings {
        home_override: Some(dir.0.clone()),
    })
    .unwrap();
    paths.create_dirs().unwrap();
    for online in [true, false] {
        let requests_before = server.seen().len();
        let (paths, base) = (paths.clone(), server.base.clone());
        std::thread::spawn(move || {
            let mut ui = Headless::new(1280, 800);
            let app = ui.app.clone_strong();
            let log = Arc::new(Log::stderr_only());
            let db = SharedDb::default();
            let network = Network::with_post(4, ui.post());
            let client = TmdbClient::for_tests(&base, Duration::from_secs(1));
            let posters = poster::tests::offline();
            let lock = start(
                &app,
                Ok(paths.clone()),
                log.clone(),
                db.clone(),
                posters.clone(),
            );
            let token = remote::start(
                &app,
                client.clone(),
                Arc::new(if online {
                    secrets::MemoryStore::with("r17-fake-token")
                } else {
                    secrets::MemoryStore::default()
                }),
                network.clone(),
                log.clone(),
                db.clone(),
                posters.clone(),
            );
            let (clock, _) = metadata::Clock::fake(statistics::tests::NOW);
            detail::start(
                &app,
                db.clone(),
                client,
                token,
                network,
                posters,
                clock.clone(),
                log.clone(),
            );
            history::start(&app, db.clone(), log.clone());
            dashboard::start(&app, db.clone(), clock.clone(), log.clone());
            statistics_page::start(&app, db.clone(), clock.clone(), log.clone());
            release_page::start(&app, db.clone(), clock.clone(), log.clone());
            backup_page::start(
                &app,
                db.clone(),
                clock,
                log,
                BackupPath(paths.data.join("journey.json")),
            );
            if online {
                ui.pump_until("credential", |app| {
                    app.get_tmdb().status == "Connected to TMDB"
                });
                app.set_page("discover".into());
                app.invoke_discover_query_changed("journey".into());
                ui.advance(Duration::from_millis(300));
                ui.pump_until("two identities", |app| {
                    app.get_discover_results().row_count() == 2
                });
                for row in 0..2 {
                    app.invoke_discover_row_selected(row);
                    app.invoke_discover_add();
                }
                assert_eq!(db.with(library::count).unwrap(), 2);
                app.set_page("library".into());
                app.invoke_detail_selected(1);
                ui.advance(Duration::from_millis(300));
                ui.pump_until("movie details", |app| !app.get_media_detail().refreshing);
                app.invoke_detail_watch_toggle();
                app.invoke_detail_rate(8);
                app.invoke_detail_selected(2);
                ui.advance(Duration::from_millis(300));
                ui.pump_until("season episodes", |app| {
                    app.get_detail_episodes().row_count() == 3
                });
                app.invoke_detail_episode_toggle(0);
                app.invoke_detail_rate(9);
                let personal = db
                    .with(|db| backup::export(db, statistics::tests::NOW))
                    .unwrap()
                    .data;
                app.invoke_detail_refresh();
                ui.advance(Duration::from_millis(300));
                ui.pump_until("manual refresh", |app| !app.get_media_detail().refreshing);
                assert_eq!(app.get_media_detail().notice, "");
                let after = db
                    .with(|db| backup::export(db, statistics::tests::NOW))
                    .unwrap()
                    .data;
                assert_eq!(after.media_tracking, personal.media_tracking);
                assert_eq!(after.episode_tracking, personal.episode_tracking);
                assert_eq!(after.watch_events, personal.watch_events);
                app.set_page("settings".into());
                app.invoke_backup_export();
                assert!(app.get_backup_status().contains("exported"));
                db.with(|db| tracking::set_rating(db, 1, Some(1))).unwrap();
                app.invoke_backup_select();
                assert!(app.get_backup_ready());
                app.invoke_backup_confirm();
                assert!(app.get_backup_status().starts_with("Restore complete"));
                assert!(std::fs::read_dir(&paths.data).unwrap().flatten().any(|e| {
                    e.file_name()
                        .to_string_lossy()
                        .starts_with("bingee-pre-restore-")
                }));
            }
            let expected = backup::read_from_path(&paths.data.join("journey.json")).unwrap();
            assert_eq!(
                db.with(|db| backup::export(db, statistics::tests::NOW))
                    .unwrap(),
                expected
            );
            db.with(Database::quick_check).unwrap();
            if !online {
                app.set_page("library".into());
                app.invoke_detail_selected(2);
                ui.advance(Duration::from_millis(300));
                assert_eq!(app.get_detail_episodes().row_count(), 3);
                assert_eq!(expected.data.watch_events.len(), 2);
                for page in ["home", "history", "statistics", "calendar", "updates"] {
                    app.set_page(page.into());
                    match page {
                        "home" => app.invoke_home_opened(),
                        "history" => app.invoke_history_opened(),
                        "statistics" => app.invoke_statistics_opened(),
                        "calendar" => app.invoke_calendar_opened(),
                        _ => app.invoke_updates_opened(),
                    }
                    ui.render();
                }
            }
            lock.borrow_mut().take();
        })
        .join()
        .unwrap();
        if online {
            assert!(
                server
                    .seen()
                    .iter()
                    .filter(|r| r.target.starts_with("/3/tv/603/season/1"))
                    .count()
                    >= 2
            );
        } else {
            assert_eq!(
                server.seen().len(),
                requests_before,
                "offline restart made a request"
            );
        }
    }
}

/// Kill an isolated test process during the actual export or metadata write.
/// The child uses production persistence and profile locking, with fake data.
#[test]
#[ignore]
fn r17_active_write_crash_recovery() {
    if let Ok(root) = std::env::var("BINGEE_R17_CRASH_PROFILE") {
        let root = std::path::PathBuf::from(root);
        let _lock = ProfileLock::acquire(&root).unwrap();
        let db = Database::open(&root.join("bingee.db"), &Log::stderr_only()).unwrap();
        if std::env::var("BINGEE_R17_CRASH_MODE").unwrap() == "export" {
            backup::export_to_path(&db, &root.join("export.json"), statistics::tests::NOW).unwrap();
        } else {
            let episodes: Vec<_> = (1..=50_000)
                .map(|n| metadata::tests::episode(1, n))
                .collect();
            metadata::save_episodes(&db, 851, 1, &episodes, statistics::tests::NOW).unwrap();
        }
        return;
    }
    let seed_dir = paths::TestDir::new("r17-crash-seed");
    let seed = Database::open(&seed_dir.0.join("seed.db"), &Log::stderr_only()).unwrap();
    statistics::tests::large_history(&seed);
    let original = backup::export(&seed, statistics::tests::NOW).unwrap();
    drop(seed);
    for mode in ["export", "refresh"] {
        let dir = paths::TestDir::new("r17-crash-child");
        let path = dir.0.join("bingee.db");
        std::fs::copy(seed_dir.0.join("seed.db"), &path).unwrap();
        let mut child = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "r17_validation::r17_active_write_crash_recovery",
                "--ignored",
                "--nocapture",
            ])
            .env("BINGEE_R17_CRASH_PROFILE", &dir.0)
            .env("BINGEE_R17_CRASH_MODE", mode)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            let active = if mode == "export" {
                std::fs::read_dir(&dir.0).unwrap().flatten().any(|entry| {
                    entry.file_name().to_string_lossy().contains(".bingee-")
                        && entry.metadata().is_ok_and(|m| m.len() > 0)
                })
            } else {
                path.with_extension("db-journal")
                    .metadata()
                    .is_ok_and(|m| m.len() > 512)
            };
            if active {
                break;
            }
            if child.try_wait().unwrap().is_some() || Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                panic!("did not observe active {mode} write before exit/timeout");
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        child.kill().unwrap();
        let status = child.wait().unwrap();
        assert!(!status.success());
        let _lock = ProfileLock::acquire(&dir.0).unwrap();
        let recovered = Database::open(&path, &Log::stderr_only()).unwrap();
        assert_eq!(
            backup::export(&recovered, statistics::tests::NOW).unwrap(),
            original
        );
        let integrity: String = recovered
            .conn()
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .unwrap();
        assert_eq!(integrity, "ok");
        let foreign_keys: i64 = recovered
            .conn()
            .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(foreign_keys, 0);
        println!(
            "R17 crash mode={mode}: active write observed, killed, lock reacquired, original state recovered, integrity and foreign keys PASS"
        );
    }
}

/// Run in a separate release test process with --ignored --nocapture.
/// External process samples are paired with the cycle markers below.
#[test]
#[ignore]
fn r17_product_soak() {
    use crate::tmdb::fake::{FakeServer, Reply, movie_details, season_details, tv_details};
    let dir = paths::TestDir::new("r17-product");
    let paths = AppPaths::from_env(&Settings {
        home_override: Some(dir.0.clone()),
    })
    .unwrap();
    paths.create_dirs().unwrap();
    let seed = Database::open(&paths.database(), &Log::stderr_only()).unwrap();
    statistics::tests::large_history(&seed);
    drop(seed);
    if let Some(destination) = std::env::var_os("BINGEE_R17_SEED_COPY") {
        std::fs::copy(paths.database(), destination).unwrap();
    }
    let jpeg = poster::tests::jpeg(185, 278, 80);
    let server = FakeServer::start(move |seen| {
        let path = seen.target.split('?').next().unwrap();
        if path.starts_with("/t/p/") {
            return Reply::bytes(200, jpeg.clone());
        }
        let parts: Vec<_> = path.split('/').collect();
        match path {
            "/3/authentication" => Reply::json(200, r#"{"success":true}"#),
            "/3/configuration" => Reply::json(503, "{}"),
            "/3/search/movie" => Reply::json(
                200,
                r#"{"page":1,"total_pages":1,"total_results":1,"results":[{"id":603,"title":"Soak Movie"}]}"#,
            ),
            "/3/search/tv" => Reply::json(
                200,
                r#"{"page":1,"total_pages":1,"total_results":0,"results":[]}"#,
            ),
            _ if parts.len() == 6 && parts[4] == "season" => {
                Reply::json(200, &season_details(parts[5].parse().unwrap(), 23))
            }
            _ if parts.len() == 4 && parts[2] == "tv" => Reply::json(
                200,
                &tv_details(
                    parts[3].parse().unwrap(),
                    "Soak Series",
                    &[(0, 2), (1, 23), (2, 23), (3, 23), (4, 23), (5, 23)],
                ),
            ),
            _ if parts.len() == 4 && parts[2] == "movie" => {
                Reply::json(200, &movie_details(parts[3].parse().unwrap(), "Soak Movie"))
            }
            _ => Reply::json(404, "{}"),
        }
    });
    let mut ui = Headless::new(1280, 800);
    let app = ui.app.clone_strong();
    let log = Arc::new(Log::stderr_only());
    let db = SharedDb::default();
    let network = Network::with_post(4, ui.post());
    let client = TmdbClient::for_tests(&server.base, Duration::from_secs(2));
    let posters = posters(
        &app,
        Some(paths.cache.join("posters")),
        client.clone(),
        network.clone(),
        log.clone(),
    );
    posters.set_base_url(format!("{}/t/p/", server.base));
    let lock = start(
        &app,
        Ok(paths.clone()),
        log.clone(),
        db.clone(),
        posters.clone(),
    );
    let token = remote::start(
        &app,
        client.clone(),
        Arc::new(secrets::MemoryStore::with("r17-fake-token")),
        network.clone(),
        log.clone(),
        db.clone(),
        posters.clone(),
    );
    let (clock, _) = metadata::Clock::fake(statistics::tests::NOW);
    history::start(&app, db.clone(), log.clone());
    dashboard::start(&app, db.clone(), clock.clone(), log.clone());
    statistics_page::start(&app, db.clone(), clock.clone(), log.clone());
    release_page::start(&app, db.clone(), clock.clone(), log.clone());
    detail::start(
        &app,
        db.clone(),
        client.clone(),
        token.clone(),
        network.clone(),
        posters.clone(),
        clock.clone(),
        log.clone(),
    );
    refresh::start(&app, db.clone(), token, client, network, clock, log.clone());
    ui.pump_until("fake credential", |app| {
        app.get_tmdb().status == "Connected to TMDB"
    });
    app.invoke_automatic_refresh_toggle();
    let mut timings = Vec::new();
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
            let started = Instant::now();
            app.set_page(page.into());
            match page {
                "home" => app.invoke_home_opened(),
                "library" => {
                    app.invoke_query_changed("movie".into());
                    app.invoke_query_changed("".into());
                    app.invoke_reveal_row((cycle * 8) % 1000);
                    app.invoke_detail_selected(851);
                    app.invoke_detail_episode_toggle(cycle % 23);
                    app.invoke_detail_rate(cycle % 10 + 1);
                }
                "discover" => {
                    app.invoke_discover_query_changed(format!("soak {cycle}").into());
                    ui.advance(Duration::from_millis(300));
                    ui.pump_until("search", |app| app.get_discover_results().row_count() == 1);
                }
                "history" => app.invoke_history_opened(),
                "statistics" => app.invoke_statistics_opened(),
                "calendar" => app.invoke_calendar_opened(),
                "updates" => app.invoke_updates_opened(),
                _ => {}
            }
            ui.render();
            ui.pump();
            timings.push(serde_json::json!({"cycle":cycle,"page":page,"ms":started.elapsed().as_secs_f64()*1000.0}));
        }
        ui.pump_until("posters", |_| posters.idle());
        assert!(posters.ram_bytes() <= poster::RAM_BUDGET);
        assert_eq!(app.get_startup_error(), "");
        assert_eq!(app.get_media_detail().tracking_notice, "");
        if cycle % 10 == 0 {
            let path = dir.0.join(format!("backup-{cycle}.json"));
            db.with(|db| backup::export_to_path(db, &path, statistics::tests::NOW))
                .unwrap();
            let exported = backup::read_from_path(&path).unwrap();
            let restored = Database::open_in_memory();
            backup::restore(&restored, &exported).unwrap();
            assert_eq!(
                backup::export(&restored, statistics::tests::NOW).unwrap(),
                exported
            );
        }
        println!(
            "R17 cycle={cycle} cache_bytes={} requests={}",
            posters.ram_bytes(),
            server.seen().len()
        );
        std::thread::sleep(Duration::from_millis(100));
    }
    println!(
        "R17_PAGE_SAMPLES={}",
        serde_json::to_string(&timings).unwrap()
    );
    let before = db
        .with(|db| backup::export(db, statistics::tests::NOW))
        .unwrap();
    let reopened = Database::open(&paths.database(), &log).unwrap();
    assert_eq!(
        backup::export(&reopened, statistics::tests::NOW).unwrap(),
        before
    );
    assert!(
        server
            .seen()
            .iter()
            .all(|request| !request.target.contains("r17-fake-token"))
    );
    lock.borrow_mut().take();
}
