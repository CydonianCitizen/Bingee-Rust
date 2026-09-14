//! TMDB v3 client (ADR-0007): token validation and movie/TV search over one
//! pooled `ureq` agent. The response DTOs are private to this module and are
//! mapped at once to `search::MediaSearchResult`.

use std::fmt;
use std::time::Duration;

use serde::Deserialize;

use crate::error::{AppError, ErrorKind};
use crate::library::MediaType;
use crate::search::{ExternalRef, MAX_PAGE, MediaSearchResult, SearchPage, Source};
use crate::secrets::Token;
use crate::{APP_ID, APP_VERSION};

const API_BASE: &str = "https://api.themoviedb.org";
/// TMDB's documented image base URL, used until `/3/configuration` answers.
pub const IMAGE_BASE: &str = "https://image.tmdb.org/t/p/";
/// Metadata language. The UI is English and has no language setting yet.
const LANGUAGE: &str = "en-US";
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// The whole request, from DNS lookup to the last body byte.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
/// A search page is tens of KB; anything this large is not a TMDB answer.
const MAX_BODY: u64 = 2 * 1024 * 1024;
/// A `w185` poster is tens of KB.
const MAX_IMAGE: u64 = 4 * 1024 * 1024;

/// Why a TMDB call failed. `Display` is diagnostic text for the log and never
/// contains the token; `user_message` is what the UI shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TmdbError {
    /// HTTP 401: the token is wrong, revoked or expired.
    CredentialInvalid,
    /// HTTP 404.
    NotFound,
    /// HTTP 429.
    RateLimited,
    Timeout,
    /// No connection: DNS, refused, reset, TLS.
    Offline(String),
    /// HTTP 5xx.
    Server(u16),
    /// An unreadable body or unexpected JSON.
    MalformedResponse(String),
    /// Anything else, such as an unexpected HTTP status.
    Unexpected(String),
}

impl TmdbError {
    pub fn user_message(&self) -> &'static str {
        match self {
            TmdbError::CredentialInvalid => {
                "TMDB rejected your access token. Check it in Settings."
            }
            TmdbError::NotFound => "TMDB does not have it.",
            TmdbError::RateLimited => {
                "TMDB is receiving too many requests. Wait a moment, then try again."
            }
            TmdbError::Timeout => "TMDB took too long to answer. Try again.",
            TmdbError::Offline(_) => {
                "Bingee Desktop can't reach TMDB. Check your internet connection, then try again."
            }
            TmdbError::Server(_) => "TMDB is having problems right now. Try again later.",
            TmdbError::MalformedResponse(_) => {
                "TMDB sent a response Bingee Desktop could not read. Try again later."
            }
            TmdbError::Unexpected(_) => "Something went wrong while talking to TMDB.",
        }
    }
}

impl fmt::Display for TmdbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TmdbError::CredentialInvalid => f.write_str("TMDB rejected the token (HTTP 401)"),
            TmdbError::NotFound => f.write_str("not found at TMDB (HTTP 404)"),
            TmdbError::RateLimited => f.write_str("TMDB rate limit (HTTP 429)"),
            TmdbError::Timeout => f.write_str("TMDB request timed out"),
            TmdbError::Offline(detail) => write!(f, "no connection to TMDB: {detail}"),
            TmdbError::Server(status) => write!(f, "TMDB server error (HTTP {status})"),
            TmdbError::MalformedResponse(detail) => write!(f, "unreadable TMDB response: {detail}"),
            TmdbError::Unexpected(detail) => write!(f, "unexpected TMDB failure: {detail}"),
        }
    }
}

impl std::error::Error for TmdbError {}

impl From<TmdbError> for AppError {
    fn from(error: TmdbError) -> Self {
        let kind = match error {
            TmdbError::CredentialInvalid => ErrorKind::Configuration,
            TmdbError::MalformedResponse(_) => ErrorKind::InvalidData,
            _ => ErrorKind::Network,
        };
        AppError::new(kind, error.user_message()).with_source(error)
    }
}

/// One TMDB client for the whole app: cheap to clone, one connection pool.
#[derive(Clone)]
pub struct TmdbClient {
    agent: ureq::Agent,
    base: String,
}

impl fmt::Debug for TmdbClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TmdbClient")
            .field("base", &self.base)
            .finish_non_exhaustive()
    }
}

impl TmdbClient {
    pub fn new() -> Self {
        Self::with(API_BASE, CONNECT_TIMEOUT, REQUEST_TIMEOUT)
    }

    fn with(base: &str, connect: Duration, total: Duration) -> Self {
        let config = ureq::Agent::config_builder()
            .http_status_as_error(false)
            .timeout_connect(Some(connect))
            .timeout_global(Some(total))
            .user_agent(format!("{APP_ID}/{APP_VERSION}"))
            .build();
        Self {
            agent: config.into(),
            base: base.trim_end_matches('/').to_owned(),
        }
    }

    /// A client for a local fake server. The connect timeout stays generous:
    /// Windows takes about two seconds to refuse a closed local port.
    #[cfg(test)]
    pub fn for_tests(base: &str, total: Duration) -> Self {
        Self::with(base, Duration::from_secs(5), total)
    }

    /// `Ok` if TMDB accepts the token.
    pub fn validate(&self, token: &Token) -> Result<(), TmdbError> {
        let body = self.get(token, "/3/authentication", &[])?;
        match decode::<AuthDto>(&body)?.success {
            true => Ok(()),
            false => Err(TmdbError::CredentialInvalid),
        }
    }

    pub fn search(
        &self,
        token: &Token,
        media_type: MediaType,
        query: &str,
        page: u32,
    ) -> Result<SearchPage, TmdbError> {
        let page = page.to_string();
        let params = [
            ("query", query),
            ("page", page.as_str()),
            ("include_adult", "false"),
            ("language", LANGUAGE),
        ];
        let body = self.get(token, &format!("/3/search/{}", media_type.key()), &params)?;
        match media_type {
            MediaType::Movie => decode::<PageDto<MovieDto>>(&body).map(|p| p.map(MovieDto::map)),
            MediaType::Tv => decode::<PageDto<TvDto>>(&body).map(|p| p.map(TvDto::map)),
        }
    }

    /// The image base URL from `/3/configuration`, if it offers our poster
    /// size.
    pub fn image_base(&self, token: &Token) -> Result<String, TmdbError> {
        let body = self.get(token, "/3/configuration", &[])?;
        let images = decode::<ConfigDto>(&body)?.images;
        let usable = images.secure_base_url.starts_with("http")
            && images.secure_base_url.ends_with('/')
            && images
                .poster_sizes
                .iter()
                .any(|size| size == crate::poster::SIZE);
        match usable {
            true => Ok(images.secure_base_url),
            false => Err(TmdbError::MalformedResponse(format!(
                "no usable {} image base in the configuration",
                crate::poster::SIZE
            ))),
        }
    }

    /// An image from TMDB's image server. Needs no token, and gets none.
    pub fn get_image(&self, url: &str) -> Result<Vec<u8>, TmdbError> {
        let mut response = self
            .agent
            .get(url)
            .header("Accept", "image/*")
            .call()
            .map_err(|err| transport(err, None))?;
        status(response.status().as_u16())?;
        response
            .body_mut()
            .with_config()
            .limit(MAX_IMAGE)
            .read_to_vec()
            .map_err(|err| body_error(err, None))
    }

    /// The body of a successful GET. The token goes into the header only.
    fn get(&self, token: &Token, path: &str, params: &[(&str, &str)]) -> Result<String, TmdbError> {
        let mut request = self
            .agent
            .get(format!("{}{path}", self.base))
            .header("Authorization", format!("Bearer {}", token.secret()))
            .header("Accept", "application/json");
        for (key, value) in params {
            request = request.query(key, value);
        }
        let mut response = request.call().map_err(|err| transport(err, Some(token)))?;
        status(response.status().as_u16())?;
        response
            .body_mut()
            .with_config()
            .limit(MAX_BODY)
            .read_to_string()
            .map_err(|err| body_error(err, Some(token)))
    }
}

fn status(status: u16) -> Result<(), TmdbError> {
    match status {
        200..=299 => Ok(()),
        401 => Err(TmdbError::CredentialInvalid),
        404 => Err(TmdbError::NotFound),
        429 => Err(TmdbError::RateLimited),
        500..=599 => Err(TmdbError::Server(status)),
        status => Err(TmdbError::Unexpected(format!("HTTP {status}"))),
    }
}

/// A failure while reading a body: the transport's kinds, anything else is a
/// malformed response.
fn body_error(err: ureq::Error, token: Option<&Token>) -> TmdbError {
    match transport(err, token) {
        TmdbError::Unexpected(detail) => TmdbError::MalformedResponse(detail),
        other => other,
    }
}

/// Classifies a transport failure. The detail text is scrubbed of the token,
/// although ureq never puts headers in its errors.
fn transport(err: ureq::Error, token: Option<&Token>) -> TmdbError {
    let mut detail = err.to_string();
    if let Some(token) = token {
        detail = detail.replace(token.secret(), "[redacted]");
    }
    match err {
        ureq::Error::Timeout(_) => TmdbError::Timeout,
        ureq::Error::Io(io) if io.kind() == std::io::ErrorKind::TimedOut => TmdbError::Timeout,
        ureq::Error::Io(_)
        | ureq::Error::HostNotFound
        | ureq::Error::ConnectionFailed
        | ureq::Error::Tls(_)
        | ureq::Error::Rustls(_) => TmdbError::Offline(detail),
        ureq::Error::Protocol(_) | ureq::Error::BodyExceedsLimit(_) => {
            TmdbError::MalformedResponse(detail)
        }
        _ => TmdbError::Unexpected(detail),
    }
}

fn decode<'a, T: Deserialize<'a>>(body: &'a str) -> Result<T, TmdbError> {
    serde_json::from_str(body).map_err(|err| TmdbError::MalformedResponse(err.to_string()))
}

#[derive(Deserialize)]
struct AuthDto {
    success: bool,
}

/// `/3/configuration`, the part Bingee uses.
#[derive(Deserialize)]
struct ConfigDto {
    images: ImagesDto,
}

#[derive(Deserialize)]
struct ImagesDto {
    secure_base_url: String,
    poster_sizes: Vec<String>,
}

#[derive(Deserialize)]
struct PageDto<T> {
    page: u32,
    results: Vec<T>,
    total_pages: u32,
    total_results: u32,
}

impl<T> PageDto<T> {
    fn map(self, item: fn(T) -> Option<MediaSearchResult>) -> SearchPage {
        SearchPage {
            page: self.page,
            total_pages: self.total_pages.min(MAX_PAGE),
            total_results: self.total_results,
            results: self.results.into_iter().filter_map(item).collect(),
        }
    }
}

/// `/3/search/movie` result. Unknown fields are ignored.
#[derive(Deserialize)]
struct MovieDto {
    id: u64,
    title: Option<String>,
    original_title: Option<String>,
    release_date: Option<String>,
    overview: Option<String>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
}

/// `/3/search/tv` result: `name` and `first_air_date` instead of `title` and
/// `release_date`.
#[derive(Deserialize)]
struct TvDto {
    id: u64,
    name: Option<String>,
    original_name: Option<String>,
    first_air_date: Option<String>,
    overview: Option<String>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
}

impl MovieDto {
    fn map(self) -> Option<MediaSearchResult> {
        let fields = [self.title, self.original_title, self.release_date];
        result(
            MediaType::Movie,
            self.id,
            fields,
            self.overview,
            [self.poster_path, self.backdrop_path],
        )
    }
}

impl TvDto {
    fn map(self) -> Option<MediaSearchResult> {
        let fields = [self.name, self.original_name, self.first_air_date];
        result(
            MediaType::Tv,
            self.id,
            fields,
            self.overview,
            [self.poster_path, self.backdrop_path],
        )
    }
}

/// Builds the domain result. `None` for an entry without an id or any title.
/// Empty strings become `None`; a date that is not `YYYY-MM-DD` is dropped.
fn result(
    media_type: MediaType,
    id: u64,
    [title, original_title, date]: [Option<String>; 3],
    overview: Option<String>,
    [poster_path, backdrop_path]: [Option<String>; 2],
) -> Option<MediaSearchResult> {
    let text = |value: Option<String>| value.filter(|v| !v.trim().is_empty());
    let original_title = text(original_title);
    let title = text(title).or_else(|| original_title.clone())?;
    (id > 0).then(|| MediaSearchResult {
        external: ExternalRef {
            source: Source::Tmdb,
            media_type,
            id: id.to_string(),
        },
        title,
        original_title,
        release_date: date.filter(|d| is_iso_date(d)),
        overview: text(overview),
        poster_path: text(poster_path),
        backdrop_path: text(backdrop_path),
    })
}

fn is_iso_date(date: &str) -> bool {
    let b = date.as_bytes();
    b.len() == 10
        && b.iter().enumerate().all(|(i, c)| {
            if i == 4 || i == 7 {
                *c == b'-'
            } else {
                c.is_ascii_digit()
            }
        })
}

/// A scripted HTTP/1.1 server on 127.0.0.1 for tests: no live TMDB, no token.
#[cfg(test)]
pub mod fake {
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    /// What the server saw: the request target (path and query) and the
    /// `Authorization` header.
    #[derive(Debug, Clone)]
    pub struct Seen {
        pub target: String,
        pub authorization: Option<String>,
    }

    pub struct Reply {
        pub status: u16,
        pub body: Vec<u8>,
        pub delay: Duration,
        /// Content-Length to announce; more than `body` fakes a cut transfer.
        pub declared: Option<usize>,
    }

    impl Reply {
        pub fn json(status: u16, body: &str) -> Self {
            Self::bytes(status, body.as_bytes().to_vec())
        }

        pub fn bytes(status: u16, body: Vec<u8>) -> Self {
            Self {
                status,
                body,
                delay: Duration::ZERO,
                declared: None,
            }
        }

        pub fn after(mut self, delay: Duration) -> Self {
            self.delay = delay;
            self
        }

        /// Announces the full length, sends half, and closes.
        pub fn truncated(mut self) -> Self {
            self.declared = Some(self.body.len());
            self.body.truncate(self.body.len() / 2);
            self
        }
    }

    type Handler = dyn Fn(&Seen) -> Reply + Send + Sync;

    pub struct FakeServer {
        pub base: String,
        seen: Arc<Mutex<Vec<Seen>>>,
    }

    impl FakeServer {
        /// Serves every connection on its own thread, so a delayed reply does
        /// not hold up the others.
        pub fn start(handler: impl Fn(&Seen) -> Reply + Send + Sync + 'static) -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let base = format!("http://{}", listener.local_addr().unwrap());
            let seen = Arc::new(Mutex::new(Vec::new()));
            let handler: Arc<Handler> = Arc::new(handler);
            let log = seen.clone();
            std::thread::spawn(move || {
                for stream in listener.incoming().flatten() {
                    let (log, handler) = (log.clone(), handler.clone());
                    std::thread::spawn(move || serve(stream, &log, &*handler));
                }
            });
            Self { base, seen }
        }

        pub fn seen(&self) -> Vec<Seen> {
            self.seen.lock().unwrap().clone()
        }
    }

    fn serve(mut stream: TcpStream, log: &Mutex<Vec<Seen>>, handler: &Handler) {
        let mut head = Vec::new();
        let mut chunk = [0u8; 1024];
        while !head.windows(4).any(|w| w == b"\r\n\r\n") {
            match stream.read(&mut chunk) {
                Ok(0) | Err(_) => return,
                Ok(n) => head.extend_from_slice(&chunk[..n]),
            }
        }
        let head = String::from_utf8_lossy(&head);
        let mut lines = head.lines();
        let target = lines.next().and_then(|l| l.split(' ').nth(1)).unwrap_or("");
        let authorization = lines.find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("authorization")
                .then(|| value.trim().to_owned())
        });
        let seen = Seen {
            target: target.to_owned(),
            authorization,
        };
        let reply = handler(&seen);
        log.lock().unwrap().push(seen);
        std::thread::sleep(reply.delay);
        let _ = write!(
            stream,
            "HTTP/1.1 {} Fake\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            reply.status,
            reply.declared.unwrap_or(reply.body.len()),
        );
        let _ = stream.write_all(&reply.body);
    }

    /// A search page body with `(id, title)` movies or series.
    pub fn page(kind: &str, page: u32, total_pages: u32, items: &[(u64, &str)]) -> String {
        let (title, date) = match kind {
            "movie" => ("title", "release_date"),
            _ => ("name", "first_air_date"),
        };
        let results: Vec<String> = items
            .iter()
            .map(|(id, name)| {
                format!(r#"{{"id":{id},"{title}":"{name}","{date}":"2016-11-10","overview":"","poster_path":null}}"#)
            })
            .collect();
        format!(
            r#"{{"page":{page},"results":[{}],"total_pages":{total_pages},"total_results":{}}}"#,
            results.join(","),
            total_pages * 20
        )
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{FakeServer, Reply};
    use super::*;

    const TOKEN: &str = "test-token-not-real-7f3a";
    const SUCCESS: &str = r#"{"success":true,"status_code":1,"status_message":"Success."}"#;
    const INVALID: &str = r#"{"status_code":7,"status_message":"Invalid API key: You must be granted a valid key.","success":false}"#;

    fn token() -> Token {
        Token::parse(TOKEN).unwrap()
    }

    fn client(server: &FakeServer) -> TmdbClient {
        TmdbClient::for_tests(&server.base, Duration::from_secs(5))
    }

    fn replying(status: u16, body: &'static str) -> FakeServer {
        FakeServer::start(move |_| Reply::json(status, body))
    }

    #[test]
    fn validation_sends_the_bearer_header_and_reads_success() {
        let server = replying(200, SUCCESS);
        assert_eq!(client(&server).validate(&token()), Ok(()));
        let seen = server.seen();
        assert_eq!(seen[0].target, "/3/authentication");
        assert_eq!(
            seen[0].authorization.as_deref(),
            Some("Bearer test-token-not-real-7f3a")
        );
    }

    #[test]
    fn statuses_map_to_distinct_errors() {
        let cases: [(u16, &'static str, TmdbError); 6] = [
            (401, INVALID, TmdbError::CredentialInvalid),
            (200, r#"{"success":false}"#, TmdbError::CredentialInvalid),
            (429, r#"{"status_code":25}"#, TmdbError::RateLimited),
            (500, r#"{"status_code":11}"#, TmdbError::Server(500)),
            (503, r#"{"status_code":46}"#, TmdbError::Server(503)),
            (404, r#"{"status_code":34}"#, TmdbError::NotFound),
        ];
        for (status, body, expected) in cases {
            let server = replying(status, body);
            assert_eq!(
                client(&server).validate(&token()),
                Err(expected),
                "{status} {body}"
            );
        }
    }

    #[test]
    fn malformed_json_is_its_own_error() {
        let server = replying(200, r#"{"page":1,"results":[{"title":"no id"}]"#);
        let error = client(&server)
            .search(&token(), MediaType::Movie, "x", 1)
            .unwrap_err();
        assert!(
            matches!(error, TmdbError::MalformedResponse(_)),
            "{error:?}"
        );
        let server = replying(200, "<html>captive portal</html>");
        let error = client(&server).validate(&token()).unwrap_err();
        assert!(
            matches!(error, TmdbError::MalformedResponse(_)),
            "{error:?}"
        );
    }

    #[test]
    fn no_server_is_offline_and_a_slow_one_times_out() {
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let closed =
            TmdbClient::for_tests(&format!("http://127.0.0.1:{port}"), Duration::from_secs(10));
        let error = closed.validate(&token()).unwrap_err();
        assert!(matches!(error, TmdbError::Offline(_)), "{error:?}");

        let slow = FakeServer::start(|_| Reply::json(200, SUCCESS).after(Duration::from_secs(3)));
        let impatient = TmdbClient::for_tests(&slow.base, Duration::from_millis(300));
        assert_eq!(impatient.validate(&token()), Err(TmdbError::Timeout));
    }

    #[test]
    fn movie_and_tv_search_map_to_separate_identities() {
        let server = FakeServer::start(|seen| {
            if seen.target.starts_with("/3/search/movie") {
                Reply::json(
                    200,
                    r#"{"page":1,"total_pages":1,"total_results":2,"results":[
                    {"id":123,"title":"Arrival","original_title":"Premier contact","release_date":"2016-11-10",
                     "overview":"A linguist…","poster_path":"/x.jpg","backdrop_path":null,"genre_ids":[18],"vote_average":7.6},
                    {"id":0,"title":"Bad id"},
                    {"id":7,"title":"","original_title":"","release_date":""}]}"#,
                )
            } else {
                Reply::json(
                    200,
                    r#"{"page":1,"total_pages":1,"total_results":1,"results":[
                    {"id":123,"name":"Shōgun","original_name":"SHŌGUN","first_air_date":"2024-02-27",
                     "overview":"","poster_path":null,"origin_country":["US"]}]}"#,
                )
            }
        });
        let client = client(&server);
        let movies = client
            .search(&token(), MediaType::Movie, "Shōgun & Co", 2)
            .unwrap();
        let tv = client
            .search(&token(), MediaType::Tv, "Shōgun & Co", 2)
            .unwrap();

        assert_eq!(
            movies.results.len(),
            1,
            "id 0 and title-less entries are skipped"
        );
        let arrival = &movies.results[0];
        assert_eq!(
            arrival.external,
            ExternalRef {
                source: Source::Tmdb,
                media_type: MediaType::Movie,
                id: "123".into()
            }
        );
        assert_eq!(
            (arrival.title.as_str(), arrival.original_title.as_deref()),
            ("Arrival", Some("Premier contact"))
        );
        assert_eq!(
            (arrival.year(), arrival.overview.as_deref()),
            (Some("2016"), Some("A linguist…"))
        );
        assert_eq!(
            (
                arrival.poster_path.as_deref(),
                arrival.backdrop_path.as_deref()
            ),
            (Some("/x.jpg"), None)
        );

        let shogun = &tv.results[0];
        assert_eq!(shogun.external.media_type, MediaType::Tv);
        assert_eq!(shogun.external.id, arrival.external.id);
        assert_ne!(
            shogun.external, arrival.external,
            "same number, different title"
        );
        assert_eq!(
            (shogun.title.as_str(), shogun.original_title.as_deref()),
            ("Shōgun", Some("SHŌGUN"))
        );
        assert_eq!(
            (shogun.release_date.as_deref(), shogun.overview.as_deref()),
            (Some("2024-02-27"), None)
        );

        let targets: Vec<String> = server.seen().into_iter().map(|s| s.target).collect();
        assert_eq!(
            targets,
            [
                "/3/search/movie?query=Sh%C5%8Dgun%20%26%20Co&page=2&include_adult=false&language=en-US",
                "/3/search/tv?query=Sh%C5%8Dgun%20%26%20Co&page=2&include_adult=false&language=en-US",
            ]
        );
    }

    #[test]
    fn configuration_gives_the_image_base_and_images_need_no_token() {
        let server = FakeServer::start(|seen| match seen.target.as_str() {
            "/3/configuration" => Reply::json(
                200,
                r#"{"images":{"base_url":"http://image.tmdb.org/t/p/",
                "secure_base_url":"https://image.tmdb.org/t/p/","poster_sizes":["w92","w185","original"]},
                "change_keys":["x"]}"#,
            ),
            "/t/p/w185/p.jpg" => Reply::bytes(200, vec![0xff, 0xd8, 0xff]),
            _ => Reply::json(404, "{}"),
        });
        let client = client(&server);
        assert_eq!(client.image_base(&token()).as_deref(), Ok(IMAGE_BASE));
        let image = format!("{}/t/p/w185/p.jpg", server.base);
        assert_eq!(client.get_image(&image), Ok(vec![0xff, 0xd8, 0xff]));
        let missing = format!("{}/t/p/w185/none.jpg", server.base);
        assert_eq!(client.get_image(&missing), Err(TmdbError::NotFound));
        let seen = server.seen();
        assert!(seen[0].authorization.is_some(), "the API needs the token");
        assert!(
            seen[1..].iter().all(|s| s.authorization.is_none()),
            "images never get it"
        );

        let no_size = FakeServer::start(|_| {
            Reply::json(
                200,
                r#"{"images":{"secure_base_url":"https://x/t/p/","poster_sizes":["w500"]}}"#,
            )
        });
        let error =
            TmdbClient::for_tests(&no_size.base, Duration::from_secs(5)).image_base(&token());
        assert!(
            matches!(error, Err(TmdbError::MalformedResponse(_))),
            "{error:?}"
        );
    }

    #[test]
    fn page_numbers_are_capped_at_the_tmdb_maximum() {
        let server = replying(
            200,
            r#"{"page":1,"results":[],"total_pages":2000,"total_results":40000}"#,
        );
        let page = client(&server)
            .search(&token(), MediaType::Tv, "a", 1)
            .unwrap();
        assert_eq!((page.total_pages, page.total_results), (MAX_PAGE, 40_000));
    }

    #[test]
    fn the_token_never_appears_in_debug_display_or_errors() {
        let client = TmdbClient::for_tests("http://127.0.0.1:9", Duration::from_millis(200));
        assert!(!format!("{client:?}").contains(TOKEN));
        let redacted = transport(
            ureq::Error::BadUri(format!("http://x/?t={TOKEN}")),
            Some(&token()),
        );
        assert_eq!(
            redacted,
            TmdbError::Unexpected("bad uri: http://x/?t=[redacted]".into())
        );
        for error in [redacted, client.validate(&token()).unwrap_err()] {
            let app = AppError::from(error.clone());
            for text in [
                format!("{error}"),
                format!("{error:?}"),
                format!("{app}"),
                format!("{app:?}"),
            ] {
                assert!(!text.contains(TOKEN), "{text}");
            }
        }
    }

    #[test]
    fn errors_map_into_app_error_kinds() {
        assert_eq!(
            AppError::from(TmdbError::CredentialInvalid).kind,
            ErrorKind::Configuration
        );
        assert_eq!(
            AppError::from(TmdbError::MalformedResponse(String::new())).kind,
            ErrorKind::InvalidData
        );
        let offline = AppError::from(TmdbError::Offline("refused".into()));
        assert_eq!(offline.kind, ErrorKind::Network);
        assert!(
            !offline.message.contains("refused"),
            "user text has no diagnostics"
        );
        assert!(offline.to_string().contains("refused"), "the log has them");
    }
}
