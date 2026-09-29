//! Opt-in, bounded metadata maintenance while the application is open.
//! Database reads/writes stay on the UI thread; the shared network pool only
//! performs HTTP. One maintenance request can be in flight at a time.

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rusqlite::params;
use slint::ComponentHandle;

use crate::AppWindow;
use crate::database::{Database, SharedDb};
use crate::diagnostics::Log;
use crate::error::AppError;
use crate::library::MediaType;
use crate::metadata::{self, Clock, Episode, MediaDetails};
use crate::network::Network;
use crate::secrets::SharedToken;
use crate::settings;
use crate::tmdb::{TmdbClient, TmdbError};

const CYCLE: Duration = Duration::from_secs(6 * 60 * 60);
const LIMIT: usize = 50;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Task {
    Detail {
        id: i64,
        media_type: MediaType,
        external: String,
    },
    Season {
        id: i64,
        external: String,
        number: i64,
    },
}

enum Answer {
    Detail(Box<MediaDetails>),
    Season(Vec<Episode>),
}

#[derive(Default)]
struct State {
    queue: VecDeque<Task>,
    seen: HashSet<Task>,
    active: bool,
    in_flight: bool,
    pause_until: i64,
    invalid_token: bool,
    considered: usize,
    refreshed: usize,
    failures: usize,
}

#[derive(Clone)]
struct Coordinator {
    window: slint::Weak<AppWindow>,
    db: SharedDb,
    token: SharedToken,
    client: TmdbClient,
    network: Network,
    clock: Clock,
    log: Arc<Log>,
    state: Arc<Mutex<State>>,
}

pub fn start(
    window: &AppWindow,
    db: SharedDb,
    token: SharedToken,
    client: TmdbClient,
    network: Network,
    clock: Clock,
    log: Arc<Log>,
) {
    let coordinator = Coordinator {
        window: window.as_weak(),
        db,
        token,
        client,
        network,
        clock,
        log,
        state: Arc::default(),
    };
    let enabled = coordinator
        .db
        .with(settings::automatic_refresh)
        .unwrap_or(false);
    window.set_automatic_refresh_enabled(enabled);
    window.on_automatic_refresh_toggle({
        let c = coordinator.clone();
        move || {
            let Some(window) = c.window.upgrade() else {
                return;
            };
            let enabled = !window.get_automatic_refresh_enabled();
            match c.db.with(|db| settings::set_automatic_refresh(db, enabled)) {
                Ok(()) => {
                    window.set_automatic_refresh_enabled(enabled);
                    if enabled {
                        c.begin(true);
                    }
                }
                Err(error) => {
                    c.log
                        .error(format_args!("Automatic refresh setting: {error}"));
                }
            }
        }
    });
    window.on_refresh_token_changed({
        let c = coordinator.clone();
        move || {
            let mut state = c.state.lock().unwrap();
            state.invalid_token = false;
            state.pause_until = 0;
            drop(state);
            c.begin(true);
        }
    });
    slint::Timer::single_shot(Duration::from_secs(2), move || coordinator.begin(true));
}

fn read_error(error: rusqlite::Error) -> AppError {
    AppError::database(
        "Automatic refresh could not inspect Library metadata.",
        error,
    )
}

/// Two statements, each capped at 50 entries. Never-fetched seasons are
/// intentionally excluded; opening them remains a user action.
fn due(db: &Database, now: i64) -> Result<VecDeque<Task>, AppError> {
    let mut tasks = VecDeque::new();
    let mut detail = db
        .conn()
        .prepare_cached(
            "SELECT m.local_media_id, m.media_type, x.external_id, m.details_fetched_at
         FROM library_entries AS l JOIN media AS m ON m.local_media_id = l.local_media_id
         JOIN external_refs AS x ON x.local_media_id = m.local_media_id
             AND x.media_type = m.media_type AND x.source = 'tmdb'
         ORDER BY m.details_fetched_at, m.local_media_id",
        )
        .map_err(read_error)?;
    let rows = detail
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<i64>>(3)?,
            ))
        })
        .map_err(read_error)?;
    for row in rows {
        let (id, kind, external, fetched) = row.map_err(read_error)?;
        if !metadata::freshness(fetched, now).wants_refresh() {
            continue;
        }
        let media_type = if kind == "tv" {
            MediaType::Tv
        } else {
            MediaType::Movie
        };
        tasks.push_back(Task::Detail {
            id,
            media_type,
            external,
        });
        if tasks.len() == LIMIT {
            break;
        }
    }
    let mut season = db
        .conn()
        .prepare_cached(
            "SELECT s.local_media_id, x.external_id, s.season_number,
                s.episodes_fetched_at, s.episodes_known, s.episode_count
         FROM library_entries AS l JOIN seasons AS s ON s.local_media_id = l.local_media_id
         JOIN external_refs AS x ON x.local_media_id = s.local_media_id
             AND x.media_type = 'tv' AND x.source = 'tmdb'
         WHERE s.episodes_fetched_at IS NOT NULL
         ORDER BY s.episodes_fetched_at, s.local_media_id, s.season_number",
        )
        .map_err(read_error)?;
    let rows = season
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, Option<i64>>(3)?,
                r.get::<_, Option<u32>>(4)?,
                r.get::<_, Option<u32>>(5)?,
            ))
        })
        .map_err(read_error)?;
    let mut seasons = 0;
    for row in rows {
        let (id, external, number, fetched, known, expected) = row.map_err(read_error)?;
        if !metadata::freshness(fetched, now).wants_refresh()
            && !matches!((known, expected), (Some(k), Some(e)) if k < e)
        {
            continue;
        }
        tasks.push_back(Task::Season {
            id,
            external,
            number,
        });
        seasons += 1;
        if seasons == LIMIT {
            break;
        }
    }
    Ok(tasks)
}

impl Coordinator {
    fn begin(&self, force: bool) {
        let now = self.clock.now();
        let mut state = self.state.lock().unwrap();
        if state.active || state.in_flight || state.invalid_token || now < state.pause_until {
            return;
        }
        if !force && self.token.get().is_none() {
            return;
        }
        if self.token.get().is_none() || !self.db.with(settings::automatic_refresh).unwrap_or(false)
        {
            return;
        }
        let queue = match self.db.with(|db| due(db, now)) {
            Ok(queue) => queue,
            Err(error) => {
                self.log.error(format_args!("Automatic refresh: {error}"));
                return;
            }
        };
        state.considered = queue.len();
        state.refreshed = 0;
        state.failures = 0;
        state.seen = queue.iter().cloned().collect();
        state.queue = queue;
        state.active = true;
        self.log.info(format_args!(
            "Automatic refresh cycle started: {} due requests",
            state.considered
        ));
        drop(state);
        self.next();
    }

    fn next(&self) {
        let task = {
            let mut state = self.state.lock().unwrap();
            if !state.active || state.in_flight {
                return;
            }
            match state.queue.pop_front() {
                Some(task) => {
                    state.in_flight = true;
                    task
                }
                None => {
                    state.active = false;
                    self.log.info(format_args!(
                        "Automatic refresh cycle: considered {}, refreshed {}, failures {}",
                        state.considered, state.refreshed, state.failures
                    ));
                    drop(state);
                    self.later(CYCLE);
                    return;
                }
            }
        };
        let Some(token) = self.token.get() else {
            let mut state = self.state.lock().unwrap();
            state.active = false;
            state.in_flight = false;
            state.queue.clear();
            return;
        };
        let c = self.clone();
        let client = self.client.clone();
        let network = self.network.clone();
        self.network.run(move || {
            let answer = match &task {
                Task::Detail {
                    media_type,
                    external,
                    ..
                } => client
                    .details(&token, *media_type, external)
                    .map(|d| Answer::Detail(Box::new(d))),
                Task::Season {
                    external, number, ..
                } => client
                    .season_episodes(&token, external, *number)
                    .map(Answer::Season),
            };
            network.post(move || c.arrived(task, answer));
        });
    }

    fn arrived(&self, task: Task, answer: Result<Answer, TmdbError>) {
        let now = self.clock.now();
        let id = match &task {
            Task::Detail { id, .. } | Task::Season { id, .. } => *id,
        };
        let still_member =
            self.db
                .with(|db| {
                    db.conn().query_row(
            "SELECT EXISTS (SELECT 1 FROM library_entries WHERE local_media_id = ?1)",
            [id], |r| r.get::<_, bool>(0)).map_err(read_error)
                })
                .unwrap_or(false);
        let result = if !still_member
            || self.token.get().is_none()
            || !self.db.with(settings::automatic_refresh).unwrap_or(false)
        {
            None
        } else {
            Some(answer)
        };
        match result {
            Some(Ok(Answer::Detail(details))) => {
                let Task::Detail { id, .. } = &task else {
                    unreachable!()
                };
                let id = *id;
                let changed = self.db.with(|db| {
                    let before = downloaded_counts(db, id)?;
                    metadata::save(db, id, &details, now)?;
                    let after = downloaded_counts(db, id)?;
                    Ok(after
                        .into_iter()
                        .filter(|(n, count)| {
                            before
                                .iter()
                                .any(|(old_n, old_count)| old_n == n && old_count != count)
                        })
                        .map(|(n, _)| n)
                        .collect::<Vec<_>>())
                });
                match changed {
                    Ok(seasons) => {
                        if let Task::Detail { external, .. } = &task {
                            let mut state = self.state.lock().unwrap();
                            for number in seasons {
                                let job = Task::Season {
                                    id,
                                    external: external.clone(),
                                    number,
                                };
                                if state.queue.len() < LIMIT * 3 && state.seen.insert(job.clone()) {
                                    state.queue.push_back(job);
                                }
                            }
                        }
                        self.success();
                    }
                    Err(error) => self.local_failure(error),
                }
            }
            Some(Ok(Answer::Season(episodes))) => {
                let Task::Season { id, number, .. } = task else {
                    unreachable!()
                };
                match self
                    .db
                    .with(|db| metadata::save_episodes(db, id, number, &episodes, now))
                {
                    Ok(()) => self.success(),
                    Err(error) => self.local_failure(error),
                }
            }
            Some(Err(error)) => {
                let delay = self.remote_failure(error, now);
                self.state.lock().unwrap().in_flight = false;
                self.later(delay);
                return;
            }
            None => {
                let mut state = self.state.lock().unwrap();
                state.active = false;
                state.in_flight = false;
                state.queue.clear();
                return;
            }
        }
        if let Some(window) = self.window.upgrade() {
            window.invoke_library_changed();
            window.invoke_metadata_changed();
            window.invoke_release_events_changed();
        }
        self.state.lock().unwrap().in_flight = false;
        self.next();
    }

    fn success(&self) {
        self.state.lock().unwrap().refreshed += 1;
    }
    fn local_failure(&self, error: AppError) {
        self.log
            .error(format_args!("Automatic refresh save: {error}"));
        self.state.lock().unwrap().failures += 1;
    }
    fn remote_failure(&self, error: TmdbError, now: i64) -> Duration {
        self.log
            .info(format_args!("Automatic refresh paused: {error}"));
        if matches!(error, TmdbError::CredentialInvalid)
            && let Some(window) = self.window.upgrade()
        {
            window.invoke_tmdb_check();
        }
        let mut state = self.state.lock().unwrap();
        state.failures += 1;
        state.queue.clear();
        state.active = false;
        state.invalid_token = matches!(error, TmdbError::CredentialInvalid);
        let delay = if matches!(error, TmdbError::RateLimited) {
            30 * 60
        } else {
            15 * 60
        };
        state.pause_until = now + delay;
        Duration::from_secs(delay as u64)
    }
    fn later(&self, delay: Duration) {
        let c = self.clone();
        slint::Timer::single_shot(delay, move || c.begin(false));
    }
}

fn downloaded_counts(db: &Database, id: i64) -> Result<Vec<(i64, Option<u32>)>, AppError> {
    let mut stmt = db
        .conn()
        .prepare_cached(
            "SELECT season_number, episode_count FROM seasons
        WHERE local_media_id = ?1 AND episodes_fetched_at IS NOT NULL ORDER BY season_number",
        )
        .map_err(read_error)?;
    stmt.query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?)))
        .map_err(read_error)?
        .collect::<rusqlite::Result<_>>()
        .map_err(read_error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::metadata::tests::{episode, season, series, stored};
    use crate::tests::Headless;
    use crate::tmdb::fake::{FakeServer, Reply, season_details, tv_details};
    use crate::tracking;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn selection_is_bounded_and_skips_fresh_and_removed_titles() {
        let db = Database::open_in_memory();
        for n in 1..=1_000 {
            let id = stored(&db, MediaType::Movie, n, &format!("Film {n}"));
            if n == 1 {
                db.conn()
                    .execute(
                        "UPDATE media SET details_fetched_at = 1000 WHERE local_media_id = ?1",
                        [id],
                    )
                    .unwrap();
            }
            if n == 2 {
                db.conn()
                    .execute(
                        "DELETE FROM library_entries WHERE local_media_id = ?1",
                        [id],
                    )
                    .unwrap();
            }
        }
        let work = due(&db, 1000).unwrap();
        assert_eq!(work.len(), LIMIT);
        assert!(work.iter().all(
            |t| !matches!(t, Task::Detail { external, .. } if external == "1" || external == "2")
        ));
    }

    #[test]
    fn disabled_mode_makes_no_request_and_setting_persists() {
        let ui = Headless::new(1280, 800);
        let db = SharedDb::default();
        db.set(Database::open_in_memory());
        db.with(|db| {
            stored(db, MediaType::Tv, 9, "Show");
            Ok(())
        })
        .unwrap();
        let server = FakeServer::start(|_| Reply::json(500, "{}"));
        let token = SharedToken::default();
        token.set(crate::secrets::Token::parse("fake-token"));
        let (clock, _) = Clock::fake(1_800_000_000);
        start(
            &ui.app,
            db.clone(),
            token,
            TmdbClient::for_tests(&server.base, Duration::from_secs(1)),
            Network::with_post(4, ui.post()),
            clock,
            Arc::new(Log::stderr_only()),
        );
        ui.advance(Duration::from_secs(2));
        ui.pump();
        assert!(server.seen().is_empty());
        ui.app.invoke_automatic_refresh_toggle();
        assert!(db.with(settings::automatic_refresh).unwrap());
    }

    #[test]
    fn refresh_finds_one_new_episode_and_preserves_tracking() {
        let ui = Headless::new(1280, 800);
        let db = SharedDb::default();
        db.set(Database::open_in_memory());
        let show = db
            .with(|db| {
                let id = stored(db, MediaType::Tv, 9, "Show");
                metadata::save(db, id, &series("Show", vec![season(1, Some(10))]), 1)?;
                let old: Vec<_> = (1..=10).map(|n| episode(1, n)).collect();
                metadata::save_episodes(db, id, 1, &old, 1)?;
                for n in 1..=10 {
                    tracking::watch_episode(db, id, 1, n, 2, false)?;
                }
                settings::set_automatic_refresh(db, true)?;
                Ok(id)
            })
            .unwrap();
        let running = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let active = running.clone();
        let maximum = peak.clone();
        let server = FakeServer::start(move |seen| {
            let concurrent = active.fetch_add(1, Ordering::SeqCst) + 1;
            maximum.fetch_max(concurrent, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(10));
            active.fetch_sub(1, Ordering::SeqCst);
            if seen.target.contains("/season/1") {
                Reply::json(200, &season_details(1, 11))
            } else {
                Reply::json(200, &tv_details(9, "Show", &[(1, 11)]))
            }
        });
        let token = SharedToken::default();
        token.set(crate::secrets::Token::parse("fake-token"));
        let (clock, _) = Clock::fake(1_800_000_000);
        start(
            &ui.app,
            db.clone(),
            token,
            TmdbClient::for_tests(&server.base, Duration::from_secs(1)),
            Network::with_post(4, ui.post()),
            clock,
            Arc::new(Log::stderr_only()),
        );
        ui.advance(Duration::from_secs(2));
        ui.pump_until("new release event", |_| {
            db.with(crate::release::unread_count).unwrap() == 1
        });
        assert_eq!(server.seen().len(), 2);
        assert_eq!(peak.load(Ordering::SeqCst), 1);
        db.with(|db| {
            let progress = tracking::series_progress(db, show)?;
            assert!(!progress.is_complete());
            assert_eq!(
                tracking::next_episode(db, show, &progress)?,
                tracking::NextEpisode::Episode {
                    season: 1,
                    number: 11,
                    name: Some("Episode 11".into())
                }
            );
            assert_eq!(
                crate::calendar::events(db, "2011-05-11", "2011-05-12")?.len(),
                1
            );
            assert_eq!(crate::release::recent(db, 10)?.len(), 1);
            assert_eq!(tracking::continue_watching(db)?, vec![show]);
            assert!(
                crate::home::load(db, 1_800_000_000)?
                    .entries
                    .iter()
                    .any(|e| e.section == "Up Next" && e.episode == Some(11))
            );
            Ok(())
        })
        .unwrap();
        ui.app.invoke_refresh_token_changed();
        assert_eq!(db.with(crate::release::unread_count).unwrap(), 1);
    }

    #[test]
    fn offline_maintenance_keeps_local_data_and_pauses() {
        let ui = Headless::new(1280, 800);
        let db = SharedDb::default();
        db.set(Database::open_in_memory());
        db.with(|db| {
            stored(db, MediaType::Movie, 9, "Offline film");
            settings::set_automatic_refresh(db, true)
        })
        .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        drop(listener);
        let token = SharedToken::default();
        token.set(crate::secrets::Token::parse("fake-token"));
        let (clock, _) = Clock::fake(1_800_000_000);
        let c = Coordinator {
            window: ui.app.as_weak(),
            db: db.clone(),
            token,
            client: TmdbClient::for_tests(&base, Duration::from_secs(1)),
            network: Network::with_post(4, ui.post()),
            clock,
            log: Arc::new(Log::stderr_only()),
            state: Arc::default(),
        };
        c.begin(true);
        ui.pump_until("offline pause", |_| {
            c.state.lock().unwrap().pause_until > 1_800_000_000
        });
        assert_eq!(db.with(crate::library::count).unwrap(), 1);
        assert_eq!(
            db.with(|db| crate::home::load(db, 1_800_000_000))
                .unwrap()
                .summary,
            "1 movies · 0 series in Library"
        );
    }

    #[test]
    fn rate_limit_server_failure_and_bad_token_back_off_without_retry_storm() {
        let ui = Headless::new(1280, 800);
        let db = SharedDb::default();
        db.set(Database::open_in_memory());
        db.with(|db| {
            stored(db, MediaType::Movie, 9, "Film");
            settings::set_automatic_refresh(db, true)
        })
        .unwrap();
        let count = Arc::new(AtomicUsize::new(0));
        let calls = count.clone();
        let server = FakeServer::start(move |_| match calls.fetch_add(1, Ordering::SeqCst) {
            0 => Reply::json(429, "{}"),
            1 => Reply::json(500, "{}"),
            _ => Reply::json(401, "{}"),
        });
        let token = SharedToken::default();
        token.set(crate::secrets::Token::parse("fake-token"));
        let (clock, seconds) = Clock::fake(1_800_000_000);
        let c = Coordinator {
            window: ui.app.as_weak(),
            db,
            token,
            client: TmdbClient::for_tests(&server.base, Duration::from_secs(1)),
            network: Network::with_post(4, ui.post()),
            clock,
            log: Arc::new(Log::stderr_only()),
            state: Arc::default(),
        };
        c.begin(true);
        ui.pump_until("429", |_| {
            server.seen().len() == 1 && !c.state.lock().unwrap().in_flight
        });
        assert_eq!(c.state.lock().unwrap().pause_until, 1_800_001_800);
        c.begin(true);
        assert_eq!(server.seen().len(), 1, "no immediate retry");
        seconds.store(1_800_001_801, Ordering::SeqCst);
        c.begin(true);
        ui.pump_until("500", |_| {
            server.seen().len() == 2 && !c.state.lock().unwrap().in_flight
        });
        assert_eq!(c.state.lock().unwrap().pause_until, 1_800_002_701);
        seconds.store(1_800_002_702, Ordering::SeqCst);
        c.begin(true);
        ui.pump_until("401", |_| {
            server.seen().len() == 3 && !c.state.lock().unwrap().in_flight
        });
        assert!(c.state.lock().unwrap().invalid_token);
        seconds.store(1_800_100_000, Ordering::SeqCst);
        c.begin(true);
        assert_eq!(
            server.seen().len(),
            3,
            "invalid credential halts maintenance"
        );
    }
}
