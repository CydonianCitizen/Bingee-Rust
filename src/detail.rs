//! The Library detail pane, cache first (ADR-0013).
//!
//! Opening a title reads SQLite and renders at once. Only then does the
//! freshness policy decide whether TMDB is asked, on the worker pool. A
//! successful answer is committed to SQLite and the pane is rendered again
//! from SQLite; a failure leaves every cached value on screen and says what
//! went wrong beside it. Nothing here deletes a title or a library entry.
//!
//! Series detail stores season summaries only. A season's episodes are
//! fetched when that season is shown (the first regular season is shown by
//! default) and only if they were never fetched or are stale, so opening a
//! twenty-season series costs at most two requests (ADR-0014).
//!
//! Requests wait until the selection has stayed on a title or season for
//! `SETTLE`: moving through the library with the keyboard renders every
//! cached detail it passes but asks TMDB only where it stops.
//!
//! Stale-result protection: a new title starts a new title generation, a new
//! title or season a new season generation. An answer is always saved under
//! the ids it was requested for — it is correct data for that title — but it
//! reaches the pane only while its generation is still current.
//!
//! Personal tracking (ADR-0016, ADR-0017) is a separate path: watched state,
//! ratings and season bulk actions are written to SQLite synchronously on the
//! UI thread (one small transaction each, ADR-0019), never touch the network,
//! and the pane then renders what SQLite committed. A failed write leaves the
//! committed state on screen with a notice.

use std::collections::HashSet;
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use slint::{ComponentHandle, ModelRc, SharedString, Timer, VecModel};

use crate::database::{Database, SharedDb};
use crate::diagnostics::Log;
use crate::error::AppError;
use crate::library::{self, MediaType};
use crate::metadata::{self, Clock, Coverage, Episode, Freshness, MediaDetails, Season};
use crate::network::Network;
use crate::poster::{PosterKey, Posters};
use crate::search::ExternalRef;
use crate::secrets::SharedToken;
use crate::tmdb::{TmdbClient, TmdbError};
use crate::tracking::{self, NextEpisode, SeriesProgress, TitleState};
use crate::view::tint;
use crate::{AppWindow, DetailFact, EpisodeRow, MediaDetail, SeasonPanel, SeasonRow};

/// How long a selection must stay put before its requests start.
pub const SETTLE: Duration = crate::search::DEBOUNCE;

/// What the shown season's episode request is doing.
#[derive(Debug, Clone, PartialEq, Default)]
enum Episodes {
    #[default]
    Idle,
    Loading,
    /// The last request failed, with the text to show.
    Failed(String),
}

#[derive(Default)]
struct State {
    /// The local id of the title on screen.
    id: Option<i64>,
    title_generation: u64,
    season_generation: u64,
    /// The title as SQLite has it, or `None` if it could not be read.
    details: Option<MediaDetails>,
    /// Why the title could not be read from SQLite.
    read_error: String,
    /// The season number on screen.
    season: Option<i64>,
    /// Its stored episodes.
    episodes: Vec<Episode>,
    episodes_request: Episodes,
    episode_row: Option<usize>,
    details_pending: bool,
    /// Why the last detail refresh failed. Cleared by the next success.
    notice: String,
    /// The title's committed personal state.
    tracking: TitleState,
    /// A movie's "Watched 2026-09-16 21:04" / "Not watched".
    watched_label: String,
    /// A series' progress and next episode.
    progress: SeriesProgress,
    next: Option<NextEpisode>,
    /// Watched episode numbers of the shown season.
    watched: HashSet<i64>,
    /// Why the last personal change or read failed.
    tracking_notice: String,
}

/// The ids an answer belongs to.
#[derive(Debug, Clone, Copy)]
struct Request {
    generation: u64,
    id: i64,
    season: i64,
}

#[derive(Clone)]
pub struct Detail {
    window: slint::Weak<AppWindow>,
    state: Arc<Mutex<State>>,
    db: SharedDb,
    client: TmdbClient,
    token: SharedToken,
    network: Network,
    posters: Arc<Posters>,
    clock: Clock,
    log: Arc<Log>,
}

/// Wires the Library detail pane to `window`. The token is read from `token`
/// whenever a request is due; `remote` announces changes through
/// `token-changed`.
#[allow(clippy::too_many_arguments)] // the pane's collaborators, each injected
pub fn start(
    window: &AppWindow,
    db: SharedDb,
    client: TmdbClient,
    token: SharedToken,
    network: Network,
    posters: Arc<Posters>,
    clock: Clock,
    log: Arc<Log>,
) {
    let detail = Detail {
        window: window.as_weak(),
        state: Arc::default(),
        db,
        client,
        token,
        network,
        posters,
        clock,
        log,
    };
    let on = |f: fn(&Detail)| {
        let detail = detail.clone();
        move || f(&detail)
    };
    let on_row = |f: fn(&Detail, i32)| {
        let detail = detail.clone();
        move |row| f(&detail, row)
    };
    window.on_detail_selected(on_row(Detail::select));
    window.on_detail_season_selected(on_row(Detail::select_season));
    window.on_detail_episode_selected(on_row(Detail::select_episode));
    window.on_detail_refresh(on(Detail::refresh));
    window.on_detail_retry_season(on(|d| d.fetch_episodes(true)));
    window.on_detail_poster_ready(on(Detail::render));
    window.on_detail_watch_toggle(on(Detail::watch_toggle));
    window.on_detail_watch_again(on(|d| {
        d.track("watch again", |db, id, now| {
            tracking::watch_movie(db, id, now, true).map(drop)
        })
    }));
    window.on_detail_rate(on_row(Detail::rate));
    window.on_detail_episode_toggle(on_row(Detail::episode_toggle));
    window.on_detail_season_watched({
        let detail = detail.clone();
        move |watched| detail.season_watched(watched)
    });
    window.on_token_changed(on(|d| {
        d.fetch_details(false);
        d.fetch_episodes(false);
        d.render();
    }));
    // The library may have selected a title before this was connected.
    detail.select(window.get_selected_id());
}

impl Detail {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs `work` on a network worker, then `done` on the UI thread.
    fn spawn<T: Send + 'static>(
        &self,
        work: impl FnOnce(&TmdbClient) -> T + Send + 'static,
        done: impl FnOnce(&Detail, T, Duration) + Send + 'static,
    ) {
        let detail = self.clone();
        self.network.run(move || {
            let started = Instant::now();
            let value = work(&detail.client);
            let took = started.elapsed();
            let network = detail.network.clone();
            network.post(move || done(&detail, value, took));
        });
    }

    // Selection

    /// The library selected another title (`-1`: none). The cached detail is
    /// on screen before anything else happens.
    fn select(&self, id: i32) {
        let id = u32::try_from(id).ok().map(i64::from);
        let mut state = self.state();
        if state.id == id {
            return;
        }
        let generations = (state.title_generation + 1, state.season_generation + 1);
        *state = State {
            id,
            title_generation: generations.0,
            season_generation: generations.1,
            ..State::default()
        };
        self.load_details(&mut state);
        self.load_episodes(&mut state);
        drop(state);
        self.render();
        self.fetch_when_settled();
    }

    /// Applies the refresh policy once the selection has not changed for
    /// `SETTLE`. Every title or season change bumps the season generation, so
    /// a timer from an earlier selection does nothing.
    fn fetch_when_settled(&self) {
        let generation = self.state().season_generation;
        let detail = self.clone();
        Timer::single_shot(SETTLE, move || {
            let state = detail.state();
            if state.season_generation != generation {
                return;
            }
            if let (Some(id), Some(details)) = (state.id, &state.details) {
                let freshness = metadata::freshness(details.fetched_at, detail.clock.now());
                detail.log.info(format_args!(
                    "Detail #{id}: shown from the local cache ({})",
                    describe(freshness)
                ));
            }
            drop(state);
            detail.fetch_details(false);
            detail.fetch_episodes(false);
            detail.render();
        });
    }

    fn select_season(&self, row: i32) {
        let mut state = self.state();
        let number = usize::try_from(row)
            .ok()
            .and_then(|row| Some(seasons(&state.details).get(row)?.number));
        if number.is_none() || number == state.season {
            return;
        }
        state.season = number;
        state.season_generation += 1;
        state.episodes_request = Episodes::Idle;
        self.load_episodes(&mut state);
        drop(state);
        self.render();
        self.fetch_when_settled();
    }

    fn select_episode(&self, row: i32) {
        let mut state = self.state();
        let row = usize::try_from(row)
            .ok()
            .filter(|r| *r < state.episodes.len());
        if row.is_some() {
            state.episode_row = row;
        }
        drop(state);
        self.render();
    }

    /// Refresh: asks TMDB for the title and the shown season whatever their
    /// age, keeping everything cached on screen meanwhile.
    fn refresh(&self) {
        self.fetch_details(true);
        self.fetch_episodes(true);
        self.render();
    }

    // SQLite

    /// Reads the title into `state` and keeps a valid season selected.
    fn load_details(&self, state: &mut State) {
        let Some(id) = state.id else {
            return;
        };
        match self.db.with(|db| metadata::read(db, id)) {
            Ok(details) => {
                state.details = details;
                state.read_error.clear();
            }
            Err(error) => {
                self.log.error(format_args!("Detail #{id}: {error}"));
                state.read_error = error.message;
            }
        }
        let list = seasons(&state.details);
        if !list
            .iter()
            .any(|season| Some(season.number) == state.season)
        {
            state.season = default_season(list);
            state.season_generation += 1;
            state.episodes_request = Episodes::Idle;
        }
    }

    /// Reads the shown season's stored episodes into `state`.
    fn load_episodes(&self, state: &mut State) {
        let episodes = match (state.id, state.season) {
            (Some(id), Some(season)) => self
                .db
                .with(|db| metadata::read_episodes(db, id, season))
                .unwrap_or_else(|error| {
                    self.log
                        .error(format_args!("Detail #{id} season {season}: {error}"));
                    Vec::new()
                }),
            _ => Vec::new(),
        };
        if episodes.len() != state.episodes.len() {
            state.episode_row = None;
        }
        state.episodes = episodes;
        self.load_tracking(state);
    }

    /// Reads the title's personal state (and a series' progress and the shown
    /// season's watched episodes) into `state`. Local only.
    fn load_tracking(&self, state: &mut State) {
        let Some(id) = state.id else {
            return;
        };
        let is_tv = state.details.as_ref().is_some_and(|d| d.tv.is_some());
        let season = state.season;
        let read = self.db.with(|db| {
            let title = tracking::title_state(db, id)?;
            if !is_tv {
                let label = match title.watched_at {
                    Some(at) => format!("Watched {}", tracking::local_time(db, at)?),
                    None => "Not watched".to_owned(),
                };
                return Ok((
                    title,
                    label,
                    SeriesProgress::default(),
                    None,
                    HashSet::new(),
                ));
            }
            let progress = tracking::series_progress(db, id)?;
            let next = tracking::next_episode(db, id, &progress)?;
            let watched = match season {
                Some(season) => tracking::watched_episodes(db, id, season)?,
                None => HashSet::new(),
            };
            Ok((title, String::new(), progress, Some(next), watched))
        });
        match read {
            Ok((title, label, progress, next, watched)) => {
                state.tracking = title;
                state.watched_label = label;
                state.progress = progress;
                state.next = next;
                state.watched = watched;
            }
            Err(error) => {
                self.log
                    .error(format_args!("Tracking #{id}: not read: {error}"));
                state.tracking_notice = error.message;
            }
        }
    }

    // Personal tracking

    /// Runs one personal write for the shown title, then renders what SQLite
    /// holds. Only failures are logged: watch activity stays in the database.
    fn track<T>(&self, what: &str, write: impl FnOnce(&Database, i64, i64) -> Result<T, AppError>) {
        let Some(id) = self.state().id else {
            return;
        };
        let now = self.clock.now();
        let result = self.db.with(|db| write(db, id, now));
        let mut state = self.state();
        let saved = match result {
            Ok(_) => {
                state.tracking_notice.clear();
                true
            }
            Err(error) => {
                self.log
                    .error(format_args!("Tracking #{id}: {what} not saved: {error}"));
                state.tracking_notice = error.message;
                false
            }
        };
        self.load_tracking(&mut state);
        drop(state);
        self.render();
        if saved {
            self.library_changed();
        }
    }

    /// The Library list re-reads its rows' personal status.
    fn library_changed(&self) {
        if let Some(window) = self.window.upgrade() {
            window.invoke_library_changed();
        }
    }

    fn watch_toggle(&self) {
        let watched = self.state().tracking.watched_at.is_some();
        self.track("watched", move |db, id, now| match watched {
            true => tracking::unwatch_movie(db, id).map(drop),
            false => tracking::watch_movie(db, id, now, false).map(drop),
        });
    }

    /// `index` in the rating menu: 0 clears, 1 to 10 rate.
    fn rate(&self, index: i32) {
        let rating = u8::try_from(index).ok().filter(|rating| *rating > 0);
        if rating == self.state().tracking.rating {
            return;
        }
        self.track("rating", move |db, id, _| {
            tracking::set_rating(db, id, rating)
        });
    }

    fn episode_toggle(&self, row: i32) {
        let state = self.state();
        let Some(episode) = usize::try_from(row)
            .ok()
            .and_then(|row| state.episodes.get(row))
        else {
            return;
        };
        let (season, number) = (episode.season_number, episode.number);
        let watched = state.watched.contains(&number);
        drop(state);
        self.track("episode", move |db, id, now| match watched {
            true => tracking::unwatch_episode(db, id, season, number).map(drop),
            false => tracking::watch_episode(db, id, season, number, now, false).map(drop),
        });
    }

    fn season_watched(&self, watched: bool) {
        let Some(season) = self.state().season else {
            return;
        };
        self.track("season", move |db, id, now| match watched {
            true => tracking::watch_season(db, id, season, now).map(drop),
            false => tracking::unwatch_season(db, id, season).map(drop),
        });
    }

    fn external(&self, id: i64) -> Option<ExternalRef> {
        self.db
            .with(|db| library::external_ref(db, id))
            .unwrap_or_else(|error| {
                self.log.error(format_args!("Detail #{id}: {error}"));
                None
            })
    }

    // TMDB

    /// Starts a detail request if the policy wants one (never fetched, or
    /// stale) or `force` is set, a token exists and none is on its way.
    fn fetch_details(&self, force: bool) {
        let mut state = self.state();
        let (Some(id), Some(details)) = (state.id, &state.details) else {
            return;
        };
        let wanted =
            force || metadata::freshness(details.fetched_at, self.clock.now()).wants_refresh();
        if !wanted || state.details_pending {
            return;
        }
        let (Some(token), Some(external)) = (self.token.get(), self.external(id)) else {
            return;
        };
        state.details_pending = true;
        let request = Request {
            generation: state.title_generation,
            id,
            season: -1,
        };
        drop(state);
        self.spawn(
            move |client| {
                let details = client.details(&token, external.media_type, &external.id);
                (external, details)
            },
            move |detail, (external, result), took| {
                detail.details_arrived(request, &external, result, took)
            },
        );
    }

    fn details_arrived(
        &self,
        request: Request,
        external: &ExternalRef,
        result: Result<MediaDetails, TmdbError>,
        took: Duration,
    ) {
        let now = self.clock.now();
        let name = identity(request.id, external);
        // Saved whichever title is on screen now: it is that title's data.
        let failure = match result {
            Ok(details) => match self
                .db
                .with(|db| metadata::save(db, request.id, &details, now))
            {
                Ok(()) => {
                    self.log.info(format_args!(
                        "Detail {name}: refreshed in {} ms",
                        took.as_millis()
                    ));
                    // Coverage may have changed a Library row's progress.
                    self.library_changed();
                    None
                }
                Err(error) => {
                    self.log
                        .error(format_args!("Detail {name}: not saved: {error}"));
                    Some(error.message)
                }
            },
            Err(error) => {
                self.log.info(format_args!(
                    "Detail {name}: refresh failed in {} ms: {error}",
                    took.as_millis()
                ));
                Some(refresh_notice(&error))
            }
        };
        let mut state = self.state();
        if state.title_generation != request.generation {
            self.log.info(format_args!(
                "Detail {name}: another title is shown, not displayed"
            ));
            return;
        }
        state.details_pending = false;
        let saved = failure.is_none();
        state.notice = failure.unwrap_or_default();
        if saved {
            self.load_details(&mut state);
            self.load_episodes(&mut state);
        }
        drop(state);
        if saved {
            // A first fetch brings the season list: open the shown season.
            self.fetch_episodes(false);
        }
        self.render();
    }

    /// Starts an episode request for the shown season if its episodes were
    /// never fetched, are stale, or `force` is set.
    fn fetch_episodes(&self, force: bool) {
        let mut state = self.state();
        let (Some(id), Some(number)) = (state.id, state.season) else {
            return;
        };
        let Some(season) = seasons(&state.details).iter().find(|s| s.number == number) else {
            return;
        };
        let freshness = metadata::freshness(season.episodes_fetched_at, self.clock.now());
        if !(force || freshness.wants_refresh()) || state.episodes_request == Episodes::Loading {
            return;
        }
        let (Some(token), Some(external)) = (self.token.get(), self.external(id)) else {
            return;
        };
        if external.media_type != MediaType::Tv {
            return;
        }
        state.episodes_request = Episodes::Loading;
        let request = Request {
            generation: state.season_generation,
            id,
            season: number,
        };
        drop(state);
        self.log.info(format_args!(
            "Detail {} season {number}: episodes {}, fetching",
            identity(id, &external),
            describe(freshness)
        ));
        self.spawn(
            move |client| {
                let episodes = client.season_episodes(&token, &external.id, request.season);
                (external, episodes)
            },
            move |detail, (external, result), took| {
                detail.episodes_arrived(request, &external, result, took)
            },
        );
    }

    fn episodes_arrived(
        &self,
        request: Request,
        external: &ExternalRef,
        result: Result<Vec<Episode>, TmdbError>,
        took: Duration,
    ) {
        let now = self.clock.now();
        let name = format!(
            "{} season {}",
            identity(request.id, external),
            request.season
        );
        let failure = match result {
            Ok(episodes) => {
                let saved = self.db.with(|db| {
                    metadata::save_episodes(db, request.id, request.season, &episodes, now)
                });
                match saved {
                    Ok(()) => {
                        self.log.info(format_args!(
                            "Detail {name}: {} episodes saved in {} ms",
                            episodes.len(),
                            took.as_millis()
                        ));
                        self.library_changed();
                        None
                    }
                    Err(error) => {
                        self.log
                            .error(format_args!("Detail {name}: not saved: {error}"));
                        Some(error.message)
                    }
                }
            }
            Err(error) => {
                self.log.info(format_args!(
                    "Detail {name}: episodes failed in {} ms: {error}",
                    took.as_millis()
                ));
                Some(season_notice(&error))
            }
        };
        let mut state = self.state();
        if state.season_generation != request.generation {
            self.log.info(format_args!(
                "Detail {name}: another season is shown, not displayed"
            ));
            return;
        }
        state.episodes_request = match failure {
            None => Episodes::Idle,
            Some(message) => Episodes::Failed(message),
        };
        // Coverage lives on the season row: read it again with the episodes.
        self.load_details(&mut state);
        self.load_episodes(&mut state);
        drop(state);
        self.render();
    }

    // Rendering

    pub fn render(&self) {
        let Some(window) = self.window.upgrade() else {
            return;
        };
        let can_refresh = self.token.get().is_some();
        let now = self.clock.now();
        let state = self.state();
        let detail = self.media_detail(&state, can_refresh, now);
        let facts = state.details.as_ref().map(facts).unwrap_or_default();
        let list = seasons(&state.details);
        let season_row = list
            .iter()
            .position(|season| Some(season.number) == state.season);
        let season_rows: Vec<SeasonRow> = list
            .iter()
            .map(|season| season_row_view(season, state.progress.season(season.number)))
            .collect();
        let panel = season_panel(&state, season_row.map(|row| &list[row]), can_refresh);
        let episodes: Vec<EpisodeRow> = state
            .episodes
            .iter()
            .map(|episode| episode_row_view(episode, state.watched.contains(&episode.number)))
            .collect();
        let episode_row = state.episode_row;
        let rating = state.tracking.rating.map_or(0, i32::from);
        // Slint may read the models while properties are set: no lock held.
        drop(state);
        window.set_media_detail(detail);
        window.set_detail_facts(model(facts));
        window.set_detail_seasons(model(season_rows));
        window.set_detail_season_row(season_row.map_or(-1, |row| row as i32));
        window.set_detail_season(panel);
        window.set_detail_episodes(model(episodes));
        window.set_detail_episode_row(episode_row.map_or(-1, |row| row as i32));
        // Always the committed rating, also after the menu changed and the
        // write failed.
        window.set_detail_rating(rating);
    }

    fn media_detail(&self, state: &State, can_refresh: bool, now: i64) -> MediaDetail {
        let Some(details) = &state.details else {
            return match state.read_error.is_empty() {
                true => MediaDetail::default(),
                false => MediaDetail {
                    has_item: true,
                    title: "Details unavailable".into(),
                    notice: state.read_error.as_str().into(),
                    ..Default::default()
                },
            };
        };
        let id = state.id.unwrap_or_default();
        let key = details.poster_path.as_deref().and_then(PosterKey::tmdb);
        let refreshing = state.details_pending || state.episodes_request == Episodes::Loading;
        let (progress, progress_fraction, progress_detail, specials) =
            progress_view(&state.progress, state.next.as_ref());
        MediaDetail {
            watched: state.tracking.watched_at.is_some(),
            watched_label: state.watched_label.as_str().into(),
            progress: progress.into(),
            progress_fraction,
            progress_detail: progress_detail.into(),
            specials: specials.into(),
            tracking_notice: state.tracking_notice.as_str().into(),
            has_item: true,
            title: details.title.as_str().into(),
            original_title: details
                .original_title
                .as_deref()
                .filter(|original| *original != details.title)
                .unwrap_or_default()
                .into(),
            meta: match details.year() {
                Some(year) => format!("{} · {year}", details.media_type.label()),
                None => details.media_type.label().to_owned(),
            }
            .into(),
            tagline: details.tagline.as_deref().unwrap_or_default().into(),
            overview: details.overview.as_deref().unwrap_or_default().into(),
            initials: library::initials(&details.title).into(),
            tint: tint(id),
            poster: key
                .as_ref()
                .and_then(|key| self.posters.image(key))
                .unwrap_or_default(),
            poster_key: key.as_ref().map(PosterKey::name).unwrap_or_default().into(),
            is_tv: details.tv.is_some(),
            freshness: updated(details.fetched_at, now).into(),
            can_refresh,
            refreshing,
            notice: state.notice.as_str().into(),
        }
    }
}

fn model<T: Clone + 'static>(rows: Vec<T>) -> ModelRc<T> {
    ModelRc::from(Rc::new(VecModel::from(rows)))
}

fn seasons(details: &Option<MediaDetails>) -> &[Season] {
    details
        .as_ref()
        .and_then(|details| details.tv.as_ref())
        .map_or(&[], |tv| tv.seasons.as_slice())
}

/// The first regular season, or specials when a series has nothing else.
/// Specials stay in the list either way.
fn default_season(seasons: &[Season]) -> Option<i64> {
    let numbers = || seasons.iter().map(|season| season.number);
    numbers().find(|n| *n >= 1).or_else(|| numbers().next())
}

fn identity(id: i64, external: &ExternalRef) -> String {
    format!(
        "{}/{}/{} (#{id})",
        external.source.key(),
        external.media_type.key(),
        external.id
    )
}

fn describe(freshness: Freshness) -> &'static str {
    match freshness {
        Freshness::Never => "never fetched",
        Freshness::Fresh => "fresh, no request",
        Freshness::Stale => "stale",
    }
}

/// A refresh failure next to cached details. The cache is never cleared, so
/// every message says what is still shown.
fn refresh_notice(error: &TmdbError) -> String {
    match error {
        TmdbError::NotFound => "TMDB no longer has this title. It stays in your library with the \
                               details saved on this computer."
            .into(),
        TmdbError::CredentialInvalid => "TMDB rejected your access token, so nothing was \
                                        refreshed. Check it in Settings."
            .into(),
        TmdbError::Offline(_) | TmdbError::Timeout => {
            "Can't reach TMDB. Showing the details saved on this computer.".into()
        }
        other => format!("Not refreshed. {}", other.user_message()),
    }
}

fn season_notice(error: &TmdbError) -> String {
    match error {
        TmdbError::Offline(_) | TmdbError::Timeout => {
            "Can't reach TMDB. Check your internet connection, then try again.".into()
        }
        TmdbError::NotFound => "TMDB does not list episodes for this season.".into(),
        other => other.user_message().into(),
    }
}

fn updated(fetched_at: Option<i64>, now: i64) -> String {
    let Some(at) = fetched_at else {
        return "Details not downloaded yet".into();
    };
    match now.saturating_sub(at).max(0) / 86_400 {
        0 => "Updated today".into(),
        1 => "Updated yesterday".into(),
        days => format!("Updated {days} days ago"),
    }
}

/// "57 min", "2 h 16 min".
fn duration(minutes: u32) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

fn plural(n: u32, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

/// The label/value rows. A value the provider did not give has no row.
fn facts(details: &MediaDetails) -> Vec<DetailFact> {
    let fact = |label: &str, value: Option<String>| {
        value.map(|value| DetailFact {
            label: label.into(),
            value: value.into(),
        })
    };
    let genres = Some(details.genre_line()).filter(|line| !line.is_empty());
    let runtime = details.runtime_minutes.map(duration);
    let rows = match &details.tv {
        None => vec![
            fact("Released", details.release_date.clone()),
            fact("Runtime", runtime),
            fact("Status", details.status.clone()),
            fact("Genres", genres),
        ],
        Some(tv) => vec![
            fact("First aired", details.release_date.clone()),
            fact("Last aired", tv.last_air_date.clone()),
            fact("Status", details.status.clone()),
            fact("Seasons", tv.season_count.map(|n| n.to_string())),
            fact("Episodes", tv.episode_count.map(|n| n.to_string())),
            fact("Episode length", runtime),
            fact("Genres", genres),
        ],
    };
    rows.into_iter().flatten().collect()
}

/// A series' personal progress: the main line, the watched share of
/// downloaded episodes, the next episode, and specials. Only complete
/// coverage gets a percentage (ADR-0018).
fn progress_view(
    progress: &SeriesProgress,
    next: Option<&NextEpisode>,
) -> (String, f32, String, String) {
    let (watched, known) = (progress.watched(), progress.known());
    let main = match (known, progress.coverage_complete()) {
        (0, _) => "No episodes downloaded yet".to_owned(),
        (_, true) => format!(
            "{watched} / {known} episodes watched · {}%",
            watched * 100 / known
        ),
        (_, false) => {
            format!("{watched} / {known} downloaded episodes watched · episode list incomplete")
        }
    };
    let fraction = match known {
        0 => 0.0,
        _ => watched as f32 / known as f32,
    };
    let detail = match next {
        Some(NextEpisode::Episode {
            season,
            number,
            name,
        }) => match name {
            Some(name) => format!("Next: S{season} E{number} · {name}"),
            None => format!("Next: S{season} E{number}"),
        },
        Some(NextEpisode::Complete) => "Series watched".to_owned(),
        Some(NextEpisode::CaughtUp) => "All downloaded episodes watched; more may exist".to_owned(),
        Some(NextEpisode::NothingKnown) | None => String::new(),
    };
    let specials = progress
        .specials()
        .map(|(watched, known)| format!("{watched} / {known} specials watched"))
        .unwrap_or_default();
    (main, fraction, detail, specials)
}

fn season_row_view(season: &Season, progress: Option<&tracking::SeasonProgress>) -> SeasonRow {
    let meta: Vec<String> = [
        season.year().map(str::to_owned),
        season.episode_count.map(|n| plural(n, "episode")),
        progress
            .filter(|progress| progress.watched > 0)
            .map(|progress| format!("{} / {} watched", progress.watched, progress.known)),
    ]
    .into_iter()
    .flatten()
    .collect();
    SeasonRow {
        number: i32::try_from(season.number).unwrap_or(-1),
        title: season.label().into(),
        meta: meta.join(" · ").into(),
        coverage: match season.coverage() {
            Coverage::NotFetched => "Not downloaded".into(),
            Coverage::Partial { known, expected } => format!("{known} of {expected} saved").into(),
            Coverage::Complete(_) => SharedString::new(),
        },
    }
}

fn episode_row_view(episode: &Episode, watched: bool) -> EpisodeRow {
    let meta: Vec<String> = [
        episode.air_date.clone(),
        episode.runtime_minutes.map(duration),
    ]
    .into_iter()
    .flatten()
    .collect();
    EpisodeRow {
        label: format!("E{}", episode.number).into(),
        title: match &episode.name {
            Some(name) => name.as_str().into(),
            None => format!("Episode {}", episode.number).into(),
        },
        meta: meta.join(" · ").into(),
        overview: episode.overview.as_deref().unwrap_or_default().into(),
        watched,
    }
}

/// The shown season's header and the state of its episode list: loading,
/// failed with nothing cached (retryable), not downloaded without a token,
/// fetched and empty, or the list — with a notice when a refresh of cached
/// episodes failed.
fn season_panel(state: &State, season: Option<&Season>, can_refresh: bool) -> SeasonPanel {
    let Some(season) = season else {
        return SeasonPanel {
            state: "message".into(),
            title: "No seasons yet".into(),
            message: match can_refresh {
                true => "Seasons appear once the details are downloaded.",
                false => "Connect TMDB in Settings to download this series' seasons.",
            }
            .into(),
            ..Default::default()
        };
    };
    let meta: Vec<String> = [
        season.air_date.as_ref().map(|date| format!("Aired {date}")),
        season.episode_count.map(|n| plural(n, "episode")),
        match season.coverage() {
            Coverage::Partial { known, .. } => Some(format!("{known} saved")),
            _ => None,
        },
    ]
    .into_iter()
    .flatten()
    .collect();
    let header = SeasonPanel {
        heading: season.label().into(),
        meta: meta.join(" · ").into(),
        overview: season.overview.as_deref().unwrap_or_default().into(),
        ..Default::default()
    };
    if !state.episodes.is_empty() {
        let known = state.episodes.len();
        let watched = state
            .episodes
            .iter()
            .filter(|episode| state.watched.contains(&episode.number))
            .count();
        return SeasonPanel {
            state: "episodes".into(),
            progress: format!("{watched} / {known} watched").into(),
            can_watch_all: watched < known,
            can_unwatch_all: watched > 0,
            notice: match &state.episodes_request {
                Episodes::Failed(message) => format!("Showing saved episodes. {message}").into(),
                _ => SharedString::new(),
            },
            ..header
        };
    }
    let (title, message, action) = match (&state.episodes_request, season.coverage()) {
        (Episodes::Loading, _) => ("Loading episodes…", String::new(), ""),
        (Episodes::Failed(message), _) => ("Episodes not available", message.clone(), "retry"),
        (Episodes::Idle, Coverage::NotFetched) if !can_refresh => (
            "Episodes not downloaded",
            "Connect TMDB in Settings to download this season's episodes.".into(),
            "",
        ),
        (Episodes::Idle, Coverage::NotFetched) => {
            ("Episodes not downloaded", String::new(), "retry")
        }
        (Episodes::Idle, _) => (
            "No episodes listed",
            "TMDB lists no episodes for this season yet.".into(),
            "",
        ),
    };
    SeasonPanel {
        state: "message".into(),
        title: title.into(),
        message: message.into(),
        action: action.into(),
        ..header
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use crate::metadata::FRESH_FOR;
    use crate::search::tests::item;
    use crate::tests::Headless;
    use crate::tmdb::fake::{FakeServer, Reply, movie_details, season_details, tv_details};
    use slint::Model;
    use std::sync::atomic::{AtomicI64, Ordering};

    const TOKEN: &str = "detail-token-1a2b";
    const START: i64 = 1_800_000_000;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// TMDB with movie 603 and series 1399 (specials, seasons 1 and 2, the
    /// first slow to answer). Movie 404 and anything else is not found.
    fn tmdb() -> FakeServer {
        FakeServer::start(|seen| {
            let path = seen.target.split('?').next().unwrap_or("");
            match path {
                "/3/movie/603" => Reply::json(200, &movie_details(603, "The Matrix")),
                "/3/movie/604" => {
                    Reply::json(200, &movie_details(604, "Slow Matrix")).after(ms(700))
                }
                "/3/movie/500" => Reply::json(500, r#"{"status_code":11}"#),
                "/3/tv/1399" => Reply::json(
                    200,
                    &tv_details(1399, "Game of Thrones", &[(0, 2), (1, 3), (2, 12)]),
                ),
                "/3/tv/1399/season/0" => Reply::json(200, &season_details(0, 2)),
                "/3/tv/1399/season/1" => Reply::json(200, &season_details(1, 3)).after(ms(700)),
                "/3/tv/1399/season/2" => Reply::json(200, &season_details(2, 12)),
                _ => Reply::json(404, r#"{"status_code":34}"#),
            }
        })
    }

    struct Pane {
        ui: Headless,
        token: SharedToken,
        seconds: Arc<AtomicI64>,
        ids: Vec<i64>,
    }

    impl Pane {
        /// A detail pane on `ui` over `db` (already holding its titles),
        /// talking to `base`, with a fake clock at `START`.
        fn new(base: &str, db: SharedDb, token: Option<&str>) -> Self {
            let ui = Headless::new(1280, 800);
            let log = Arc::new(Log::stderr_only());
            let client = TmdbClient::for_tests(base, Duration::from_secs(5));
            let network = Network::with_post(4, ui.post());
            let posters =
                crate::posters(&ui.app, None, client.clone(), network.clone(), log.clone());
            posters.set_base_url(format!("{base}/t/p/"));
            let shared = SharedToken::default();
            shared.set(token.and_then(crate::secrets::Token::parse));
            let (clock, seconds) = Clock::fake(START);
            start(
                &ui.app,
                db.clone(),
                client,
                shared.clone(),
                network,
                posters,
                clock,
                log,
            );
            let ids = db
                .with(|db| {
                    let mut stmt = db
                        .conn()
                        .prepare("SELECT local_media_id FROM media ORDER BY local_media_id")
                        .unwrap();
                    let rows = stmt.query_map([], |r| r.get(0)).unwrap();
                    Ok(rows.map(Result::unwrap).collect())
                })
                .unwrap();
            Self {
                ui,
                token: shared,
                seconds,
                ids,
            }
        }

        fn app(&self) -> AppWindow {
            self.ui.app.clone_strong()
        }

        /// Selects title `n` and lets the selection settle.
        fn open(&self, n: usize) {
            self.app().invoke_detail_selected(self.ids[n] as i32);
            self.ui.advance(SETTLE);
        }

        /// Selects season row `row` and lets the selection settle.
        fn season(&self, row: i32) {
            self.app().invoke_detail_season_selected(row);
            self.ui.advance(SETTLE);
        }

        fn wait(&self, what: &str, done: impl Fn(&AppWindow) -> bool) {
            self.ui.pump_until(what, done);
        }

        /// Pumps until no request is on its way.
        fn settle(&self) {
            self.wait("the requests", |app| !app.get_media_detail().refreshing);
        }

        fn later(&self, seconds: i64) {
            self.seconds.fetch_add(seconds, Ordering::SeqCst);
        }
    }

    /// A library of movie 603, series 1399, movie 604 (slow), movie 404
    /// (gone from TMDB) and movie 500 (server error), in that order.
    fn library() -> SharedDb {
        let db = SharedDb::default();
        db.set(Database::open_in_memory());
        db.with(|db| {
            for (kind, id, title) in [
                (MediaType::Movie, 603, "The Matrix"),
                (MediaType::Tv, 1399, "Game of Thrones"),
                (MediaType::Movie, 604, "Slow Matrix"),
                (MediaType::Movie, 404, "Gone Movie"),
                (MediaType::Movie, 500, "Broken Movie"),
            ] {
                library::add(db, &item(kind, id, title), 1)?;
            }
            Ok(())
        })
        .unwrap();
        db
    }

    fn requests(server: &FakeServer, path: &str) -> usize {
        let wanted = format!("{path}?");
        server
            .seen()
            .iter()
            .filter(|seen| seen.target.starts_with(&wanted))
            .count()
    }

    fn facts_of(app: &AppWindow) -> Vec<(String, String)> {
        app.get_detail_facts()
            .iter()
            .map(|fact| (fact.label.into(), fact.value.into()))
            .collect()
    }

    fn episode_titles(app: &AppWindow) -> Vec<String> {
        app.get_detail_episodes()
            .iter()
            .map(|row| row.title.into())
            .collect()
    }

    fn season_titles(app: &AppWindow) -> Vec<String> {
        app.get_detail_seasons()
            .iter()
            .map(|row| row.title.into())
            .collect()
    }

    #[test]
    fn cached_detail_shows_first_then_the_policy_decides() {
        let server = tmdb();
        let mut pane = Pane::new(&server.base, library(), Some(TOKEN));
        let app = &pane.app();

        // Never fetched: the search-time metadata shows at once, a request
        // starts in the background.
        pane.open(0);
        let shown = app.get_media_detail();
        assert!(shown.has_item);
        assert_eq!(shown.title, "The Matrix");
        assert_eq!(shown.freshness, "Details not downloaded yet");
        assert!(shown.refreshing);
        assert!(
            facts_of(app).iter().all(|(label, _)| label != "Runtime"),
            "not invented"
        );
        pane.settle();
        assert_eq!(requests(&server, "/3/movie/603"), 1);
        let shown = app.get_media_detail();
        assert_eq!(
            (shown.freshness.as_str(), shown.notice.as_str()),
            ("Updated today", "")
        );
        assert_eq!(shown.tagline, "Free your mind.");
        assert_eq!(
            facts_of(app),
            [
                ("Released".into(), "1999-03-31".into()),
                ("Runtime".into(), "2 h 16 min".into()),
                ("Status".into(), "Released".into()),
                ("Genres".into(), "Action, Science Fiction".into()),
            ]
        );

        // Fresh: opening it again sends nothing.
        pane.open(1);
        pane.settle();
        pane.later(FRESH_FOR - 60);
        pane.open(0);
        assert!(!app.get_media_detail().refreshing);
        assert_eq!(
            requests(&server, "/3/movie/603"),
            1,
            "fresh detail is not refetched"
        );
        assert_eq!(app.get_media_detail().freshness, "Updated 6 days ago");

        // Stale: the cached detail shows while a refresh runs.
        pane.open(1);
        pane.settle();
        pane.later(120);
        pane.open(0);
        let shown = app.get_media_detail();
        assert!(
            shown.refreshing,
            "a stale detail refreshes in the background"
        );
        assert_eq!(
            shown.tagline, "Free your mind.",
            "and stays on screen meanwhile"
        );
        pane.settle();
        assert_eq!(requests(&server, "/3/movie/603"), 2);
        assert_eq!(app.get_media_detail().freshness, "Updated today");

        // Manual refresh always asks, even when fresh.
        app.invoke_detail_refresh();
        assert!(app.get_media_detail().refreshing);
        pane.settle();
        assert_eq!(requests(&server, "/3/movie/603"), 3);
        pane.ui.render();
    }

    #[test]
    fn a_failed_refresh_keeps_the_cached_detail_and_404_keeps_the_title() {
        let server = tmdb();
        let db = library();
        let pane = Pane::new(&server.base, db.clone(), Some(TOKEN));
        let app = &pane.app();

        // Give "Broken Movie" (TMDB 500) a cached detail first.
        let id = pane.ids[4];
        let cached = crate::metadata::tests::movie("Broken Movie", Vec::new());
        db.with(|db| metadata::save(db, id, &cached, START))
            .unwrap();
        pane.later(FRESH_FOR + 1);
        pane.open(4);
        pane.settle();
        let shown = app.get_media_detail();
        assert_eq!(shown.title, "Broken Movie");
        assert_eq!(shown.tagline, "A tagline.", "nothing cached was erased");
        assert_eq!(shown.freshness, "Updated 7 days ago");
        assert!(
            shown
                .notice
                .starts_with("Not refreshed. TMDB is having problems"),
            "{}",
            shown.notice
        );
        let stored = db.with(|db| metadata::read(db, id)).unwrap().unwrap();
        assert_eq!(stored.fetched_at, Some(START), "still stale, still there");

        // TMDB no longer has "Gone Movie": it stays in the library.
        pane.open(3);
        pane.settle();
        let shown = app.get_media_detail();
        assert_eq!(shown.title, "Gone Movie");
        assert!(
            shown.notice.starts_with("TMDB no longer has this title"),
            "{}",
            shown.notice
        );
        assert_eq!(db.with(library::count).unwrap(), 5, "nothing was removed");
        assert!(
            db.with(|db| metadata::read(db, pane.ids[3]))
                .unwrap()
                .is_some()
        );

        // Retrying (manual refresh) is always possible.
        app.invoke_detail_refresh();
        pane.settle();
        assert_eq!(requests(&server, "/3/movie/404"), 2);
    }

    #[test]
    fn series_detail_saves_seasons_and_fetches_only_the_shown_season() {
        let server = tmdb();
        let db = library();
        let mut pane = Pane::new(&server.base, db.clone(), Some(TOKEN));
        let app = &pane.app();

        pane.open(1);
        assert!(app.get_media_detail().is_tv);
        assert_eq!(app.get_detail_season().title, "No seasons yet");
        // Details, then season 1 (the first regular season), which is slow.
        pane.wait("the season list", |app| {
            app.get_detail_seasons().row_count() == 3
        });
        assert_eq!(season_titles(app), ["Specials", "Season 1", "Season 2"]);
        assert_eq!(app.get_detail_season_row(), 1);
        assert_eq!(app.get_detail_season().title, "Loading episodes…");
        pane.wait("season 1", |app| app.get_detail_episodes().row_count() == 3);
        pane.settle();
        let panel = app.get_detail_season();
        assert_eq!(panel.state, "episodes");
        assert_eq!(panel.heading, "Season 1");
        assert_eq!(panel.meta, "Aired 2011-04-11 · 3 episodes");
        assert_eq!(episode_titles(app), ["Episode 1", "Episode 2", "Episode 3"]);
        let first = app.get_detail_episodes().row_data(0).unwrap();
        assert_eq!(
            (first.label.as_str(), first.meta.as_str()),
            ("E1", ""),
            "no invented runtime"
        );
        let second = app.get_detail_episodes().row_data(1).unwrap();
        assert_eq!(second.meta, "2011-05-02 · 57 min");
        assert_eq!(
            facts_of(app),
            [
                ("First aired".into(), "2011-04-17".into()),
                ("Last aired".into(), "2019-05-19".into()),
                ("Status".into(), "Ended".into()),
                ("Seasons".into(), "3".into()),
                ("Episodes".into(), "17".into()),
                ("Episode length".into(), "57 min".into()),
                ("Genres".into(), "Drama".into()),
            ]
        );
        let coverage: Vec<String> = app
            .get_detail_seasons()
            .iter()
            .map(|row| row.coverage.into())
            .collect();
        assert_eq!(coverage, ["Not downloaded", "", "Not downloaded"]);
        // Two requests, not one per season.
        let series: Vec<String> = server
            .seen()
            .into_iter()
            .map(|seen| seen.target)
            .filter(|target| target.starts_with("/3/tv/"))
            .collect();
        assert_eq!(
            series,
            [
                "/3/tv/1399?language=en-US",
                "/3/tv/1399/season/1?language=en-US"
            ]
        );

        // Specials open like any season.
        pane.season(0);
        assert_eq!(app.get_detail_season().heading, "Specials");
        pane.wait("the specials", |app| {
            app.get_detail_episodes().row_count() == 2
        });
        pane.settle();

        // Back to season 1: cached, fresh, no request.
        let sent = server.seen().len();
        pane.season(1);
        assert_eq!(
            app.get_detail_episodes().row_count(),
            3,
            "from SQLite at once"
        );
        assert!(!app.get_media_detail().refreshing);
        assert_eq!(server.seen().len(), sent);

        // Selecting an episode shows its overview.
        app.invoke_detail_episode_selected(2);
        assert_eq!(app.get_detail_episode_row(), 2);
        pane.ui.render();

        // Everything is in SQLite, keyed to the series.
        let id = pane.ids[1];
        let stored = db.with(|db| metadata::read(db, id)).unwrap().unwrap();
        let seasons = stored.tv.unwrap().seasons;
        assert_eq!(seasons.len(), 3);
        assert_eq!(seasons[1].coverage(), Coverage::Complete(3));
        assert_eq!(seasons[2].coverage(), Coverage::NotFetched);
    }

    #[test]
    fn a_late_answer_never_reaches_another_title_or_season() {
        let server = tmdb();
        let db = library();
        let pane = Pane::new(&server.base, db.clone(), Some(TOKEN));
        let app = &pane.app();

        // Title A (slow) is opened, then title B before A answers.
        pane.open(2);
        pane.open(0);
        pane.wait("B", |app| {
            app.get_media_detail().freshness == "Updated today"
        });
        assert_eq!(app.get_media_detail().title, "The Matrix");
        pane.wait("A's late answer", |_| {
            db.with(|db| metadata::read(db, pane.ids[2]))
                .unwrap()
                .is_some_and(|details| details.fetched_at.is_some())
        });
        pane.ui.pump();
        let shown = app.get_media_detail();
        assert_eq!(shown.title, "The Matrix", "A's answer did not replace B");
        assert!(!shown.refreshing);

        // Season 1 (slow) is shown, then season 2 before season 1 answers.
        pane.open(1);
        pane.wait("the season list", |app| {
            app.get_detail_seasons().row_count() == 3
        });
        assert_eq!(app.get_detail_season().heading, "Season 1");
        pane.season(2);
        pane.wait("season 2", |app| {
            app.get_detail_episodes().row_count() == 12
        });
        pane.wait("season 1's late answer", |_| {
            db.with(|db| metadata::read_episodes(db, pane.ids[1], 1))
                .unwrap()
                .len()
                == 3
        });
        pane.ui.pump();
        assert_eq!(app.get_detail_season().heading, "Season 2");
        assert_eq!(
            app.get_detail_episodes().row_count(),
            12,
            "season 1 did not land here"
        );
        let first = app.get_detail_episodes().row_data(0).unwrap();
        assert_eq!(first.title, "Episode 1");
        // The late season 1 answer was saved for season 1.
        pane.season(1);
        assert_eq!(app.get_detail_episodes().row_count(), 3);
    }

    #[test]
    fn without_a_token_nothing_is_requested_until_one_appears() {
        let server = tmdb();
        let pane = Pane::new(&server.base, library(), None);
        let app = &pane.app();
        pane.open(1);
        let shown = app.get_media_detail();
        assert!(!shown.can_refresh && !shown.refreshing);
        assert_eq!(app.get_detail_season().title, "No seasons yet");
        assert!(app.get_detail_season().message.contains("Settings"));
        app.invoke_detail_refresh();
        std::thread::sleep(ms(100));
        pane.ui.pump();
        assert!(server.seen().is_empty());

        // The token arrives (Settings, or the credential store at startup).
        pane.token.set(crate::secrets::Token::parse(TOKEN));
        app.invoke_token_changed();
        pane.wait("the seasons", |app| {
            app.get_detail_seasons().row_count() == 3
        });
        assert!(app.get_media_detail().can_refresh);
    }

    #[test]
    fn a_failed_season_fetch_is_retryable_and_never_hides_cached_episodes() {
        // Season 2 fails; everything else answers.
        let failing = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let fail = failing.clone();
        let server =
            FakeServer::start(
                move |seen| match seen.target.split('?').next().unwrap_or("") {
                    "/3/tv/1399" => Reply::json(200, &tv_details(1399, "GoT", &[(1, 1), (2, 2)])),
                    "/3/tv/1399/season/1" => Reply::json(200, &season_details(1, 1)),
                    "/3/tv/1399/season/2" if fail.load(Ordering::SeqCst) => {
                        Reply::json(503, r#"{"status_code":46}"#)
                    }
                    "/3/tv/1399/season/2" => Reply::json(200, &season_details(2, 2)),
                    _ => Reply::json(404, "{}"),
                },
            );
        let pane = Pane::new(&server.base, library(), Some(TOKEN));
        let app = &pane.app();
        pane.open(1);
        pane.wait("season 1", |app| app.get_detail_episodes().row_count() == 1);
        pane.settle();

        // Nothing cached for season 2 and the request fails: retryable.
        pane.season(1);
        pane.settle();
        let panel = app.get_detail_season();
        assert_eq!(
            (panel.state.as_str(), panel.title.as_str()),
            ("message", "Episodes not available")
        );
        assert_eq!(panel.action, "retry");
        failing.store(false, Ordering::SeqCst);
        app.invoke_detail_retry_season();
        pane.wait("season 2", |app| app.get_detail_episodes().row_count() == 2);
        pane.settle();
        assert_eq!(app.get_detail_season().notice, "");

        // Cached now; a failing manual refresh keeps the list and says so.
        failing.store(true, Ordering::SeqCst);
        app.invoke_detail_refresh();
        pane.settle();
        let panel = app.get_detail_season();
        assert_eq!(panel.state, "episodes");
        assert_eq!(app.get_detail_episodes().row_count(), 2);
        assert!(
            panel.notice.starts_with("Showing saved episodes."),
            "{}",
            panel.notice
        );
    }

    /// The R9 offline scenario: fetch a movie and a series with a season
    /// online, close, reopen with TMDB unreachable — everything shows.
    #[test]
    fn details_seasons_and_episodes_work_offline_after_one_fetch() {
        use crate::paths::TestDir;
        let server = tmdb();
        let dir = TestDir::new("detail-offline");
        let path = dir.0.join("bingee.db");
        let log = Log::stderr_only();
        {
            let db = Database::open(&path, &log).unwrap();
            library::add(&db, &item(MediaType::Movie, 603, "The Matrix"), 1).unwrap();
            library::add(&db, &item(MediaType::Tv, 1399, "Game of Thrones"), 1).unwrap();
        }

        let (online, base) = (path.clone(), server.base.clone());
        std::thread::spawn(move || {
            let db = SharedDb::default();
            db.set(Database::open(&online, &Log::stderr_only()).unwrap());
            let pane = Pane::new(&base, db, Some(TOKEN));
            pane.open(0);
            pane.settle();
            pane.open(1);
            pane.wait("season 1", |app| app.get_detail_episodes().row_count() == 3);
            pane.settle();
        })
        .join()
        .unwrap();
        let online_requests = server.seen().len();
        let api: Vec<String> = server
            .seen()
            .into_iter()
            .map(|seen| seen.target)
            .filter(|target| target.starts_with("/3/"))
            .collect();
        assert_eq!(
            api,
            [
                "/3/movie/603?language=en-US",
                "/3/tv/1399?language=en-US",
                "/3/tv/1399/season/1?language=en-US"
            ],
            "the movie, the series, one season; posters aside"
        );

        let closed = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        std::thread::spawn(move || {
            let db = SharedDb::default();
            db.set(Database::open(&path, &Log::stderr_only()).unwrap());
            let mut pane = Pane::new(&format!("http://{closed}"), db, Some(TOKEN));
            let app = &pane.app();
            pane.later(3_600);

            pane.open(0);
            assert!(
                !app.get_media_detail().refreshing,
                "fresh: nothing to fetch"
            );
            assert_eq!(app.get_media_detail().tagline, "Free your mind.");
            assert_eq!(facts_of(app).len(), 4);

            pane.open(1);
            assert!(!app.get_media_detail().refreshing);
            assert_eq!(season_titles(app), ["Specials", "Season 1", "Season 2"]);
            assert_eq!(app.get_detail_season().state, "episodes");
            assert_eq!(episode_titles(app), ["Episode 1", "Episode 2", "Episode 3"]);
            pane.ui.render();

            // Stale and offline: cached content stays, the notice explains.
            pane.later(FRESH_FOR);
            pane.open(0);
            pane.open(1);
            pane.settle();
            let shown = app.get_media_detail();
            assert_eq!(
                shown.notice,
                "Can't reach TMDB. Showing the details saved on this computer."
            );
            assert_eq!(season_titles(app).len(), 3);
            assert_eq!(app.get_detail_episodes().row_count(), 3);
            assert!(
                app.get_detail_season()
                    .notice
                    .starts_with("Showing saved episodes.")
            );
            pane.ui.render();
        })
        .join()
        .unwrap();
        assert_eq!(
            server.seen().len(),
            online_requests,
            "offline never reached TMDB"
        );
    }

    #[test]
    fn the_library_selection_drives_the_pane_and_it_is_keyboard_usable() {
        use slint::LogicalPosition;
        use slint::platform::{Key, PointerEventButton, WindowEvent};

        let server = tmdb();
        // A movie, then the series: the newest (the series) is row 0.
        let db = SharedDb::default();
        db.set(Database::open_in_memory());
        db.with(|db| {
            library::add(db, &item(MediaType::Movie, 603, "The Matrix"), 1)?;
            library::add(db, &item(MediaType::Tv, 1399, "Game of Thrones"), 1)
        })
        .unwrap();
        let mut pane = Pane::new(&server.base, db.clone(), Some(TOKEN));
        let view = Rc::new(
            crate::view::LibraryView::new(crate::view::UserLibrary::new(
                db,
                crate::poster::tests::offline(),
            ))
            .unwrap(),
        );
        crate::connect_library(&pane.ui.app, view, Arc::new(Log::stderr_only()));
        let app = pane.app();
        pane.ui.render();
        pane.ui.advance(SETTLE);
        pane.ui.advance(SETTLE);
        // The selection reaches the pane through `selected-id` alone.
        pane.wait("the series", |app| {
            app.get_media_detail().title == "Game of Thrones"
        });
        pane.wait("season 1", |app| app.get_detail_episodes().row_count() == 3);
        pane.settle();
        pane.ui.render();

        let window = app.window();
        let press = |key: Key| {
            let text: SharedString = key.into();
            window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
            window.dispatch_event(WindowEvent::KeyReleased { text });
        };
        // Click the first library row: the list has focus.
        let (position, button) = (LogicalPosition::new(500.0, 160.0), PointerEventButton::Left);
        window.dispatch_event(WindowEvent::PointerPressed { position, button });
        window.dispatch_event(WindowEvent::PointerReleased { position, button });
        assert_eq!(app.get_selected_row(), 0);

        // Tab moves on from the list, past Refresh and Remove, into the
        // season list, where Down selects the next season.
        let reached_seasons = (0..10).any(|_| {
            press(Key::Tab);
            press(Key::DownArrow);
            app.get_detail_season_row() == 2
        });
        assert!(reached_seasons, "Tab never reached the season list");
        assert_eq!(
            app.get_selected_row(),
            0,
            "the library list no longer had focus"
        );
        pane.ui.advance(SETTLE);
        pane.wait("season 2", |app| {
            app.get_detail_episodes().row_count() == 12
        });
        pane.settle();
        press(Key::Home);
        assert_eq!(app.get_detail_season().heading, "Specials");
        press(Key::End);
        assert_eq!(app.get_detail_season().heading, "Season 2");

        // Tab again: past "Mark season watched", into the episode list,
        // navigable with the usual keys.
        let reached_episodes = (0..3).any(|_| {
            press(Key::Tab);
            press(Key::DownArrow);
            app.get_detail_episode_row() == 0
        });
        assert!(reached_episodes, "Tab never reached the episode list");
        press(Key::End);
        assert_eq!(app.get_detail_episode_row(), 11);
        press(Key::PageUp);
        assert!(app.get_detail_episode_row() < 11);
        press(Key::Home);
        assert_eq!(app.get_detail_episode_row(), 0);

        // Space toggles the selected episode's watched state, Enter too.
        let space = |text: &str| {
            let text = SharedString::from(text);
            window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
            window.dispatch_event(WindowEvent::KeyReleased { text });
        };
        space(" ");
        assert!(app.get_detail_episodes().row_data(0).unwrap().watched);
        assert_eq!(app.get_detail_season().progress, "1 / 12 watched");
        press(Key::DownArrow);
        press(Key::Return);
        assert!(app.get_detail_episodes().row_data(1).unwrap().watched);
        press(Key::Return);
        assert!(!app.get_detail_episodes().row_data(1).unwrap().watched);
        assert_eq!(
            app.get_detail_episode_row(),
            1,
            "toggling keeps the selection"
        );
        press(Key::Home);

        // Focus is not trapped: one more Tab leaves the episode list.
        press(Key::Tab);
        press(Key::DownArrow);
        assert_eq!(app.get_detail_episode_row(), 0);

        // Switching pages puts focus back in the library list.
        app.set_page("settings".into());
        pane.ui.render();
        app.set_page("library".into());
        pane.ui.render();
        press(Key::DownArrow);
        assert_eq!(
            app.get_selected_row(),
            1,
            "the list has focus after the page switch"
        );
        pane.wait("the movie", |app| {
            app.get_media_detail().title == "The Matrix"
        });
        assert!(!app.get_media_detail().is_tv);
    }

    #[test]
    fn moving_through_titles_and_seasons_requests_only_where_it_stops() {
        let server = tmdb();
        let pane = Pane::new(&server.base, library(), Some(TOKEN));
        let app = &pane.app();

        // Down, Down, Down through never-fetched titles: each cached detail
        // renders at once, but nothing is requested for the ones passed over.
        for n in [0, 2, 3, 4] {
            app.invoke_detail_selected(pane.ids[n] as i32);
            assert_eq!(
                app.get_media_detail().freshness,
                "Details not downloaded yet"
            );
            pane.ui.advance(SETTLE / 2);
        }
        app.invoke_detail_selected(pane.ids[1] as i32);
        assert_eq!(app.get_media_detail().title, "Game of Thrones");
        std::thread::sleep(ms(100));
        pane.ui.pump();
        assert!(server.seen().is_empty(), "no request while moving");
        pane.ui.advance(SETTLE);
        pane.wait("the seasons", |app| {
            app.get_detail_seasons().row_count() == 3
        });
        pane.wait("season 1", |app| app.get_detail_episodes().row_count() == 3);
        pane.settle();

        // The same for seasons: passing over specials costs nothing; only
        // season 2, where the selection stops, is fetched.
        app.invoke_detail_season_selected(0);
        pane.ui.advance(SETTLE / 2);
        app.invoke_detail_season_selected(2);
        pane.ui.advance(SETTLE);
        pane.wait("season 2", |app| {
            app.get_detail_episodes().row_count() == 12
        });
        pane.settle();
        let api: Vec<String> = server
            .seen()
            .into_iter()
            .map(|seen| seen.target)
            .filter(|target| target.starts_with("/3/"))
            .collect();
        assert_eq!(
            api,
            [
                "/3/tv/1399?language=en-US",
                "/3/tv/1399/season/1?language=en-US",
                "/3/tv/1399/season/2?language=en-US",
            ]
        );
    }

    fn events(db: &SharedDb) -> usize {
        db.with(|db| tracking::recent_history(db, 1_000))
            .unwrap()
            .len()
    }

    /// Stores series 1399's details and every episode of `seasons`, as a
    /// session with TMDB would have, at `START`.
    fn seed_series(db: &SharedDb, id: i64, seasons: &[(i64, u32, u32)]) {
        use crate::metadata::tests::{episode, season, series};
        db.with(|db| {
            let summaries = seasons
                .iter()
                .map(|(n, count, _)| season(*n, Some(*count)))
                .collect();
            metadata::save(db, id, &series("Game of Thrones", summaries), START)?;
            for (n, _, stored) in seasons {
                let list: Vec<_> = (1..=i64::from(*stored)).map(|e| episode(*n, e)).collect();
                metadata::save_episodes(db, id, *n, &list, START)?;
            }
            Ok(())
        })
        .unwrap();
    }

    /// R10's journeys: Movie → watched → rating → restart, and TV → episodes
    /// watched → progress → restart, first without a token, then with TMDB
    /// unreachable. No tracking action sends a request.
    #[test]
    fn tracking_survives_a_restart_and_works_offline() {
        use crate::paths::TestDir;
        let dir = TestDir::new("detail-tracking-offline");
        let path = dir.0.join("bingee.db");
        let closed = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let base = format!("http://{closed}");
        {
            let db = Database::open(&path, &Log::stderr_only()).unwrap();
            library::add(&db, &item(MediaType::Movie, 603, "The Matrix"), 1).unwrap();
            library::add(&db, &item(MediaType::Tv, 1399, "Game of Thrones"), 1).unwrap();
        }

        // Session 1: no token at all.
        let (first, first_base) = (path.clone(), base.clone());
        std::thread::spawn(move || {
            let db = SharedDb::default();
            db.set(Database::open(&first, &Log::stderr_only()).unwrap());
            let mut pane = Pane::new(&first_base, db.clone(), None);
            seed_series(&db, pane.ids[1], &[(0, 2, 2), (1, 3, 3), (2, 2, 2)]);
            let app = &pane.app();

            pane.open(0);
            let shown = app.get_media_detail();
            assert_eq!(
                (shown.watched, shown.watched_label.as_str()),
                (false, "Not watched")
            );
            assert_eq!(app.get_detail_rating(), 0);
            app.invoke_detail_watch_toggle();
            let shown = app.get_media_detail();
            assert!(shown.watched);
            assert!(
                shown.watched_label.starts_with("Watched 20"),
                "{}",
                shown.watched_label
            );
            app.invoke_detail_rate(9);
            assert_eq!(app.get_detail_rating(), 9);
            assert_eq!(events(&db), 1);

            pane.open(1);
            assert_eq!(app.get_detail_season().heading, "Season 1");
            assert_eq!(
                app.get_media_detail().progress,
                "0 / 5 episodes watched · 0%"
            );
            assert_eq!(
                app.get_media_detail().progress_detail,
                "Next: S1 E1 · Episode 1"
            );
            app.invoke_detail_episode_toggle(0);
            app.invoke_detail_episode_toggle(2);
            assert_eq!(app.get_detail_season().progress, "2 / 3 watched");
            assert!(app.get_detail_season().can_watch_all);
            assert_eq!(
                app.get_media_detail().progress_detail,
                "Next: S1 E2 · Episode 2"
            );
            // Specials: tracked, but apart from the main progress.
            pane.season(0);
            app.invoke_detail_season_watched(true);
            let shown = app.get_media_detail();
            assert_eq!(shown.specials, "2 / 2 specials watched");
            assert_eq!(shown.progress, "2 / 5 episodes watched · 40%");
            pane.ui.render();
        })
        .join()
        .unwrap();

        // Session 2: a token, but TMDB cannot be reached.
        std::thread::spawn(move || {
            let db = SharedDb::default();
            db.set(Database::open(&path, &Log::stderr_only()).unwrap());
            let mut pane = Pane::new(&base, db.clone(), Some(TOKEN));
            let app = &pane.app();
            pane.open(0);
            assert!(app.get_media_detail().watched);
            assert_eq!(app.get_detail_rating(), 9);

            pane.open(1);
            pane.settle();
            assert_eq!(app.get_detail_season().progress, "2 / 3 watched");
            let rows: Vec<bool> = app
                .get_detail_episodes()
                .iter()
                .map(|row| row.watched)
                .collect();
            assert_eq!(rows, [true, false, true]);
            let seasons: Vec<String> = app
                .get_detail_seasons()
                .iter()
                .map(|row| row.meta.into())
                .collect();
            assert_eq!(
                seasons,
                [
                    "2011 · 2 episodes · 2 / 2 watched",
                    "2011 · 3 episodes · 2 / 3 watched",
                    "2011 · 2 episodes"
                ]
            );
            // Finish season 1 and 2 while TMDB is unreachable: complete.
            app.invoke_detail_season_watched(true);
            pane.season(2);
            app.invoke_detail_season_watched(true);
            let shown = app.get_media_detail();
            assert_eq!(shown.progress, "5 / 5 episodes watched · 100%");
            assert_eq!(shown.progress_detail, "Series watched");
            assert_eq!(shown.progress_fraction, 1.0);
            assert_eq!(shown.tracking_notice, "");
            // Unmark the season: state goes, history stays.
            let before = events(&db);
            app.invoke_detail_season_watched(false);
            assert_eq!(app.get_detail_season().progress, "0 / 2 watched");
            assert!(!app.get_detail_season().can_unwatch_all);
            assert_eq!(events(&db), before);
            pane.ui.render();
        })
        .join()
        .unwrap();
    }

    #[test]
    fn a_failed_personal_write_shows_the_committed_state_and_says_so() {
        let server = tmdb();
        let db = library();
        let pane = Pane::new(&server.base, db.clone(), None);
        let app = &pane.app();
        pane.open(0);
        app.invoke_detail_rate(6);
        db.with(|db| {
            db.conn()
                .execute_batch(
                    "CREATE TEMP TRIGGER fail BEFORE UPDATE ON media_tracking
                     BEGIN SELECT RAISE(ABORT, 'simulated'); END;",
                )
                .unwrap();
            Ok(())
        })
        .unwrap();

        // The menu already shows 2; the write fails; the menu goes back to 6.
        app.set_detail_rating(2);
        app.invoke_detail_rate(2);
        assert_eq!(app.get_detail_rating(), 6);
        let notice = app.get_media_detail().tracking_notice;
        assert!(notice.contains("could not be saved"), "{notice}");
        // Mark watched fails too: still shown as not watched, no event.
        app.invoke_detail_watch_toggle();
        assert!(!app.get_media_detail().watched);
        assert_eq!(events(&db), 0);
        // The metadata part of the pane is unaffected.
        assert_eq!(app.get_media_detail().title, "The Matrix");
        assert!(server.seen().is_empty(), "no network for personal actions");

        // The next successful change clears the notice.
        db.with(|db| {
            db.conn().execute_batch("DROP TRIGGER fail").unwrap();
            Ok(())
        })
        .unwrap();
        app.invoke_detail_watch_toggle();
        assert!(app.get_media_detail().watched);
        assert_eq!(app.get_media_detail().tracking_notice, "");
    }

    #[test]
    fn refresh_404_and_new_episodes_never_touch_personal_state() {
        let server = tmdb();
        let db = library();
        let pane = Pane::new(&server.base, db.clone(), Some(TOKEN));
        let app = &pane.app();

        // A movie watched and rated before its first detail fetch: the TMDB
        // answer (vote average 8.2) is metadata, never the user's rating.
        pane.open(0);
        app.invoke_detail_watch_toggle();
        app.invoke_detail_rate(4);
        pane.settle();
        app.invoke_detail_refresh();
        pane.settle();
        assert_eq!(app.get_media_detail().tagline, "Free your mind.");
        assert!(app.get_media_detail().watched);
        assert_eq!(app.get_detail_rating(), 4);

        // TMDB no longer has "Gone Movie": rating and history stay.
        pane.open(3);
        app.invoke_detail_watch_toggle();
        app.invoke_detail_rate(8);
        app.invoke_detail_refresh();
        pane.settle();
        assert!(
            app.get_media_detail()
                .notice
                .starts_with("TMDB no longer has")
        );
        assert!(app.get_media_detail().watched);
        assert_eq!(app.get_detail_rating(), 8);

        // A series: season 1 watched in full, then TMDB refreshes it.
        pane.open(1);
        pane.wait("season 1", |app| app.get_detail_episodes().row_count() == 3);
        pane.settle();
        app.invoke_detail_season_watched(true);
        let before = events(&db);
        app.invoke_detail_refresh();
        pane.settle();
        assert_eq!(app.get_detail_season().progress, "3 / 3 watched");
        assert_eq!(events(&db), before);
        assert_eq!(
            app.get_media_detail().progress,
            "3 / 3 downloaded episodes watched · episode list incomplete",
            "specials and season 2 are not downloaded: never 100%"
        );
        assert_eq!(
            app.get_media_detail().progress_detail,
            "All downloaded episodes watched; more may exist"
        );
    }

    /// Informal R10 observation, not a `BENCHMARK_SPEC.md` run: what a click
    /// costs on the UI thread end to end (SQLite commit, tracking re-read,
    /// pane models, Library status reload), in a file-backed database with a
    /// 1,000-title library and a 250-episode season. Run with
    /// `cargo test --release informal_tracking_ui_timings -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn informal_tracking_ui_timings() {
        use crate::paths::TestDir;
        let dir = TestDir::new("detail-tracking-timings");
        let db = SharedDb::default();
        db.set(Database::open(&dir.0.join("bingee.db"), &Log::stderr_only()).unwrap());
        db.with(|db| {
            for n in 1..1_000 {
                library::add(db, &item(MediaType::Movie, n, &format!("Movie {n}")), 1)?;
            }
            library::add(db, &item(MediaType::Tv, 1399, "Long Season"), 2)?;
            Ok(())
        })
        .unwrap();
        let pane = Pane::new("http://127.0.0.1:9", db.clone(), None);
        let series = *pane.ids.last().unwrap();
        seed_series(&db, series, &[(1, 250, 250)]);
        let view = Rc::new(
            crate::view::LibraryView::new(crate::view::UserLibrary::new(
                db.clone(),
                crate::poster::tests::offline(),
            ))
            .unwrap(),
        );
        crate::connect_library(&pane.ui.app, view, Arc::new(Log::stderr_only()));
        let app = pane.app();
        app.invoke_detail_selected(series as i32);
        assert_eq!(app.get_detail_episodes().row_count(), 250);

        let median = |mut samples: Vec<Duration>| {
            samples.sort();
            samples[samples.len() / 2].as_secs_f64() * 1e3
        };
        let time = |label: &str, runs: usize, f: &mut dyn FnMut(usize)| {
            let samples: Vec<Duration> = (0..runs)
                .map(|run| {
                    let start = Instant::now();
                    f(run);
                    start.elapsed()
                })
                .collect();
            let worst = samples.iter().max().unwrap().as_secs_f64() * 1e3;
            println!(
                "{label:<46} median {:.3} ms, max {worst:.3} ms ({runs} runs)",
                median(samples)
            );
        };
        time("toggle episode (rapid, 100 clicks)", 100, &mut |run| {
            app.invoke_detail_episode_toggle((run % 250) as i32);
        });
        time("mark 250-episode season watched", 20, &mut |_| {
            app.invoke_detail_season_watched(true);
            app.invoke_detail_season_watched(false);
        });
        app.invoke_detail_selected(pane.ids[0] as i32);
        time("movie watched toggle", 50, &mut |_| {
            app.invoke_detail_watch_toggle();
        });
        time("rating change", 50, &mut |run| {
            app.invoke_detail_rate((run % 10 + 1) as i32);
        });
    }

    #[test]
    fn view_helpers_never_invent_values() {
        assert_eq!(duration(57), "57 min");
        assert_eq!(duration(120), "2 h");
        assert_eq!(duration(136), "2 h 16 min");
        assert_eq!(updated(None, START), "Details not downloaded yet");
        assert_eq!(updated(Some(START), START + 86_399), "Updated today");
        assert_eq!(updated(Some(START), START + 86_400), "Updated yesterday");
        assert_eq!(
            updated(Some(START + 500), START),
            "Updated today",
            "clock moved back"
        );
        let mut season = crate::metadata::tests::season(0, None);
        season.air_date = None;
        let row = season_row_view(&season, None);
        assert_eq!((row.title.as_str(), row.meta.as_str()), ("Specials", ""));
        assert_eq!(row.coverage, "Not downloaded");
        let mut episode = crate::metadata::tests::episode(1, 4);
        episode.name = None;
        episode.air_date = None;
        episode.runtime_minutes = None;
        let row = episode_row_view(&episode, false);
        assert_eq!((row.title.as_str(), row.meta.as_str()), ("Episode 4", ""));
        let seasons = [crate::metadata::tests::season(0, Some(1))];
        assert_eq!(
            default_season(&seasons),
            Some(0),
            "specials when that is all"
        );
        assert_eq!(default_season(&[]), None);
    }
}
