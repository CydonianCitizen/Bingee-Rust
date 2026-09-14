//! Remote features on the window: the TMDB token in Settings and the Discover
//! search. The state sits behind one mutex that is only locked on the UI
//! thread. Network and credential-store calls run on the worker pool and hand
//! their results back through the Slint event loop (ADR-0008, ADR-0009).
//! Search results stay in memory; the database is touched only to look up
//! which results are in the library (one query per result set) and to add
//! the chosen one (ADR-0010).

use std::collections::HashSet;
use std::rc::Rc;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use slint::{ComponentHandle, Model, ModelNotify, ModelRc, ModelTracker, SharedString, Timer};

use crate::database::SharedDb;
use crate::diagnostics::Log;
use crate::error::AppError;
use crate::library::{self, Added};
use crate::network::Network;
use crate::poster::{PosterKey, Posters};
use crate::search::{
    DEBOUNCE, ExternalRef, MediaSearchResult, Request, SearchController, SearchPage, Status,
};
use crate::secrets::{STORE_NAME, SecretStore, Token};
use crate::tmdb::{TmdbClient, TmdbError};
use crate::view::{media_row, reselect, with_poster};
use crate::{AppWindow, DiscoverView, MediaRow, TmdbView};

/// What is known about the token.
#[derive(Debug, Clone, PartialEq)]
enum Credential {
    /// Reading the credential store at startup.
    Loading,
    Missing,
    Checking,
    Valid,
    Invalid,
    /// TMDB could not be reached to check the token.
    Offline,
    /// Rate limit, TMDB trouble or an unreadable answer.
    Trouble(TmdbError),
    StorageUnavailable,
}

impl Credential {
    fn from_check(result: &Result<(), TmdbError>) -> Self {
        match result {
            Ok(()) => Credential::Valid,
            Err(TmdbError::CredentialInvalid) => Credential::Invalid,
            Err(TmdbError::Offline(_) | TmdbError::Timeout) => Credential::Offline,
            Err(other) => Credential::Trouble(other.clone()),
        }
    }
}

struct State {
    token: Option<Token>,
    credential: Credential,
    /// What happened to the last token action, e.g. "It was not saved."
    note: String,
    search: SearchController,
    selected: Option<ExternalRef>,
    /// The user chose `selected`; otherwise the top result is selected.
    picked: bool,
    /// The results as the list shows them (a snapshot for the model).
    list: Vec<MediaSearchResult>,
    /// Which of `list` are in the library, as the database last said.
    in_library: HashSet<ExternalRef>,
    /// The library changed; look `in_library` up again.
    membership_stale: bool,
    /// Why the last Add to Library failed.
    add_error: String,
    /// `/3/configuration` was requested this session.
    image_base_requested: bool,
}

#[derive(Clone)]
struct Remote {
    window: slint::Weak<AppWindow>,
    state: Arc<Mutex<State>>,
    network: Network,
    client: TmdbClient,
    secrets: Arc<dyn SecretStore>,
    log: Arc<Log>,
    db: SharedDb,
    posters: Arc<Posters>,
}

/// The Discover list: rows are built only when the list view asks, so only
/// visible results request posters.
struct DiscoverModel {
    remote: Remote,
    notify: ModelNotify,
}

impl Model for DiscoverModel {
    type Data = MediaRow;

    fn row_count(&self) -> usize {
        self.remote.state().list.len()
    }

    fn row_data(&self, row: usize) -> Option<MediaRow> {
        let state = self.remote.state();
        let result = state.list.get(row)?.clone();
        let member = state.in_library.contains(&result.external);
        drop(state);
        Some(self.remote.row(&result, member))
    }

    fn model_tracker(&self) -> &dyn ModelTracker {
        &self.notify
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// A poster finished loading: Discover rows and the preview that show it are
/// read again. Rows that now show another result are left alone.
pub fn poster_ready(window: &AppWindow, key: &PosterKey) {
    let model = window.get_discover_results();
    let Some(model) = model.as_any().downcast_ref::<DiscoverModel>() else {
        return;
    };
    let rows: Vec<usize> = model
        .remote
        .state()
        .list
        .iter()
        .enumerate()
        .filter(|(_, r)| r.poster_path.as_deref().and_then(PosterKey::tmdb).as_ref() == Some(key))
        .map(|(row, _)| row)
        .collect();
    rows.into_iter()
        .for_each(|row| model.notify.row_changed(row));
    if window.get_discover_detail().poster_key == key.name() {
        model.remote.render(window);
    }
}

enum Loaded {
    Found(Token, Result<(), TmdbError>),
    Missing,
    Unavailable(AppError),
}

enum Saved {
    Accepted(Token, Result<(), AppError>),
    Rejected(TmdbError),
}

/// Wires Discover and the TMDB part of Settings to `window`, then reads the
/// saved token (and checks it with TMDB) in the background. `db` is the
/// library database, once it is open.
pub fn start(
    window: &AppWindow,
    client: TmdbClient,
    secrets: Arc<dyn SecretStore>,
    network: Network,
    log: Arc<Log>,
    db: SharedDb,
    posters: Arc<Posters>,
) {
    let remote = Remote {
        window: window.as_weak(),
        state: Arc::new(Mutex::new(State {
            token: None,
            credential: Credential::Loading,
            note: String::new(),
            search: SearchController::new(false),
            selected: None,
            picked: false,
            list: Vec::new(),
            in_library: HashSet::new(),
            membership_stale: false,
            add_error: String::new(),
            image_base_requested: false,
        })),
        network,
        client,
        secrets,
        log,
        db,
        posters,
    };
    window.set_discover_results(ModelRc::from(Rc::new(DiscoverModel {
        remote: remote.clone(),
        notify: ModelNotify::default(),
    })));
    window.on_discover_add({
        let remote = remote.clone();
        move || remote.add()
    });
    window.on_membership_changed({
        let remote = remote.clone();
        move || {
            remote.state().membership_stale = true;
            remote.refresh();
        }
    });
    window.on_discover_query_changed({
        let remote = remote.clone();
        move |text| remote.input(&text)
    });
    window.on_discover_load_more({
        let remote = remote.clone();
        move || remote.load_more()
    });
    window.on_discover_row_selected({
        let remote = remote.clone();
        move |row| remote.select(row)
    });
    window.on_tmdb_save({
        let remote = remote.clone();
        move |text| remote.save(&text)
    });
    window.on_tmdb_check({
        let remote = remote.clone();
        move || remote.check()
    });
    window.on_tmdb_remove({
        let remote = remote.clone();
        move || remote.remove()
    });
    remote.render(window);
    remote.load();
}

impl Remote {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Runs `work` on a network worker, then `done` and a render on the UI
    /// thread.
    fn spawn<T: Send + 'static>(
        &self,
        work: impl FnOnce(&Remote) -> T + Send + 'static,
        done: impl FnOnce(&Remote, &AppWindow, T) + Send + 'static,
    ) {
        let remote = self.clone();
        self.network.run(move || {
            let value = work(&remote);
            let network = remote.network.clone();
            network.post(move || {
                if let Some(window) = remote.window.upgrade() {
                    done(&remote, &window, value);
                    remote.render(&window);
                }
            });
        });
    }

    // Discover

    fn input(&self, text: &str) {
        let mut state = self.state();
        state.add_error.clear();
        let fire = state.search.input(text);
        drop(state);
        if let Some(generation) = fire {
            self.fire_after(generation, DEBOUNCE);
        }
        self.refresh();
    }

    /// One single-shot timer per edit; only the latest generation searches.
    fn fire_after(&self, generation: u64, delay: Duration) {
        let remote = self.clone();
        Timer::single_shot(delay, move || {
            let requests = remote.state().search.fire(generation);
            remote.send(requests);
        });
    }

    fn load_more(&self) {
        let requests = self.state().search.load_more();
        self.send(requests);
    }

    fn send(&self, requests: Vec<Request>) {
        let token = self.state().token.clone();
        for request in requests {
            // The controller only asks while a token exists.
            let Some(token) = token.clone() else { break };
            self.spawn(
                move |remote| {
                    let started = Instant::now();
                    let result = remote.client.search(
                        &token,
                        request.media_type,
                        &request.query,
                        request.page,
                    );
                    (request, result, started.elapsed())
                },
                |remote, _, (request, result, took)| remote.searched(request, result, took),
            );
        }
        self.refresh();
    }

    fn searched(&self, request: Request, result: Result<SearchPage, TmdbError>, took: Duration) {
        let outcome = match &result {
            Ok(page) => format!("{} results", page.results.len()),
            Err(error) => error.to_string(),
        };
        let rejected = matches!(result, Err(TmdbError::CredentialInvalid));
        let mut state = self.state();
        let applied = state.search.apply(request.respond(result));
        if applied && rejected {
            state.credential = Credential::Invalid;
            state.note.clear();
        }
        drop(state);
        // No query text: it is the user's data and not needed to diagnose.
        self.log.info(format_args!(
            "TMDB search #{} {} page {}: {outcome} in {} ms{}",
            request.generation,
            request.media_type.key(),
            request.page,
            took.as_millis(),
            if applied { "" } else { " (stale, dropped)" }
        ));
    }

    fn select(&self, row: i32) {
        let mut state = self.state();
        let key = usize::try_from(row)
            .ok()
            .and_then(|row| Some(state.list.get(row)?.external.clone()));
        if key.is_some() {
            state.selected = key;
            state.picked = true;
            state.add_error.clear();
        }
        drop(state);
        self.refresh();
    }

    // Token

    fn load(&self) {
        self.spawn(
            |remote| match remote.secrets.load() {
                Ok(Some(token)) => {
                    let check = remote.client.validate(&token);
                    Loaded::Found(token, check)
                }
                Ok(None) => Loaded::Missing,
                Err(error) => Loaded::Unavailable(error),
            },
            |remote, _, loaded| remote.loaded(loaded),
        );
    }

    fn loaded(&self, loaded: Loaded) {
        let mut state = self.state();
        if state.credential != Credential::Loading {
            return; // The user already saved a token meanwhile.
        }
        match loaded {
            Loaded::Found(token, check) => {
                self.log.info(format_args!(
                    "TMDB token read from the credential store; check: {}",
                    describe(&check)
                ));
                state.credential = Credential::from_check(&check);
                state.token = Some(token.clone());
                let fire = state.search.set_has_token(true);
                drop(state);
                if check.is_ok() {
                    self.fetch_image_base(token);
                }
                if let Some(generation) = fire {
                    self.fire_after(generation, Duration::ZERO);
                }
            }
            Loaded::Missing => state.credential = Credential::Missing,
            Loaded::Unavailable(error) => {
                self.log.error(format_args!(
                    "The credential store could not be read: {error}"
                ));
                state.credential = Credential::StorageUnavailable;
            }
        }
    }

    /// Validates a pasted token with TMDB and saves it only if TMDB accepts
    /// it. Until then a previously saved token stays in use.
    fn save(&self, input: &str) {
        let Some(candidate) = Token::parse(input) else {
            self.state().note =
                "Paste the API Read Access Token from your TMDB account settings.".into();
            return self.refresh();
        };
        let mut state = self.state();
        state.credential = Credential::Checking;
        state.note.clear();
        drop(state);
        self.refresh();
        self.spawn(
            move |remote| match remote.client.validate(&candidate) {
                Ok(()) => {
                    let saved = remote.secrets.save(&candidate);
                    Saved::Accepted(candidate, saved)
                }
                Err(error) => Saved::Rejected(error),
            },
            |remote, window, saved| remote.saved(window, saved),
        );
    }

    fn saved(&self, window: &AppWindow, saved: Saved) {
        let mut state = self.state();
        match saved {
            Saved::Accepted(token, Ok(())) => {
                self.log
                    .info("TMDB token validated and saved in the credential store");
                state.token = Some(token.clone());
                state.credential = Credential::Valid;
                state.note.clear();
                window.set_tmdb_token_input(SharedString::new());
                let fire = state.search.set_has_token(true);
                drop(state);
                self.fetch_image_base(token);
                if let Some(generation) = fire {
                    self.fire_after(generation, Duration::ZERO);
                }
            }
            Saved::Accepted(_, Err(error)) => {
                self.log.error(format_args!(
                    "A valid TMDB token could not be saved: {error}"
                ));
                state.credential = Credential::StorageUnavailable;
                state.note = "TMDB accepted the token, but it was not saved.".into();
            }
            Saved::Rejected(error) => {
                self.log.info(format_args!("TMDB token not saved: {error}"));
                state.credential = Credential::from_check(&Err(error));
                state.note = match state.token {
                    Some(_) => "It was not saved; your previous token is still saved.",
                    None => "It was not saved.",
                }
                .into();
            }
        }
    }

    /// Asks `/3/configuration` for the image base URL, once per session.
    /// Posters use TMDB's documented default until (and unless) it answers.
    fn fetch_image_base(&self, token: Token) {
        let mut state = self.state();
        if std::mem::replace(&mut state.image_base_requested, true) {
            return;
        }
        drop(state);
        self.spawn(
            move |remote| remote.client.image_base(&token),
            |remote, _, base| match base {
                Ok(url) => remote.posters.set_base_url(url),
                Err(error) => {
                    remote.state().image_base_requested = false;
                    remote.log.info(format_args!(
                        "TMDB image configuration not loaded, using the default: {error}"
                    ));
                }
            },
        );
    }

    /// Adds the selected result to the library: one SQLite transaction. The
    /// list shows "In Library" only after it committed, as the database then
    /// reports it.
    fn add(&self) {
        let mut state = self.state();
        let chosen = state
            .selected
            .as_ref()
            .and_then(|key| state.list.iter().find(|r| &r.external == key).cloned());
        let Some(result) = chosen else {
            return;
        };
        let external = &result.external;
        let name = format!(
            "{}/{}/{}",
            external.source.key(),
            external.media_type.key(),
            external.id
        );
        match self
            .db
            .with(|db| library::add(db, &result, library::unix_now()))
        {
            Ok(added) => {
                let again = matches!(added, Added::AlreadyThere(_));
                self.log.info(format_args!(
                    "Library add {name}: {}",
                    if again { "already there" } else { "added" }
                ));
                state.add_error.clear();
                state.membership_stale = true;
                drop(state);
                if let Some(window) = self.window.upgrade() {
                    window.invoke_library_changed();
                    self.render(&window);
                }
            }
            Err(error) => {
                self.log
                    .error(format_args!("Library add {name} failed: {error}"));
                state.add_error = error.message;
                drop(state);
                self.refresh();
            }
        }
    }

    fn check(&self) {
        let Some(token) = self.state().token.clone() else {
            return;
        };
        let mut state = self.state();
        state.credential = Credential::Checking;
        state.note.clear();
        drop(state);
        self.refresh();
        self.spawn(
            move |remote| (remote.client.validate(&token), token),
            |remote, _, (check, token)| {
                remote
                    .log
                    .info(format_args!("TMDB token check: {}", describe(&check)));
                let mut state = remote.state();
                if state.token.as_ref() == Some(&token) {
                    state.credential = Credential::from_check(&check);
                }
            },
        );
    }

    /// Stops remote features at once, then deletes the stored token. The
    /// library and everything else stay as they are.
    fn remove(&self) {
        let mut state = self.state();
        state.token = None;
        state.credential = Credential::Missing;
        state.note.clear();
        state.selected = None;
        state.search.set_has_token(false);
        drop(state);
        self.refresh();
        self.spawn(
            |remote| remote.secrets.delete(),
            |remote, _, deleted| {
                let mut state = remote.state();
                match deleted {
                    Ok(()) => {
                        remote.log.info("TMDB token removed from the credential store");
                        state.note = "Removed. Your library is not affected.".into();
                    }
                    Err(error) => {
                        remote.log.error(format_args!("The TMDB token could not be removed: {error}"));
                        state.credential = Credential::StorageUnavailable;
                        state.note = "The stored token could not be deleted. It is not used until you save one again.".into();
                    }
                }
            },
        );
    }

    // Rendering

    fn refresh(&self) {
        if let Some(window) = self.window.upgrade() {
            self.render(&window);
        }
    }

    fn render(&self, window: &AppWindow) {
        let mut state = self.state();
        let results: Vec<MediaSearchResult> = state.search.results().into_iter().cloned().collect();
        let list_changed = results
            .iter()
            .map(|r| &r.external)
            .ne(state.list.iter().map(|r| &r.external));
        let mut rows_changed = list_changed;
        if list_changed || state.membership_stale {
            let refs: Vec<ExternalRef> = results.iter().map(|r| r.external.clone()).collect();
            // No library open (startup failed): nothing is "in library".
            let members = self
                .db
                .with(|db| library::membership(db, &refs))
                .unwrap_or_default();
            rows_changed |= members != state.in_library;
            state.in_library = members;
            state.membership_stale = false;
        }
        state.list = results;
        let keys: Vec<ExternalRef> = state.list.iter().map(|r| r.external.clone()).collect();
        // Results stream in per type, so an automatic selection follows the
        // top row; a user's choice stays while it is still listed.
        let picked = state
            .selected
            .take()
            .filter(|key| state.picked && keys.contains(key));
        state.picked = picked.is_some();
        state.selected = reselect(&keys, Clone::clone, picked);
        let selected = state
            .selected
            .as_ref()
            .and_then(|key| keys.iter().position(|k| k == key));
        let chosen = selected.map(|row| state.list[row].clone());
        let member = chosen
            .as_ref()
            .is_some_and(|r| state.in_library.contains(&r.external));
        let discover = discover_view(&state);
        let tmdb = tmdb_view(&state);
        // Slint may read the model while we set properties: no lock held.
        drop(state);
        window.set_discover(discover);
        window.set_tmdb(tmdb);
        window.set_discover_selected_row(selected.map_or(-1, |row| row as i32));
        window.set_discover_in_library(member);
        let detail = chosen.map(|r| self.row(&r, member)).unwrap_or_default();
        window.set_discover_detail(detail);
        if rows_changed {
            let model = window.get_discover_results();
            if let Some(model) = model.as_any().downcast_ref::<DiscoverModel>() {
                model.notify.reset();
            }
        }
    }

    fn row(&self, result: &MediaSearchResult, in_library: bool) -> MediaRow {
        let mut row = media_row(
            result.external.id.parse().unwrap_or(-1),
            result.external.media_type,
            &result.title,
            result.original_title.as_deref(),
            result.year(),
            result.overview.as_deref(),
        );
        if in_library {
            row.badge = "In Library".into();
        }
        let key = result.poster_path.as_deref().and_then(PosterKey::tmdb);
        with_poster(row, key, &self.posters)
    }
}

fn describe(check: &Result<(), TmdbError>) -> String {
    match check {
        Ok(()) => "valid".into(),
        Err(error) => error.to_string(),
    }
}

fn plural(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

fn discover_view(state: &State) -> DiscoverView {
    let message = |title: &str, message: String, action: &str| DiscoverView {
        state: "message".into(),
        title: title.into(),
        message: message.into(),
        action: action.into(),
        ..Default::default()
    };
    let search = &state.search;
    let view = if state.credential == Credential::Loading {
        message("Checking your TMDB access…", String::new(), "")
    } else {
        search_view(search, message)
    };
    let notice = match state.add_error.is_empty() {
        true => view.notice.clone(),
        false => state.add_error.as_str().into(),
    };
    DiscoverView {
        ready: state.token.is_some(),
        notice,
        ..view
    }
}

fn search_view(
    search: &SearchController,
    message: impl Fn(&str, String, &str) -> DiscoverView,
) -> DiscoverView {
    match search.status() {
        Status::NoCredential => message(
            "Connect TMDB to search",
            "Discover searches The Movie Database (TMDB). Add your TMDB API Read Access Token \
             in Settings to start."
                .into(),
            "settings",
        ),
        Status::Idle => message(
            "Search movies and TV series",
            "Type a title to search TMDB. Results are not added to your library.".into(),
            "",
        ),
        Status::Waiting | Status::Loading => DiscoverView {
            state: "loading".into(),
            title: "Searching TMDB…".into(),
            ..Default::default()
        },
        Status::NoResults => message(
            "No results",
            format!("No movies or TV series on TMDB match “{}”.", search.query()),
            "",
        ),
        Status::Failed(error) => {
            let (title, action) = match error {
                TmdbError::CredentialInvalid => ("TMDB rejected your token", "settings"),
                TmdbError::RateLimited => ("Too many requests", "retry"),
                TmdbError::Timeout | TmdbError::Offline(_) => ("Can't reach TMDB", "retry"),
                TmdbError::Server(_) => ("TMDB is having problems", "retry"),
                TmdbError::NotFound
                | TmdbError::MalformedResponse(_)
                | TmdbError::Unexpected(_) => ("Something went wrong", "retry"),
            };
            message(title, error.user_message().into(), action)
        }
        Status::Results => {
            let shown = search.results().len();
            let total = search.total_results() as usize;
            DiscoverView {
                state: "results".into(),
                summary: match total > shown {
                    true => format!("Showing {shown} of {}", plural(total, "result")),
                    false => plural(shown, "result"),
                }
                .into(),
                notice: search
                    .partial_failure()
                    .map(|(kind, error)| {
                        format!(
                            "{} results could not be loaded. {}",
                            kind.label(),
                            error.user_message()
                        )
                    })
                    .unwrap_or_default()
                    .into(),
                can_load_more: search.can_load_more(),
                loading_more: search.loading_more(),
                ..Default::default()
            }
        }
    }
}

fn tmdb_view(state: &State) -> TmdbView {
    let hint = state.token.as_ref().map(Token::hint).unwrap_or_default();
    let (status, detail, tone) = match &state.credential {
        Credential::Loading => ("Reading the saved token…", String::new(), ""),
        Credential::Missing => (
            "Not connected",
            "Discover needs a TMDB API Read Access Token. Create a free TMDB account, then copy \
             the token from themoviedb.org → Settings → API."
                .into(),
            "",
        ),
        Credential::Checking => ("Checking the token with TMDB…", String::new(), ""),
        Credential::Valid => (
            "Connected to TMDB",
            format!("Using the saved token ending in {hint}, kept in {STORE_NAME}."),
            "ok",
        ),
        Credential::Invalid => (
            "TMDB rejected the token",
            "Check that you copied the API Read Access Token, not the API key.".into(),
            "error",
        ),
        Credential::Offline => (
            "Can't reach TMDB",
            "The token could not be checked. Check your internet connection.".into(),
            "warning",
        ),
        Credential::Trouble(error) => ("TMDB problem", error.user_message().into(), "warning"),
        Credential::StorageUnavailable => (
            "Secure storage unavailable",
            format!("Bingee Desktop keeps the token only in {STORE_NAME}, which is not available."),
            "error",
        ),
    };
    let detail = format!("{detail} {}", state.note);
    TmdbView {
        status: status.into(),
        detail: detail.trim().into(),
        tone: tone.into(),
        has_token: state.token.is_some(),
        busy: matches!(state.credential, Credential::Loading | Credential::Checking),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::MemoryStore;
    use crate::tests::Headless;
    use crate::tmdb::fake::{FakeServer, Reply, page};

    const SUCCESS: &str = r#"{"success":true,"status_code":1,"status_message":"Success."}"#;
    const INVALID: &str = r#"{"success":false,"status_code":7,"status_message":"Invalid API key: You must be granted a valid key."}"#;
    const GOOD: &str = "good-token-5f1e";

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// A TMDB that accepts only `GOOD`. Every search has two pages of one
    /// result per type; the query "dar" answers slowly.
    fn tmdb() -> FakeServer {
        FakeServer::start(|seen| {
            if seen.authorization.as_deref() != Some(&format!("Bearer {GOOD}")) {
                return Reply::json(401, INVALID);
            }
            if seen.target.starts_with("/3/authentication") {
                return Reply::json(200, SUCCESS);
            }
            let kind = match seen.target.starts_with("/3/search/movie") {
                true => "movie",
                false => "tv",
            };
            let number: u32 = seen
                .target
                .split("page=")
                .nth(1)
                .and_then(|rest| rest.split('&').next()?.parse().ok())
                .unwrap();
            let slow = seen.target.contains("query=dar&");
            let title = format!("{} {kind} {number}", if slow { "Old" } else { "Dark" });
            let reply = Reply::json(
                200,
                &page(kind, number, 2, &[(100 + u64::from(number), &title)]),
            );
            match slow {
                true => reply.after(ms(600)),
                false => reply,
            }
        })
    }

    /// Starts Discover and the TMDB settings on `ui` against `base`, with the
    /// library database `db` (open or not) and posters under `posters`.
    fn remote_with(
        ui: &Headless,
        base: &str,
        store: &Arc<MemoryStore>,
        db: SharedDb,
        posters: Option<std::path::PathBuf>,
    ) -> Arc<Posters> {
        let client = TmdbClient::for_tests(base, Duration::from_secs(5));
        let log = Arc::new(Log::stderr_only());
        let network = Network::with_post(4, ui.post());
        let posters = crate::posters(
            &ui.app,
            posters,
            client.clone(),
            network.clone(),
            log.clone(),
        );
        // Images from the same fake server, never from the real TMDB.
        posters.set_base_url(format!("{base}/t/p/"));
        start(
            &ui.app,
            client,
            store.clone(),
            network,
            log,
            db,
            posters.clone(),
        );
        posters
    }

    fn remote(ui: &Headless, base: &str, store: &Arc<MemoryStore>) {
        remote_with(ui, base, store, SharedDb::default(), None);
    }

    fn titles(app: &AppWindow) -> Vec<String> {
        app.get_discover_results()
            .iter()
            .map(|row| row.title.to_string())
            .collect()
    }

    fn searches(server: &FakeServer, query: &str) -> usize {
        let needle = format!("query={query}&");
        server
            .seen()
            .iter()
            .filter(|s| s.target.contains(&needle))
            .count()
    }

    #[test]
    fn discover_debounces_drops_stale_answers_and_pages() {
        let server = tmdb();
        let mut ui = Headless::new(1280, 800);
        remote(&ui, &server.base, &Arc::new(MemoryStore::with(GOOD)));
        ui.pump_until("the saved token check", |app| {
            app.get_tmdb().status == "Connected to TMDB"
        });
        let app = ui.app.clone_strong();
        app.set_page("discover".into());
        assert!(app.get_discover().ready);

        // Three quick edits: nothing is sent while the user types.
        for text in ["d", "da", "dar"] {
            app.invoke_discover_query_changed(text.into());
            ui.advance(ms(100));
        }
        assert_eq!(app.get_discover().state, "loading");
        let typed = ["d", "da", "dar"].map(|q| searches(&server, q));
        assert_eq!(typed, [0, 0, 0]);
        // 300 ms after the last edit: one search, movie and TV.
        ui.advance(ms(200));
        ui.pump_until("the slow dar requests", |_| searches(&server, "dar") == 2);

        // "dark" is answered while "dar" is still on its way.
        app.invoke_discover_query_changed("dark".into());
        ui.advance(ms(300));
        ui.pump_until("the dark results", |app| titles(app).len() == 2);
        assert_eq!(titles(&app), ["Dark movie 1", "Dark tv 1"]);
        // The late "dar" answers arrive and are dropped.
        std::thread::sleep(ms(900));
        ui.pump();
        ui.render();
        assert_eq!(titles(&app), ["Dark movie 1", "Dark tv 1"]);
        let typed = ["d", "da"].map(|q| searches(&server, q));
        assert_eq!(typed, [0, 0], "debounced");
        let view = app.get_discover();
        assert_eq!(view.state, "results");
        assert_eq!(view.summary, "Showing 2 of 80 results");
        assert_eq!(app.get_discover_selected_row(), 0);
        assert_eq!(app.get_discover_detail().meta, "Movie · 2016");

        // Page 2 of both types, appended; then there is nothing more.
        assert!(view.can_load_more);
        app.invoke_discover_load_more();
        ui.pump_until("page 2", |app| titles(app).len() == 4);
        let expected = ["Dark movie 1", "Dark tv 1", "Dark movie 2", "Dark tv 2"];
        assert_eq!(titles(&app), expected);
        assert!(!app.get_discover().can_load_more);
        app.invoke_discover_row_selected(3);
        assert_eq!(app.get_discover_detail().title, "Dark tv 2");

        // Clearing the query clears the results and sends nothing.
        let sent = server.seen().len();
        app.invoke_discover_query_changed("   ".into());
        ui.advance(ms(400));
        assert_eq!((titles(&app).len(), server.seen().len()), (0, sent));
        assert_eq!(app.get_discover().title, "Search movies and TV series");
        ui.render();
    }

    #[test]
    fn token_lifecycle_never_saves_a_rejected_token() {
        let server = tmdb();
        let store = Arc::new(MemoryStore::default());
        let mut ui = Headless::new(1280, 800);
        remote(&ui, &server.base, &store);
        ui.pump_until("the empty store", |app| {
            app.get_tmdb().status == "Not connected"
        });
        let app = ui.app.clone_strong();
        assert_eq!(app.get_discover().title, "Connect TMDB to search");
        assert_eq!(app.get_discover().action, "settings");
        assert!(!app.get_discover().ready);

        app.invoke_tmdb_save("bad-token".into());
        assert!(app.get_tmdb().busy);
        ui.pump_until("the rejection", |app| {
            app.get_tmdb().status == "TMDB rejected the token"
        });
        assert_eq!(store.current(), None);
        assert!(app.get_tmdb().detail.ends_with("It was not saved."));

        app.set_tmdb_token_input(GOOD.into());
        app.invoke_tmdb_save(GOOD.into());
        ui.pump_until("the accepted token", |app| {
            app.get_tmdb().status == "Connected to TMDB"
        });
        let view = app.get_tmdb();
        assert_eq!(store.current().as_deref(), Some(GOOD));
        assert_eq!(app.get_tmdb_token_input(), "", "the field is cleared");
        assert!(view.has_token && view.detail.contains("…5f1e"));
        assert!(!view.detail.contains(GOOD));
        assert!(app.get_discover().ready);

        // Replacing it with a rejected token keeps the saved one.
        app.invoke_tmdb_save("worse-token".into());
        ui.pump_until("the second rejection", |app| {
            app.get_tmdb().status == "TMDB rejected the token"
        });
        assert_eq!(store.current().as_deref(), Some(GOOD));
        assert!(
            app.get_tmdb()
                .detail
                .contains("previous token is still saved")
        );

        // Removing disables Discover at once and deletes the stored token.
        app.set_page("settings".into());
        ui.render();
        app.invoke_tmdb_remove();
        assert_eq!(app.get_tmdb().status, "Not connected");
        assert_eq!(app.get_discover().title, "Connect TMDB to search");
        ui.pump_until("the deletion", |app| {
            app.get_tmdb().detail.contains("Removed.")
        });
        assert_eq!(store.current(), None);
        ui.render();
    }

    #[test]
    fn an_unavailable_credential_store_is_reported_and_nothing_is_kept() {
        let server = tmdb();
        let store = Arc::new(MemoryStore::default());
        store
            .failing
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let ui = Headless::new(1280, 800);
        remote(&ui, &server.base, &store);
        ui.pump_until("the failed read", |app| {
            app.get_tmdb().status == "Secure storage unavailable"
        });
        let app = ui.app.clone_strong();
        app.invoke_tmdb_save(GOOD.into());
        ui.pump_until("the failed save", |app| {
            app.get_tmdb()
                .detail
                .ends_with("TMDB accepted the token, but it was not saved.")
        });
        let view = app.get_tmdb();
        assert_eq!(view.status, "Secure storage unavailable");
        assert!(!view.has_token, "an unsaved token is not used");
        assert_eq!(app.get_discover().title, "Connect TMDB to search");
    }

    #[test]
    fn a_revoked_token_is_reported_by_settings_and_discover() {
        let server = tmdb();
        let ui = Headless::new(1280, 800);
        remote(
            &ui,
            &server.base,
            &Arc::new(MemoryStore::with("revoked-token")),
        );
        ui.pump_until("the startup check", |app| {
            app.get_tmdb().status == "TMDB rejected the token"
        });
        let app = ui.app.clone_strong();
        app.invoke_discover_query_changed("dark".into());
        ui.advance(ms(300));
        ui.pump_until("the failed search", |app| {
            app.get_discover().title == "TMDB rejected your token"
        });
        assert_eq!(app.get_discover().action, "settings");
    }

    #[test]
    fn offline_discover_leaves_the_library_working() {
        use crate::library::tests::add;
        use crate::paths::{AppPaths, TestDir};
        use crate::settings::Settings;

        let dir = TestDir::new("offline");
        let settings = Settings {
            home_override: Some(dir.0.clone()),
        };
        let paths = AppPaths::from_env(&settings).unwrap();
        paths.create_dirs().unwrap();
        let db = crate::database::Database::open(&paths.database(), &Log::stderr_only()).unwrap();
        add(&db, "movie", "Arrival", None, true);
        drop(db);
        let before = std::fs::read(paths.database()).unwrap();
        let closed = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();

        let mut ui = Headless::new(1280, 800);
        let app = ui.app.clone_strong();
        let db = SharedDb::default();
        let store = Arc::new(MemoryStore::with(GOOD));
        let base = format!("http://{closed}");
        let posters = remote_with(&ui, &base, &store, db.clone(), None);
        crate::start(
            &app,
            Ok(paths.clone()),
            Arc::new(Log::stderr_only()),
            db,
            posters,
        );
        assert_eq!(app.get_total_count(), 1);

        // While the startup check waits for a connection that never comes,
        // the UI thread still serves the library at once.
        let started = Instant::now();
        app.invoke_query_changed("arr".into());
        ui.render();
        assert!(started.elapsed() < ms(500), "{:?}", started.elapsed());
        assert_eq!(app.get_results().row_count(), 1);

        ui.pump_until("the offline check", |app| {
            app.get_tmdb().status == "Can't reach TMDB"
        });
        app.set_page("discover".into());
        app.invoke_discover_query_changed("dark".into());
        ui.advance(ms(300));
        ui.pump_until("the offline search", |app| {
            app.get_discover().title == "Can't reach TMDB"
        });
        assert_eq!(app.get_discover().action, "retry");
        ui.render();

        app.set_page("library".into());
        app.invoke_query_changed("".into());
        assert_eq!(app.get_results().row_count(), 1);
        assert_eq!(app.get_startup_error(), "");
        assert_eq!(app.get_detail().title, "Arrival");
        ui.render();
        let after = std::fs::read(paths.database()).unwrap();
        assert_eq!(after, before, "the database is untouched");
    }

    /// TMDB with a search for "x": movie 603 and TV 603 share an id, movie
    /// 604 has no poster, TV 605's poster is missing (404). Posters and the
    /// configuration are served too; everything but images needs `GOOD`.
    fn journey_tmdb() -> FakeServer {
        let jpeg = crate::poster::tests::jpeg(185, 278, 80);
        FakeServer::start(move |seen| {
            if seen.target.starts_with("/t/p/w185/") {
                return match seen.target.as_str() {
                    "/t/p/w185/matrix.jpg" | "/t/p/w185/show.jpg" => {
                        Reply::bytes(200, jpeg.clone())
                    }
                    _ => Reply::json(404, r#"{"status_code":34}"#),
                };
            }
            if seen.authorization.as_deref() != Some(&format!("Bearer {GOOD}")) {
                return Reply::json(401, INVALID);
            }
            match seen.target.split('?').next().unwrap() {
                "/3/authentication" => Reply::json(200, SUCCESS),
                "/3/search/movie" => Reply::json(
                    200,
                    r#"{"page":1,"total_pages":1,"total_results":2,"results":[
                    {"id":603,"title":"The Matrix","release_date":"1999-03-31","overview":"A hacker learns the truth.","poster_path":"/matrix.jpg"},
                    {"id":604,"title":"No Poster","release_date":"2003-05-15","overview":"","poster_path":null}]}"#,
                ),
                "/3/search/tv" => Reply::json(
                    200,
                    r#"{"page":1,"total_pages":1,"total_results":2,"results":[
                    {"id":603,"name":"Some Show","first_air_date":"2010-01-01","overview":"Same number, other title.","poster_path":"/show.jpg"},
                    {"id":605,"name":"Broken Poster","first_air_date":"2011-01-01","overview":"","poster_path":"/missing.jpg"}]}"#,
                ),
                _ => Reply::json(404, "{}"),
            }
        })
    }

    fn row_titles(model: slint::ModelRc<MediaRow>) -> Vec<String> {
        model.iter().map(|row| row.title.to_string()).collect()
    }

    fn db_counts(path: &std::path::Path) -> [i64; 3] {
        let conn = rusqlite::Connection::open(path).unwrap();
        ["media", "external_refs", "library_entries"].map(|t| {
            conn.query_row(&format!("SELECT count(*) FROM {t}"), [], |r| r.get(0))
                .unwrap()
        })
    }

    /// The R8 journey: search, add a movie and a series (same TMDB id), add
    /// again, let posters cache, restart offline: the library still works.
    #[test]
    fn search_add_restart_offline() {
        use crate::paths::{AppPaths, TestDir};
        use crate::settings::Settings;

        let server = journey_tmdb();
        let dir = TestDir::new("journey");
        let settings = Settings {
            home_override: Some(dir.0.clone()),
        };
        let paths = AppPaths::from_env(&settings).unwrap();
        let poster_dir = paths.cache.join("posters");

        // Session 1, online. Each session gets its own UI thread.
        let (online_paths, base) = (paths.clone(), server.base.clone());
        std::thread::spawn(move || {
            let mut ui = Headless::new(1280, 800);
            let app = ui.app.clone_strong();
            let db = SharedDb::default();
            let store = Arc::new(MemoryStore::with(GOOD));
            let poster_dir = online_paths.cache.join("posters");
            let posters = remote_with(&ui, &base, &store, db.clone(), Some(poster_dir.clone()));
            crate::start(
                &app,
                Ok(online_paths),
                Arc::new(Log::stderr_only()),
                db,
                posters.clone(),
            );
            ui.pump_until("the token check", |app| {
                app.get_tmdb().status == "Connected to TMDB"
            });
            assert_eq!(app.get_total_count(), 0);

            app.set_page("discover".into());
            app.invoke_discover_query_changed("x".into());
            ui.advance(ms(300));
            ui.pump_until("the results", |app| {
                app.get_discover_results().row_count() == 4
            });
            assert_eq!(
                row_titles(app.get_discover_results()),
                ["The Matrix", "Some Show", "No Poster", "Broken Poster"]
            );
            assert!(
                app.get_discover_results()
                    .iter()
                    .all(|row| row.badge.is_empty())
            );

            // Add all four; the first one twice.
            for row in [0, 1, 2, 3, 0] {
                app.invoke_discover_row_selected(row);
                app.invoke_discover_add();
                assert!(app.get_discover_in_library(), "row {row}");
            }
            assert_eq!(app.get_total_count(), 4, "the second add changed nothing");
            let badges: Vec<String> = app
                .get_discover_results()
                .iter()
                .map(|r| r.badge.into())
                .collect();
            assert_eq!(badges, ["In Library"; 4]);
            assert_eq!(app.get_discover().notice, "");

            // The Library page shows them without a restart, newest first.
            app.set_page("library".into());
            ui.render();
            assert_eq!(
                row_titles(app.get_results()),
                ["Broken Poster", "No Poster", "Some Show", "The Matrix"]
            );
            ui.pump_until("two posters on disk", |_| {
                ["tmdb-w185-matrix.jpg", "tmdb-w185-show.jpg"]
                    .iter()
                    .all(|name| poster_dir.join(name).is_file())
            });
            ui.pump_until("the posters in the rows", |app| {
                app.get_results()
                    .iter()
                    .filter(|row| row.poster.size().width > 0)
                    .count()
                    == 2
            });
            let stats = posters.stats();
            assert_eq!(stats.downloads, 2, "{stats:?}");
            ui.render();
        })
        .join()
        .unwrap();
        for name in ["tmdb-w185-matrix.jpg", "tmdb-w185-show.jpg"] {
            assert!(poster_dir.join(name).is_file(), "{name}");
        }
        assert_eq!(db_counts(&paths.database()), [4, 4, 4]);
        let before = server.seen().len();

        // Session 2: TMDB is unreachable.
        let closed = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap();
        let offline_paths = paths.clone();
        std::thread::spawn(move || {
            let mut ui = Headless::new(1280, 800);
            let app = ui.app.clone_strong();
            let db = SharedDb::default();
            let store = Arc::new(MemoryStore::with(GOOD));
            let base = format!("http://{closed}");
            let posters = remote_with(
                &ui,
                &base,
                &store,
                db.clone(),
                Some(offline_paths.cache.join("posters")),
            );
            crate::start(
                &app,
                Ok(offline_paths),
                Arc::new(Log::stderr_only()),
                db,
                posters.clone(),
            );
            // The library is there at once, from SQLite alone.
            assert_eq!(app.get_total_count(), 4);
            assert_eq!(app.get_startup_error(), "");
            ui.render();
            ui.pump_until("the cached posters", |app| {
                app.get_results()
                    .iter()
                    .filter(|row| row.poster.size().width > 0)
                    .count()
                    == 2
            });
            let stats = posters.stats();
            assert_eq!((stats.disk_loads, stats.downloads), (2, 0), "{stats:?}");
            let placeholders: Vec<String> = app
                .get_results()
                .iter()
                .filter(|row| row.poster.size().width == 0)
                .map(|row| row.title.into())
                .collect();
            assert_eq!(placeholders, ["Broken Poster", "No Poster"]);

            // Local search, filter and sort.
            app.invoke_query_changed("matrix".into());
            assert_eq!(row_titles(app.get_results()), ["The Matrix"]);
            app.invoke_query_changed("".into());
            app.invoke_library_options_changed("tv".into(), 1);
            assert_eq!(
                row_titles(app.get_results()),
                ["Broken Poster", "Some Show"]
            );
            app.invoke_library_options_changed("all".into(), 1);
            assert_eq!(
                row_titles(app.get_results()),
                ["Broken Poster", "No Poster", "Some Show", "The Matrix"]
            );

            // Remove works offline and keeps the metadata.
            app.invoke_row_selected(3);
            assert_eq!(app.get_detail().title, "The Matrix");
            app.invoke_library_remove();
            assert_eq!(app.get_total_count(), 3);
            assert_eq!(
                row_titles(app.get_results()),
                ["Broken Poster", "No Poster", "Some Show"]
            );

            // Discover says TMDB is out of reach.
            ui.pump_until("the offline check", |app| {
                app.get_tmdb().status == "Can't reach TMDB"
            });
            app.set_page("discover".into());
            app.invoke_discover_query_changed("x".into());
            ui.advance(ms(300));
            ui.pump_until("the offline search", |app| {
                app.get_discover().title == "Can't reach TMDB"
            });
            ui.render();
        })
        .join()
        .unwrap();
        assert_eq!(
            db_counts(&paths.database()),
            [4, 4, 3],
            "remove kept metadata and identity"
        );
        assert_eq!(server.seen().len(), before, "session 2 never reached TMDB");
    }

    #[test]
    fn membership_follows_adds_and_removes_in_one_query_per_result_set() {
        use crate::database::Database;

        let server = journey_tmdb();
        let mut ui = Headless::new(1280, 800);
        let app = ui.app.clone_strong();
        let db = SharedDb::default();
        db.set(Database::open_in_memory());
        // Already in the library before the search: the TV series 603 only.
        let tv = crate::search::tests::item(crate::library::MediaType::Tv, 603, "Some Show");
        db.with(|db| library::add(db, &tv, 1)).unwrap();
        let store = Arc::new(MemoryStore::with(GOOD));
        remote_with(&ui, &server.base, &store, db.clone(), None);
        ui.pump_until("the token check", |app| {
            app.get_tmdb().status == "Connected to TMDB"
        });
        app.invoke_discover_query_changed("x".into());
        ui.advance(ms(300));
        ui.pump_until("the results", |app| {
            app.get_discover_results().row_count() == 4
        });
        let badges = |app: &AppWindow| -> Vec<String> {
            app.get_discover_results()
                .iter()
                .map(|r| r.badge.into())
                .collect()
        };
        // Movie 603 is not TV 603.
        assert_eq!(badges(&app), ["", "In Library", "", ""]);
        assert!(!app.get_discover_in_library(), "the movie is selected");

        // Removing it elsewhere (the Library page) is reflected here.
        let id = db.with(|db| library::add(db, &tv, 2)).unwrap();
        let crate::library::Added::AlreadyThere(id) = id else {
            panic!()
        };
        db.with(|db| library::remove(db, id)).unwrap();
        app.invoke_membership_changed();
        assert_eq!(badges(&app), ["", "", "", ""]);
        ui.render();
    }

    #[test]
    fn a_failed_add_keeps_the_result_and_says_why() {
        let server = journey_tmdb();
        let ui = Headless::new(1280, 800);
        let app = ui.app.clone_strong();
        // No library open (as after a startup failure).
        let store = Arc::new(MemoryStore::with(GOOD));
        remote_with(&ui, &server.base, &store, SharedDb::default(), None);
        ui.pump_until("the token check", |app| {
            app.get_tmdb().status == "Connected to TMDB"
        });
        app.invoke_discover_query_changed("x".into());
        ui.advance(ms(300));
        ui.pump_until("the results", |app| {
            app.get_discover_results().row_count() == 4
        });
        app.invoke_discover_add();
        assert!(!app.get_discover_in_library());
        assert_eq!(app.get_discover().notice, "Your library is not open.");
        assert_eq!(app.get_discover_results().row_count(), 4, "results kept");
    }

    #[test]
    fn discover_is_usable_from_the_keyboard() {
        use crate::database::Database;
        use slint::LogicalPosition;
        use slint::platform::{Key, PointerEventButton, WindowEvent};

        let server = journey_tmdb();
        let mut ui = Headless::new(1280, 800);
        let app = ui.app.clone_strong();
        let db = SharedDb::default();
        db.set(Database::open_in_memory());
        let store = Arc::new(MemoryStore::with(GOOD));
        remote_with(&ui, &server.base, &store, db.clone(), None);
        ui.pump_until("the token check", |app| {
            app.get_tmdb().status == "Connected to TMDB"
        });
        app.set_page("discover".into());
        app.invoke_discover_query_changed("x".into());
        ui.advance(ms(300));
        ui.pump_until("the results", |app| {
            app.get_discover_results().row_count() == 4
        });
        ui.render();

        let window = app.window();
        let position = LogicalPosition::new(500.0, 160.0); // the first row
        let button = PointerEventButton::Left;
        window.dispatch_event(WindowEvent::PointerPressed { position, button });
        window.dispatch_event(WindowEvent::PointerReleased { position, button });
        assert_eq!(app.get_discover_selected_row(), 0);
        let press = |key: Key| {
            let text: SharedString = key.into();
            window.dispatch_event(WindowEvent::KeyPressed { text: text.clone() });
            window.dispatch_event(WindowEvent::KeyReleased { text });
        };
        press(Key::DownArrow);
        assert_eq!(app.get_discover_selected_row(), 1);
        assert_eq!(app.get_discover_detail().title, "Some Show");
        press(Key::Return);
        assert!(
            app.get_discover_in_library(),
            "Enter adds the selected result"
        );
        assert_eq!(db.with(library::count).unwrap(), 1);
        press(Key::End);
        assert_eq!(app.get_discover_selected_row(), 3);
    }
}
