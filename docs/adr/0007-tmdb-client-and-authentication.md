# ADR-0007: TMDB client: Bearer read-access token, v3 search, isolated DTOs

- Status: Proposed
- Date: 2026-09-14

## Context

R7 adds remote search. Bingee needs application-level read access to TMDB:
validate a user-supplied credential, search movies and TV series, page
through results. It needs no TMDB user account or session. Results stay
transient in R7; persistence is R8.

Official documentation checked on 2026-09-14:

- Application authentication:
  <https://developer.themoviedb.org/docs/authentication-application>. The
  default method is the **API Read Access Token** in `Authorization: Bearer
  <token>`; it works for v3 and v4. The v3 API key query parameter is the
  alternative.
- Validation: <https://developer.themoviedb.org/reference/authentication-validate-key>.
  `GET https://api.themoviedb.org/3/authentication` → `200
  {"success":true,"status_code":1,...}`; invalid → `401
  {"success":false,"status_code":7,...}`.
- Search: <https://developer.themoviedb.org/reference/search-movie> and
  <https://developer.themoviedb.org/reference/search-tv>.
  `GET /3/search/movie` and `/3/search/tv` with `query` (required), `page`
  (default 1), `language` (default `en-US`), `include_adult` (default false).
  Both return `{page, results, total_pages, total_results}`. Movies carry
  `title`, `original_title`, `release_date`; TV carries `name`,
  `original_name`, `first_air_date`. Both have `id`, `overview`,
  `poster_path`, `backdrop_path`.
- Errors: <https://developer.themoviedb.org/docs/errors>. 401 (codes 3, 7),
  404 (34), 400 "Invalid page: Pages start at 1 and max at 500" (22), 429
  (25), 500 (11), 502 (43), 503 (9, 46), 504 (24).
- Rate limiting: <https://developer.themoviedb.org/docs/rate-limiting>.
  Around 40 requests/second, may change, "respect the 429". No
  `Retry-After` is documented.
- Images: <https://developer.themoviedb.org/docs/image-basics>. URL =
  `base_url` (from `/configuration`) + size (`w92`, `w185`, `w500`,
  `original`) + `file_path`. Not used in R7.
- Attribution: <https://developer.themoviedb.org/docs/faq> and
  <https://www.themoviedb.org/about/logos-attribution>. Use an official TMDB
  logo, less prominent than the app's own branding, and the notice "This
  product uses the TMDB API but is not endorsed or certified by TMDB" in
  About/Credits. The API is free for non-commercial use with attribution;
  commercial use needs TMDB's agreement.

## Decision

- **Credential**: the API Read Access Token as a Bearer header, never in
  a URL. The v3 API key and account sessions are not supported.
- **Endpoints**: `GET /3/authentication` validates; `GET /3/search/movie`
  and `GET /3/search/tv` search, always with `include_adult=false` and
  `language=en-US`. `en-US` matches the English UI; there is no language
  preference yet.
- **Client** (`src/tmdb.rs`): one `TmdbClient` holding one `ureq::Agent`
  (connection pooling, rustls, gzip), with a configurable base URL so tests
  can target a local fake server. Only `validate`, `search(media_type,
  query, page)`.
- **DTOs**: private `MovieDto`/`TvDto`/`PageDto` in `tmdb.rs`, mapped at once
  to the provider-independent `MediaSearchResult`
  (`src/search.rs`). DTOs never reach Slint, the database or the domain. Empty
  strings become `None`; dates that are not `YYYY-MM-DD` are dropped; results
  without any title are skipped.
- **Identity**: `ExternalRef { source: Tmdb, media_type, id }`. Movie 123 and
  TV 123 are different keys everywhere, matching schema v1.
- **Errors**: `TmdbError` = `CredentialInvalid` (401), `RateLimited` (429),
  `Timeout`, `Offline` (DNS, connect, TLS, I/O), `Server(status)` (5xx),
  `MalformedResponse` (unreadable body or JSON), `Unexpected(status)`
  (other statuses). No automatic retries: the user retries.
- **Pages**: TMDB's maximum page (500) and each type's `total_pages` bound
  "Load more".
- **Attribution**: the official "alt short" blue TMDB logo (SVG) and the
  required notice on the About page, plus a "Results from TMDB" line on
  Discover.

## Alternatives considered

- **v3 API key in the query string**: puts the secret in URLs, which end up
  in logs and error texts. TMDB also recommends the Bearer token.
- **`/3/search/multi`**: one request for movies, TV and people, but mixed
  shapes, people to filter out, and a page that holds fewer of each type.
  Two typed requests keep the DTOs separate and the identity explicit.
- **A generated OpenAPI client or a TMDB crate**: large surface for three
  calls, and it would couple the domain to someone else's types.

## Consequences

### Positive

- The secret travels only in one header, set in one function.
- The domain type is ready for R8 persistence (`external_refs` + `media`).
- Tests run against a local fake server; nothing needs a live TMDB.

### Negative / risks

- Two requests per search page (one per type).
- English metadata only until a language setting exists.
- The logo is a third-party trademark asset used under TMDB's attribution
  rules; it must not be modified.

## Validation

Fake-server tests for 200/401/429/5xx/malformed/timeout/offline, the exact
request line and header, Unicode queries, Movie/TV mapping and the same
numeric id for a movie and a TV series.

## Revisit trigger

TMDB deprecating v3 search or the Bearer method, a language preference, or
R8 needing `/configuration` for image URLs.
