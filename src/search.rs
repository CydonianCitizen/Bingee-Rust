//! Remote search: the provider-independent result type and the state machine
//! behind Discover. No Slint, threads or clock: events and responses arrive as
//! calls, so every rule is tested deterministically (ADR-0008).
//!
//! Policy: each query searches movies and TV. Every edit starts a new
//! generation. Debounce: each edit schedules a timer for its generation, and
//! only the timer of the latest edit sends the search. A response is applied
//! only for the current generation and the page that is still pending, so late
//! or duplicate responses are dropped.
//! The two types page independently. The list interleaves them by position
//! (movie 1, TV 1, movie 2, ...), a pure function of what has loaded, so the
//! order does not depend on which response arrived first.

use std::collections::HashSet;
use std::time::Duration;

use crate::library::MediaType;
use crate::tmdb::TmdbError;

/// Quiet time after the last edit before a search is sent.
pub const DEBOUNCE: Duration = Duration::from_millis(300);
/// TMDB refuses pages above 500 (error 22).
pub const MAX_PAGE: u32 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Source {
    Tmdb,
}

/// A title's identity at a provider. The media type is part of it: TMDB movie
/// 123 and TMDB TV 123 are different titles.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExternalRef {
    pub source: Source,
    pub media_type: MediaType,
    pub id: String,
}

/// One remote search result. Transient in R7: nothing is stored.
#[derive(Debug, Clone, PartialEq)]
pub struct MediaSearchResult {
    pub external: ExternalRef,
    pub title: String,
    pub original_title: Option<String>,
    /// `YYYY-MM-DD` (for TV, the first air date).
    pub release_date: Option<String>,
    pub overview: Option<String>,
    /// Provider image paths (for TMDB, relative to its image base URL).
    pub poster_path: Option<String>,
    pub backdrop_path: Option<String>,
}

impl MediaSearchResult {
    pub fn year(&self) -> Option<&str> {
        self.release_date.as_deref().and_then(|date| date.get(..4))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchPage {
    pub page: u32,
    pub total_pages: u32,
    pub total_results: u32,
    pub results: Vec<MediaSearchResult>,
}

/// A request the controller wants made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub generation: u64,
    pub query: String,
    pub media_type: MediaType,
    pub page: u32,
}

impl Request {
    pub fn respond(&self, result: Result<SearchPage, TmdbError>) -> Response {
        Response {
            generation: self.generation,
            media_type: self.media_type,
            page: self.page,
            result,
        }
    }
}

#[derive(Debug)]
pub struct Response {
    pub generation: u64,
    pub media_type: MediaType,
    pub page: u32,
    pub result: Result<SearchPage, TmdbError>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    NoCredential,
    /// Blank query.
    Idle,
    /// The debounce is running.
    Waiting,
    /// The first pages are on their way.
    Loading,
    Results,
    NoResults,
    /// Nothing to show because the requests failed.
    Failed(TmdbError),
}

/// One media type's pages for the current query.
#[derive(Debug, Default)]
struct Side {
    loaded: u32,
    total_pages: u32,
    total_results: u32,
    results: Vec<MediaSearchResult>,
    pending: Option<u32>,
    error: Option<TmdbError>,
}

impl Side {
    /// The page "Load more" / "Try again" would fetch, if any.
    fn next_page(&self) -> Option<u32> {
        let more = self.error.is_some()
            || self.loaded == 0
            || self.loaded < self.total_pages.min(MAX_PAGE);
        (self.pending.is_none() && more).then_some(self.loaded + 1)
    }
}

#[derive(Debug)]
pub struct SearchController {
    has_token: bool,
    query: String,
    generation: u64,
    /// A debounce timer for the current generation is running.
    waiting: bool,
    movie: Side,
    tv: Side,
    /// Identities already listed for this query, for deduplication.
    seen: HashSet<ExternalRef>,
}

impl SearchController {
    pub fn new(has_token: bool) -> Self {
        Self {
            has_token,
            query: String::new(),
            generation: 0,
            waiting: false,
            movie: Side::default(),
            tv: Side::default(),
            seen: HashSet::new(),
        }
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    /// Drops everything in flight. Called when a token appears or goes away.
    /// Returns the generation to fire at once when a token appears while a
    /// query is typed.
    pub fn set_has_token(&mut self, has_token: bool) -> Option<u64> {
        self.has_token = has_token;
        self.restart();
        self.schedule()
    }

    /// A search field edit. Surrounding whitespace is ignored; a blank query
    /// cancels the search and sends nothing. Returns the generation to fire
    /// after `DEBOUNCE`, if the edit needs a search.
    pub fn input(&mut self, text: &str) -> Option<u64> {
        let query = text.trim();
        if query == self.query {
            return None;
        }
        self.query = query.to_owned();
        self.restart();
        self.schedule()
    }

    fn restart(&mut self) {
        self.generation += 1;
        self.waiting = false;
        self.movie = Side::default();
        self.tv = Side::default();
        self.seen.clear();
    }

    fn schedule(&mut self) -> Option<u64> {
        self.waiting = self.has_token && !self.query.is_empty();
        self.waiting.then_some(self.generation)
    }

    /// A debounce timer fired: the first page of both types, if no edit came
    /// after the one that started this timer.
    pub fn fire(&mut self, generation: u64) -> Vec<Request> {
        if !self.waiting || generation != self.generation {
            return Vec::new();
        }
        self.waiting = false;
        vec![
            self.request(MediaType::Movie, 1),
            self.request(MediaType::Tv, 1),
        ]
    }

    /// "Load more" and "Try again": the next page of each type that has one,
    /// and a retry of each type whose last request failed.
    pub fn load_more(&mut self) -> Vec<Request> {
        if !self.has_token || self.query.is_empty() || self.waiting {
            return Vec::new();
        }
        [MediaType::Movie, MediaType::Tv]
            .into_iter()
            .filter_map(|kind| Some(self.request(kind, self.side(kind).next_page()?)))
            .collect()
    }

    fn request(&mut self, media_type: MediaType, page: u32) -> Request {
        self.side_mut(media_type).pending = Some(page);
        Request {
            generation: self.generation,
            query: self.query.clone(),
            media_type,
            page,
        }
    }

    fn side(&self, kind: MediaType) -> &Side {
        match kind {
            MediaType::Movie => &self.movie,
            MediaType::Tv => &self.tv,
        }
    }

    fn side_mut(&mut self, kind: MediaType) -> &mut Side {
        match kind {
            MediaType::Movie => &mut self.movie,
            MediaType::Tv => &mut self.tv,
        }
    }

    /// Applies a response if it answers the pending page of the current
    /// generation; returns whether it did.
    pub fn apply(&mut self, response: Response) -> bool {
        let Self {
            generation,
            movie,
            tv,
            seen,
            ..
        } = self;
        let side = match response.media_type {
            MediaType::Movie => movie,
            MediaType::Tv => tv,
        };
        if response.generation != *generation || side.pending != Some(response.page) {
            return false;
        }
        side.pending = None;
        match response.result {
            Ok(page) => {
                side.loaded = response.page;
                side.total_pages = page.total_pages;
                side.total_results = page.total_results;
                side.error = None;
                let fresh = page.results.into_iter().filter(|result| {
                    result.external.media_type == response.media_type
                        && seen.insert(result.external.clone())
                });
                side.results.extend(fresh);
            }
            Err(error) => side.error = Some(error),
        }
        true
    }

    /// Movies and TV interleaved by position.
    pub fn results(&self) -> Vec<&MediaSearchResult> {
        let (movies, tv) = (&self.movie.results, &self.tv.results);
        (0..movies.len().max(tv.len()))
            .flat_map(|i| [movies.get(i), tv.get(i)])
            .flatten()
            .collect()
    }

    pub fn status(&self) -> Status {
        if !self.has_token {
            return Status::NoCredential;
        }
        if self.query.is_empty() {
            return Status::Idle;
        }
        if self.waiting {
            return Status::Waiting;
        }
        if !self.movie.results.is_empty() || !self.tv.results.is_empty() {
            return Status::Results;
        }
        if self.movie.pending.is_some() || self.tv.pending.is_some() {
            return Status::Loading;
        }
        match self.error() {
            Some(error) => Status::Failed(error.clone()),
            None => Status::NoResults,
        }
    }

    /// The error to report: a rejected token wins, since it explains the rest.
    fn error(&self) -> Option<&TmdbError> {
        let errors = [&self.movie.error, &self.tv.error];
        let mut errors = errors.into_iter().flatten();
        let first = errors.clone().next();
        errors
            .find(|e| **e == TmdbError::CredentialInvalid)
            .or(first)
    }

    /// A type that failed while the other one has results.
    pub fn partial_failure(&self) -> Option<(MediaType, &TmdbError)> {
        if self.status() != Status::Results {
            return None;
        }
        [MediaType::Movie, MediaType::Tv]
            .into_iter()
            .find_map(|kind| Some((kind, self.side(kind).error.as_ref()?)))
    }

    /// Results that exist for this query at TMDB, both types together.
    pub fn total_results(&self) -> u32 {
        self.movie.total_results + self.tv.total_results
    }

    pub fn loading_more(&self) -> bool {
        self.status() == Status::Results
            && (self.movie.pending.is_some() || self.tv.pending.is_some())
    }

    pub fn can_load_more(&self) -> bool {
        self.status() == Status::Results
            && !self.loading_more()
            && [&self.movie, &self.tv]
                .iter()
                .any(|side| side.next_page().is_some())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    pub fn item(kind: MediaType, id: u32, title: &str) -> MediaSearchResult {
        MediaSearchResult {
            external: ExternalRef {
                source: Source::Tmdb,
                media_type: kind,
                id: id.to_string(),
            },
            title: title.to_owned(),
            original_title: None,
            release_date: Some("2017-12-01".into()),
            overview: None,
            poster_path: None,
            backdrop_path: None,
        }
    }

    fn page(page: u32, total_pages: u32, results: Vec<MediaSearchResult>) -> SearchPage {
        SearchPage {
            page,
            total_pages,
            total_results: total_pages * 20,
            results,
        }
    }

    fn titles(search: &SearchController) -> Vec<&str> {
        search.results().iter().map(|r| r.title.as_str()).collect()
    }

    /// Types `query` and lets its debounce timer fire: the two requests.
    fn searched(search: &mut SearchController, query: &str) -> [Request; 2] {
        let generation = search.input(query).expect("a search to schedule");
        search.fire(generation).try_into().expect("two requests")
    }

    #[test]
    fn blank_query_sends_nothing() {
        let mut search = SearchController::new(true);
        assert_eq!(search.status(), Status::Idle);
        search.input("dark");
        for blank in ["", "   ", "\t\n"] {
            assert_eq!(search.input(blank), None);
            assert_eq!(search.status(), Status::Idle);
            assert!(search.load_more().is_empty());
        }
        assert!((0..5).all(|generation| search.fire(generation).is_empty()));
    }

    #[test]
    fn debounce_sends_one_search_for_the_last_edit() {
        let mut search = SearchController::new(true);
        let timers: Vec<_> = ["d", "da", "dar"]
            .into_iter()
            .map(|text| search.input(text).expect("a timer per edit"))
            .collect();
        // Trailing whitespace is not a new query: no new timer, and the timer
        // of the "dar" edit still counts.
        assert_eq!(search.input("dar "), None);
        assert_eq!(search.status(), Status::Waiting);
        // The timers fire in order; only the last one sends.
        assert!(search.fire(timers[0]).is_empty());
        assert!(search.fire(timers[1]).is_empty());
        let requests = search.fire(timers[2]);
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|r| r.query == "dar" && r.page == 1));
        let kinds: Vec<_> = requests.iter().map(|r| r.media_type).collect();
        assert_eq!(kinds, [MediaType::Movie, MediaType::Tv]);
        assert_eq!(search.status(), Status::Loading);
        assert!(search.fire(timers[2]).is_empty(), "fires once");
        assert!(search.load_more().is_empty(), "first pages still pending");
    }

    #[test]
    fn late_responses_for_an_older_query_are_dropped() {
        let mut search = SearchController::new(true);
        let [a_movie, a_tv] = searched(&mut search, "dar");
        let [b_movie, b_tv] = searched(&mut search, "dark");
        assert_ne!(a_movie.generation, b_movie.generation);

        assert!(search.apply(b_movie.respond(Ok(page(
            1,
            1,
            vec![item(MediaType::Movie, 1, "Dark Movie")]
        )))));
        assert!(search.apply(b_tv.respond(Ok(page(1, 1, vec![item(MediaType::Tv, 1, "Dark")])))));
        // A arrives after B.
        assert!(!search.apply(a_movie.respond(Ok(page(
            1,
            1,
            vec![item(MediaType::Movie, 9, "Old")]
        )))));
        assert!(!search.apply(a_tv.respond(Err(TmdbError::Timeout))));
        assert_eq!(titles(&search), ["Dark Movie", "Dark"]);
        assert_eq!(search.status(), Status::Results);
        // A duplicate of an applied response changes nothing either.
        assert!(!search.apply(b_tv.respond(Ok(page(1, 1, vec![item(MediaType::Tv, 2, "Again")])))));
        assert_eq!(titles(&search), ["Dark Movie", "Dark"]);
    }

    #[test]
    fn clearing_the_query_invalidates_the_search() {
        let mut search = SearchController::new(true);
        let [movie, _] = searched(&mut search, "dark");
        search.input("  ");
        assert_eq!(search.status(), Status::Idle);
        assert!(!search.apply(movie.respond(Ok(page(
            1,
            1,
            vec![item(MediaType::Movie, 1, "Dark")]
        )))));
        assert!(search.results().is_empty());
    }

    #[test]
    fn same_numeric_id_for_a_movie_and_a_series_are_two_results() {
        let mut search = SearchController::new(true);
        let [movie, tv] = searched(&mut search, "x");
        search.apply(movie.respond(Ok(page(1, 1, vec![item(MediaType::Movie, 123, "Film")]))));
        search.apply(tv.respond(Ok(page(1, 1, vec![item(MediaType::Tv, 123, "Series")]))));
        let refs: Vec<_> = search
            .results()
            .iter()
            .map(|r| r.external.clone())
            .collect();
        assert_eq!(refs.len(), 2);
        assert_ne!(refs[0], refs[1]);
        assert_eq!((refs[0].id.as_str(), refs[1].id.as_str()), ("123", "123"));
    }

    #[test]
    fn pages_append_deduplicate_and_stop() {
        let mut search = SearchController::new(true);
        let [movie, tv] = searched(&mut search, "harbor");
        let m = |id, t| item(MediaType::Movie, id, t);
        search.apply(movie.respond(Ok(page(1, 3, vec![m(1, "M1"), m(2, "M2")]))));
        search.apply(tv.respond(Ok(page(1, 1, vec![item(MediaType::Tv, 1, "T1")]))));
        assert_eq!(titles(&search), ["M1", "T1", "M2"]);
        assert!(search.can_load_more());

        // Only movies have more pages.
        let [more] = <[Request; 1]>::try_from(search.load_more()).unwrap();
        assert_eq!((more.media_type, more.page), (MediaType::Movie, 2));
        assert!(search.loading_more() && !search.can_load_more());
        assert!(
            search.load_more().is_empty(),
            "no second request while one is pending"
        );
        // TMDB repeats M2 on page 2 (its ranking moved): listed once.
        search.apply(more.respond(Ok(page(2, 3, vec![m(2, "M2"), m(3, "M3")]))));
        assert_eq!(titles(&search), ["M1", "T1", "M2", "M3"]);

        let [last] = <[Request; 1]>::try_from(search.load_more()).unwrap();
        assert_eq!(last.page, 3);
        search.apply(last.respond(Ok(page(3, 3, vec![m(4, "M4")]))));
        assert!(!search.can_load_more());
        assert!(search.load_more().is_empty());
        assert_eq!(search.results().len(), 5);
    }

    #[test]
    fn a_page_for_an_old_query_is_never_appended() {
        let mut search = SearchController::new(true);
        let [movie, tv] = searched(&mut search, "harbor");
        search.apply(movie.respond(Ok(page(1, 2, vec![item(MediaType::Movie, 1, "M1")]))));
        search.apply(tv.respond(Ok(page(1, 1, vec![]))));
        let [more] = <[Request; 1]>::try_from(search.load_more()).unwrap();
        let [new_movie, _] = searched(&mut search, "dark");
        assert!(!search.apply(more.respond(Ok(page(2, 2, vec![item(MediaType::Movie, 2, "M2")])))));
        assert!(search.apply(new_movie.respond(Ok(page(
            1,
            1,
            vec![item(MediaType::Movie, 5, "Dark")]
        )))));
        assert_eq!(titles(&search), ["Dark"]);
    }

    #[test]
    fn pages_stop_at_the_tmdb_maximum() {
        let mut search = SearchController::new(true);
        let [movie, tv] = searched(&mut search, "a");
        search.apply(tv.respond(Ok(page(1, 1, vec![]))));
        search.apply(movie.respond(Ok(page(1, 900, vec![item(MediaType::Movie, 1, "M")]))));
        search.movie.loaded = MAX_PAGE - 1;
        let [more] = <[Request; 1]>::try_from(search.load_more()).unwrap();
        assert_eq!(more.page, MAX_PAGE);
        search.apply(more.respond(Ok(page(MAX_PAGE, 900, vec![]))));
        assert!(!search.can_load_more());
    }

    #[test]
    fn one_type_failing_keeps_the_other_and_can_be_retried() {
        let mut search = SearchController::new(true);
        let [movie, tv] = searched(&mut search, "dark");
        search.apply(tv.respond(Err(TmdbError::Server(503))));
        assert_eq!(search.status(), Status::Loading, "movies still pending");
        search.apply(movie.respond(Ok(page(1, 1, vec![item(MediaType::Movie, 1, "Dark")]))));
        assert_eq!(search.status(), Status::Results);
        assert_eq!(
            search.partial_failure(),
            Some((MediaType::Tv, &TmdbError::Server(503)))
        );
        let [retry] = <[Request; 1]>::try_from(search.load_more()).unwrap();
        assert_eq!((retry.media_type, retry.page), (MediaType::Tv, 1));
        search.apply(retry.respond(Ok(page(1, 1, vec![item(MediaType::Tv, 1, "Dark")]))));
        assert_eq!(
            (search.partial_failure(), search.results().len()),
            (None, 2)
        );
    }

    #[test]
    fn failures_and_empty_results_are_reported() {
        let mut search = SearchController::new(true);
        let [movie, tv] = searched(&mut search, "zzzz");
        search.apply(movie.respond(Ok(page(1, 0, vec![]))));
        search.apply(tv.respond(Ok(page(1, 0, vec![]))));
        assert_eq!(search.status(), Status::NoResults);
        assert!(!search.can_load_more());

        let [movie, tv] = searched(&mut search, "dark");
        search.apply(movie.respond(Err(TmdbError::Offline("refused".into()))));
        search.apply(tv.respond(Err(TmdbError::CredentialInvalid)));
        assert_eq!(
            search.status(),
            Status::Failed(TmdbError::CredentialInvalid)
        );
        // Try again resends both first pages.
        let retry = search.load_more();
        assert_eq!(retry.iter().map(|r| r.page).collect::<Vec<_>>(), [1, 1]);
        assert_eq!(search.status(), Status::Loading);
    }

    #[test]
    fn removing_the_token_stops_everything() {
        let mut search = SearchController::new(true);
        let [movie, _] = searched(&mut search, "dark");
        search.set_has_token(false);
        assert_eq!(search.status(), Status::NoCredential);
        assert!(!search.apply(movie.respond(Ok(page(
            1,
            1,
            vec![item(MediaType::Movie, 1, "Dark")]
        )))));
        assert_eq!(search.input("dune"), None, "no token, no search");
        assert!(search.load_more().is_empty());

        // A new token searches the current query straight away.
        let now = search.set_has_token(true).expect("fire now");
        assert_eq!(search.fire(now).len(), 2);
        assert!(search.results().is_empty());
    }
}
