//! TMDB v3 client (ADR-0007): token validation and movie/TV search over one
//! pooled `ureq` agent. The response DTOs are private to this module and are
//! mapped at once to `search::MediaSearchResult`.

use std::collections::HashSet;
use std::fmt;
use std::time::Duration;

use serde::Deserialize;

use crate::error::{AppError, ErrorKind};
use crate::library::MediaType;
use crate::metadata::{Episode, Genre, MediaDetails, Season, TvDetails};
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

    /// A title's full metadata: `/3/movie/{id}` or `/3/tv/{id}` (ADR-0013).
    /// `append_to_response` is deliberately not used: R9 needs nothing from
    /// the same namespace at this moment, and appending seasons here would
    /// fetch episodes the user has not opened.
    pub fn details(
        &self,
        token: &Token,
        media_type: MediaType,
        id: &str,
    ) -> Result<MediaDetails, TmdbError> {
        let path = format!("/3/{}/{id}", media_type.key());
        let body = self.get(token, &path, &[("language", LANGUAGE)])?;
        match media_type {
            MediaType::Movie => decode::<MovieDetailsDto>(&body).map(MovieDetailsDto::map),
            MediaType::Tv => decode::<TvDetailsDto>(&body).map(TvDetailsDto::map),
        }
    }

    /// One season's episodes: `/3/tv/{id}/season/{n}` (ADR-0014). The season
    /// endpoint carries every episode field R9 stores, so no request is ever
    /// made per episode.
    pub fn season_episodes(
        &self,
        token: &Token,
        id: &str,
        season: i64,
    ) -> Result<Vec<Episode>, TmdbError> {
        let path = format!("/3/tv/{id}/season/{season}");
        let body = self.get(token, &path, &[("language", LANGUAGE)])?;
        Ok(episodes(decode::<SeasonDetailsDto>(&body)?, season))
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
    [title, original_title, date_value]: [Option<String>; 3],
    overview: Option<String>,
    [poster_path, backdrop_path]: [Option<String>; 2],
) -> Option<MediaSearchResult> {
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
        release_date: date(date_value),
        overview: text(overview),
        poster_path: text(poster_path),
        backdrop_path: text(backdrop_path),
    })
}

/// `/3/movie/{id}`, the fields Bingee stores. Everything else TMDB sends
/// (budget, revenue, production companies, votes, …) is ignored on purpose.
#[derive(Deserialize)]
struct MovieDetailsDto {
    title: Option<String>,
    original_title: Option<String>,
    overview: Option<String>,
    tagline: Option<String>,
    release_date: Option<String>,
    /// "Released", "Post Production", "Canceled", …
    status: Option<String>,
    /// Null for titles whose runtime TMDB does not know.
    runtime: Option<i64>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    genres: Option<Vec<GenreDto>>,
}

/// `/3/tv/{id}`: `name`/`first_air_date` instead of `title`/`release_date`,
/// plus the season summaries.
#[derive(Deserialize)]
struct TvDetailsDto {
    name: Option<String>,
    original_name: Option<String>,
    overview: Option<String>,
    tagline: Option<String>,
    first_air_date: Option<String>,
    last_air_date: Option<String>,
    /// "Returning Series", "Ended", "Canceled", …
    status: Option<String>,
    /// TMDB gives a list; a series with several formats has several entries.
    episode_run_time: Option<Vec<i64>>,
    number_of_seasons: Option<i64>,
    number_of_episodes: Option<i64>,
    poster_path: Option<String>,
    backdrop_path: Option<String>,
    genres: Option<Vec<GenreDto>>,
    seasons: Option<Vec<SeasonDto>>,
}

#[derive(Deserialize)]
struct GenreDto {
    id: u64,
    name: Option<String>,
}

#[derive(Deserialize)]
struct SeasonDto {
    id: Option<u64>,
    /// 0 is TMDB's specials season.
    season_number: Option<i64>,
    name: Option<String>,
    overview: Option<String>,
    air_date: Option<String>,
    episode_count: Option<i64>,
    poster_path: Option<String>,
}

/// `/3/tv/{id}/season/{n}`. Only the episode list is read: the season's own
/// summary already came with the series details.
#[derive(Deserialize)]
struct SeasonDetailsDto {
    episodes: Option<Vec<EpisodeDto>>,
}

#[derive(Deserialize)]
struct EpisodeDto {
    id: Option<u64>,
    episode_number: Option<i64>,
    name: Option<String>,
    overview: Option<String>,
    air_date: Option<String>,
    runtime: Option<i64>,
    still_path: Option<String>,
}

/// A non-empty, non-blank string, or `None`.
fn text(value: Option<String>) -> Option<String> {
    value.filter(|v| !v.trim().is_empty())
}

fn date(value: Option<String>) -> Option<String> {
    value.filter(|d| is_iso_date(d))
}

/// A count TMDB may send as null, zero or (in malformed data) negative.
fn count(value: Option<i64>) -> Option<u32> {
    value.and_then(|n| u32::try_from(n).ok())
}

/// A duration in minutes: zero means "not known", never "zero minutes".
fn minutes(value: Option<i64>) -> Option<u32> {
    count(value).filter(|n| *n > 0)
}

fn genres(dtos: Option<Vec<GenreDto>>) -> Vec<Genre> {
    dtos.unwrap_or_default()
        .into_iter()
        .filter_map(|dto| {
            Some(Genre {
                id: (dto.id > 0).then(|| dto.id.to_string())?,
                name: text(dto.name)?,
            })
        })
        .collect()
}

impl MovieDetailsDto {
    fn map(self) -> MediaDetails {
        let original_title = text(self.original_title);
        MediaDetails {
            media_type: MediaType::Movie,
            title: text(self.title)
                .or_else(|| original_title.clone())
                .unwrap_or_default(),
            original_title,
            overview: text(self.overview),
            tagline: text(self.tagline),
            release_date: date(self.release_date),
            status: text(self.status),
            runtime_minutes: minutes(self.runtime),
            poster_path: text(self.poster_path),
            backdrop_path: text(self.backdrop_path),
            genres: genres(self.genres),
            fetched_at: None,
            tv: None,
        }
    }
}

impl TvDetailsDto {
    fn map(self) -> MediaDetails {
        let original_title = text(self.original_name);
        // The shortest listed format is the typical episode length; a series
        // with one long pilot should not read as a series of long episodes.
        let runtime = self
            .episode_run_time
            .unwrap_or_default()
            .into_iter()
            .filter_map(|n| minutes(Some(n)))
            .min();
        let mut seasons: Vec<Season> = self
            .seasons
            .unwrap_or_default()
            .into_iter()
            .filter_map(SeasonDto::map)
            .collect();
        seasons.sort_by_key(|season| season.number);
        seasons.dedup_by_key(|season| season.number);
        MediaDetails {
            media_type: MediaType::Tv,
            title: text(self.name)
                .or_else(|| original_title.clone())
                .unwrap_or_default(),
            original_title,
            overview: text(self.overview),
            tagline: text(self.tagline),
            release_date: date(self.first_air_date),
            status: text(self.status),
            runtime_minutes: runtime,
            poster_path: text(self.poster_path),
            backdrop_path: text(self.backdrop_path),
            genres: genres(self.genres),
            fetched_at: None,
            tv: Some(TvDetails {
                last_air_date: date(self.last_air_date),
                season_count: count(self.number_of_seasons),
                episode_count: count(self.number_of_episodes),
                seasons,
            }),
        }
    }
}

impl SeasonDto {
    /// `None` for an entry without a usable season number: a season Bingee
    /// could not address again is no season.
    fn map(self) -> Option<Season> {
        Some(Season {
            number: self.season_number.filter(|n| *n >= 0)?,
            external_id: self.id.filter(|id| *id > 0).map(|id| id.to_string()),
            name: text(self.name),
            overview: text(self.overview),
            air_date: date(self.air_date),
            episode_count: count(self.episode_count),
            poster_path: text(self.poster_path),
            episodes_fetched_at: None,
            episodes_known: None,
        })
    }
}

/// The episodes of one season, in episode order, with each episode number and
/// each provider id appearing once: a malformed answer must not become two
/// rows for one episode.
fn episodes(dto: SeasonDetailsDto, season: i64) -> Vec<Episode> {
    let mut episodes: Vec<Episode> = dto
        .episodes
        .unwrap_or_default()
        .into_iter()
        .filter_map(|dto| {
            Some(Episode {
                season_number: season,
                number: dto.episode_number.filter(|n| *n >= 0)?,
                external_id: dto.id.filter(|id| *id > 0).map(|id| id.to_string()),
                name: text(dto.name),
                overview: text(dto.overview),
                air_date: date(dto.air_date),
                runtime_minutes: minutes(dto.runtime),
                still_path: text(dto.still_path),
            })
        })
        .collect();
    episodes.sort_by_key(|episode| episode.number);
    episodes.dedup_by_key(|episode| episode.number);
    let mut seen = HashSet::new();
    episodes.retain(|episode| match &episode.external_id {
        Some(id) => seen.insert(id.clone()),
        None => true,
    });
    episodes
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

    /// A `/3/movie/{id}` body.
    pub fn movie_details(id: u64, title: &str) -> String {
        format!(
            r#"{{"id":{id},"title":"{title}","original_title":"{title}",
            "overview":"A hacker learns the truth.","tagline":"Free your mind.",
            "release_date":"1999-03-31","status":"Released","runtime":136,
            "poster_path":"/matrix.jpg","backdrop_path":"/wide.jpg","budget":63000000,
            "genres":[{{"id":28,"name":"Action"}},{{"id":878,"name":"Science Fiction"}}],
            "vote_average":8.2,"belongs_to_collection":null}}"#
        )
    }

    /// A `/3/tv/{id}` body whose seasons are `(season_number, episode_count)`.
    pub fn tv_details(id: u64, name: &str, seasons: &[(i64, u32)]) -> String {
        let total: u32 = seasons.iter().map(|(_, count)| count).sum();
        let listed: Vec<String> = seasons
            .iter()
            .map(|(number, count)| {
                format!(
                    r#"{{"id":{},"season_number":{number},"name":{},"overview":"A season.",
                    "air_date":"2011-04-1{}","episode_count":{count},"poster_path":null,
                    "vote_average":8.0}}"#,
                    3000 + number,
                    match number {
                        0 => r#""Specials""#.to_owned(),
                        n => format!(r#""Season {n}""#),
                    },
                    number.rem_euclid(10),
                )
            })
            .collect();
        format!(
            r#"{{"id":{id},"name":"{name}","original_name":"{name}","overview":"About the series.",
            "tagline":null,"first_air_date":"2011-04-17","last_air_date":"2019-05-19",
            "status":"Ended","episode_run_time":[57,62],"number_of_seasons":{},
            "number_of_episodes":{total},"poster_path":"/show.jpg","backdrop_path":null,
            "genres":[{{"id":18,"name":"Drama"}}],"seasons":[{}],"networks":[]}}"#,
            seasons.len(),
            listed.join(","),
        )
    }

    /// A `/3/tv/{id}/season/{n}` body with `episodes` episodes, numbered from
    /// one. Episode 1 of every season has no runtime and no air date, so the
    /// missing-value paths are exercised.
    pub fn season_details(season: i64, episodes: u32) -> String {
        let listed: Vec<String> = (1..=episodes)
            .map(|number| {
                let (runtime, air_date) = match number {
                    1 => ("null".to_owned(), "null".to_owned()),
                    n => (format!("{}", 55 + n), format!(r#""2011-05-{n:02}""#)),
                };
                format!(
                    r#"{{"id":{},"episode_number":{number},"season_number":{season},
                    "name":"Episode {number}","overview":"Things happen.","air_date":{air_date},
                    "runtime":{runtime},"still_path":"/s{season}e{number}.jpg",
                    "episode_type":"standard","vote_average":7.9,"crew":[],"guest_stars":[]}}"#,
                    60_000 + season * 100 + i64::from(number),
                )
            })
            .collect();
        format!(
            r#"{{"_id":"x","id":{},"season_number":{season},"name":"Season {season}",
            "overview":"A season.","air_date":"2011-04-17","poster_path":null,
            "vote_average":8.0,"episodes":[{}]}}"#,
            3000 + season,
            listed.join(","),
        )
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
    fn movie_and_tv_details_map_to_the_domain_types() {
        use super::fake::{movie_details, tv_details};
        let server = FakeServer::start(|seen| match seen.target.split('?').next().unwrap() {
            "/3/movie/603" => Reply::json(200, &movie_details(603, "The Matrix")),
            "/3/tv/1399" => Reply::json(
                200,
                &tv_details(1399, "Game of Thrones", &[(0, 3), (1, 10), (2, 10)]),
            ),
            _ => Reply::json(404, r#"{"status_code":34}"#),
        });
        let client = client(&server);

        let movie = client
            .details(&token(), MediaType::Movie, "603")
            .expect("movie details");
        assert_eq!(movie.media_type, MediaType::Movie);
        assert_eq!(movie.title, "The Matrix");
        assert_eq!(movie.tagline.as_deref(), Some("Free your mind."));
        assert_eq!(movie.status.as_deref(), Some("Released"));
        assert_eq!(movie.runtime_minutes, Some(136));
        assert_eq!(movie.release_date.as_deref(), Some("1999-03-31"));
        assert_eq!(movie.backdrop_path.as_deref(), Some("/wide.jpg"));
        assert_eq!(movie.genre_line(), "Action, Science Fiction");
        assert_eq!(movie.fetched_at, None, "storage stamps the time");
        assert!(movie.tv.is_none());

        let series = client
            .details(&token(), MediaType::Tv, "1399")
            .expect("tv details");
        assert_eq!(series.media_type, MediaType::Tv);
        assert_eq!(series.title, "Game of Thrones");
        assert_eq!(series.tagline, None, "null is not an empty string");
        assert_eq!(series.runtime_minutes, Some(57), "the shortest format");
        assert_eq!(series.release_date.as_deref(), Some("2011-04-17"));
        let tv = series.tv.expect("tv part");
        assert_eq!(tv.last_air_date.as_deref(), Some("2019-05-19"));
        assert_eq!((tv.season_count, tv.episode_count), (Some(3), Some(23)));
        let numbers: Vec<i64> = tv.seasons.iter().map(|s| s.number).collect();
        assert_eq!(numbers, [0, 1, 2], "specials first, in season order");
        assert_eq!(tv.seasons[0].label(), "Specials");
        assert_eq!(tv.seasons[0].episode_count, Some(3));
        assert_eq!(tv.seasons[1].external_id.as_deref(), Some("3001"));

        let targets: Vec<String> = server.seen().into_iter().map(|s| s.target).collect();
        assert_eq!(
            targets,
            ["/3/movie/603?language=en-US", "/3/tv/1399?language=en-US"]
        );
        assert_eq!(
            client.details(&token(), MediaType::Movie, "99999"),
            Err(TmdbError::NotFound)
        );
    }

    #[test]
    fn season_details_give_every_episode_in_one_request() {
        let server = FakeServer::start(|seen| match seen.target.split('?').next().unwrap() {
            "/3/tv/1399/season/0" => Reply::json(200, &super::fake::season_details(0, 1)),
            "/3/tv/1399/season/1" => Reply::json(200, &super::fake::season_details(1, 3)),
            _ => Reply::json(404, r#"{"status_code":34}"#),
        });
        let client = client(&server);

        let episodes = client.season_episodes(&token(), "1399", 1).unwrap();
        assert_eq!(episodes.len(), 3);
        let numbers: Vec<i64> = episodes.iter().map(|e| e.number).collect();
        assert_eq!(numbers, [1, 2, 3]);
        assert!(episodes.iter().all(|e| e.season_number == 1));
        assert_eq!(episodes[0].external_id.as_deref(), Some("60101"));
        assert_eq!(episodes[0].name.as_deref(), Some("Episode 1"));
        assert_eq!(
            (episodes[0].runtime_minutes, episodes[0].air_date.as_deref()),
            (None, None),
            "null runtime and air date stay empty"
        );
        assert_eq!(episodes[1].runtime_minutes, Some(57));
        assert_eq!(episodes[1].air_date.as_deref(), Some("2011-05-02"));
        assert_eq!(episodes[1].still_path.as_deref(), Some("/s1e2.jpg"));

        // Season 0 is addressable like any other.
        assert_eq!(
            client.season_episodes(&token(), "1399", 0).unwrap().len(),
            1
        );
        assert_eq!(
            client.season_episodes(&token(), "1399", 9),
            Err(TmdbError::NotFound)
        );
        let targets: Vec<String> = server.seen().into_iter().map(|s| s.target).collect();
        assert_eq!(
            targets,
            [
                "/3/tv/1399/season/1?language=en-US",
                "/3/tv/1399/season/0?language=en-US",
                "/3/tv/1399/season/9?language=en-US",
            ],
            "one request per season, never one per episode"
        );
    }

    #[test]
    fn malformed_detail_payloads_are_survived_not_invented() {
        let server = FakeServer::start(|seen| {
            if seen.target.starts_with("/3/tv/1/season/") {
                // Two entries for episode 2, one provider id on two numbers,
                // one entry with no number at all, and negative values.
                return Reply::json(
                    200,
                    r#"{"episodes":[
                    {"id":5,"episode_number":2,"name":"Second","runtime":-4},
                    {"id":5,"episode_number":3,"name":"Clone of the second"},
                    {"id":6,"episode_number":2,"name":"Duplicate number"},
                    {"id":0,"episode_number":1,"name":"","air_date":"soon","runtime":0},
                    {"id":9,"name":"No number"}]}"#,
                );
            }
            Reply::json(
                200,
                r#"{"id":1,"name":"","original_name":"  ","overview":"",
                "first_air_date":"2011","episode_run_time":[],"number_of_seasons":-1,
                "genres":[{"id":0,"name":"Bad"},{"id":7,"name":""},{"id":18,"name":"Drama"}],
                "seasons":[{"id":1,"season_number":1,"episode_count":2},
                           {"id":2,"season_number":1,"episode_count":9},
                           {"id":3,"season_number":-5},
                           {"id":4,"name":"No number"}]}"#,
            )
        });
        let client = client(&server);

        let series = client.details(&token(), MediaType::Tv, "1").unwrap();
        assert_eq!(series.title, "", "no title to fall back on");
        assert_eq!(series.original_title, None, "blank is not a title");
        assert_eq!(series.overview, None);
        assert_eq!(series.release_date, None, "a year is not a date");
        assert_eq!(series.runtime_minutes, None);
        assert_eq!(series.genre_line(), "Drama", "id 0 and blank names dropped");
        let tv = series.tv.unwrap();
        assert_eq!(tv.season_count, None, "a negative count is no count");
        let numbers: Vec<i64> = tv.seasons.iter().map(|s| s.number).collect();
        assert_eq!(numbers, [1], "one row per season number, none unnumbered");

        let episodes = client.season_episodes(&token(), "1", 1).unwrap();
        let numbers: Vec<i64> = episodes.iter().map(|e| e.number).collect();
        assert_eq!(numbers, [1, 2], "one row per episode number");
        assert_eq!(episodes[0].external_id, None, "id 0 is no id");
        assert_eq!(episodes[0].name, None);
        assert_eq!(episodes[0].air_date, None);
        assert_eq!(
            episodes[0].runtime_minutes, None,
            "0 minutes is not a runtime"
        );
        assert_eq!(episodes[1].external_id.as_deref(), Some("5"));
        assert_eq!(
            episodes[1].runtime_minutes, None,
            "negative is not a runtime"
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
