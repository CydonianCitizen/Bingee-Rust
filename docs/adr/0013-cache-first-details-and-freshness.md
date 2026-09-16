# ADR-0013: Cache-first media details and the freshness policy

- Status: Proposed
- Date: 2026-09-16

## Context

R9 adds a detail layer for library titles: movie and series details, genres,
season summaries and episode metadata. The library is local-first (ADR-0010):
a title's detail must open without a network, and a refresh must never make
things worse than what is cached. TMDB is the only provider.

TMDB endpoints, checked on 2026-09-16 against the official reference:

| Endpoint | Reference |
| --- | --- |
| `GET /3/movie/{movie_id}` | <https://developer.themoviedb.org/reference/movie-details> |
| `GET /3/tv/{series_id}` | <https://developer.themoviedb.org/reference/tv-series-details> |
| `GET /3/tv/{series_id}/season/{season_number}` | <https://developer.themoviedb.org/reference/tv-season-details> |

All three take `language` (default `en-US`) and `append_to_response`, need the
Bearer token, and answer 401/404/429/5xx like the search endpoints (ADR-0007).

## Decision

### Requests and fields

- **Movie**: `/3/movie/{id}?language=en-US`. Consumed: `title`,
  `original_title`, `overview`, `tagline`, `release_date`, `status`,
  `runtime`, `poster_path`, `backdrop_path`, `genres[].id/name`. Ignored:
  budget, revenue, companies, countries, languages, votes, collection, IMDb id.
- **Series**: `/3/tv/{id}?language=en-US`. Consumed: `name`,
  `original_name`, `overview`, `tagline`, `first_air_date`, `last_air_date`,
  `status`, `episode_run_time[]`, `number_of_seasons`, `number_of_episodes`,
  `poster_path`, `backdrop_path`, `genres[]`, and `seasons[]` with `id`,
  `season_number`, `name`, `overview`, `air_date`, `episode_count`,
  `poster_path`.
- **Season**: see ADR-0014.
- **`append_to_response` is not used.** R9 needs one request per title, and
  appending `season/N` would download episode lists the user never opened
  (ADR-0014). Credits are out of scope.
- **Language**: `en-US`, like search. The UI is English and has no language
  setting; stored metadata is in that language.
- **Nullability**: every field is read as optional, whatever the reference
  marks as required. Blank strings become "absent"; dates must be
  `YYYY-MM-DD`; runtimes and counts must be positive/non-negative integers.
  A series' runtime is the shortest positive `episode_run_time` entry. An
  absent value is stored as `NULL` and the row that would show it is left out
  of the UI: no `0 min`, no empty label.
- DTOs stay private to `tmdb.rs` and map at once to the provider-independent
  `metadata::MediaDetails` / `Season` / `Episode` / `Genre`. SQLite rows and
  Slint structs are separate again.

### Cache first

Selecting a library title:

1. reads `media`, its genres and (for a series) its seasons from SQLite —
   three queries, however many seasons;
2. renders that at once (the search-time metadata if details were never
   fetched);
3. decides by the policy below whether to ask TMDB, on the worker pool;
4. on success commits the answer in one transaction (ADR-0014);
5. re-reads SQLite and renders again.

The pane is never blanked while a request runs; a refresh indicator shows.

### Freshness policy

One interval for all provider metadata: **`FRESH_FOR` = 7 days**
(`metadata::FRESH_FOR`).

| Stored timestamp | State | Opening the title |
| --- | --- | --- |
| `NULL` | never fetched | request in the background |
| age ≤ 7 days | fresh | **no request** |
| age > 7 days | stale | request in the background; cached detail stays |

- The timestamp for details is `media.details_fetched_at`; for a season's
  episodes, `seasons.episodes_fetched_at` (ADR-0015). A timestamp in the
  future counts as fresh.
- **Refresh** (the button) always asks TMDB for the title and the shown
  season, whatever their age, when a token exists.
- Without a token nothing is requested and nothing is lost; the pane says
  that TMDB can be connected in Settings. When a token appears, the policy is
  applied again to what is on screen.
- **Settle delay**: the policy runs once a title or season selection has
  stayed put for `detail::SETTLE` (300 ms, the search debounce). Moving
  through the library with the keyboard renders every cached detail it
  passes, instantly, but requests nothing for titles passed over.
- Time comes from an injected `metadata::Clock` (the system clock in the
  app, a hand-moved clock in tests); nothing else reads real time for this.
- No adaptive or background synchronization: nothing is refreshed that is
  not on screen.

### Failures

- Every remote failure with a cached detail is a **non-destructive notice**
  next to the unchanged detail: offline/timeout ("Can't reach TMDB. Showing
  the details saved on this computer."), token rejected, rate limited, server
  error, malformed answer. Nothing stored is touched: the timestamp stays, so
  the title remains stale and Refresh stays available.
- Before a first successful fetch the pane still has the search-time metadata
  (every library title does), so the same notice plus Refresh is the
  retryable state.
- **HTTP 404 never deletes anything.** Local media, membership and cached
  details stay; the notice says TMDB no longer has the title. User-owned
  local state wins; automatic destructive reconciliation is not allowed.
- A failed SQLite commit is logged and shown the same way; the previous
  cached state stays because the transaction rolled back.

### Stale results

A new title starts a new title generation; a new title or season a new season
generation. An answer is always saved under the local id (and season) it was
requested for — it is correct data for that title — but reaches the pane only
while its generation is current. So title A answering after B is selected
updates A's cache, not B's pane.

### Artwork

**Poster-first.** The detail pane shows the cached `w185` poster through the
existing service (ADR-0011, ADR-0012) at 100×150. No new size and no
backdrops: `backdrop_path` is stored for later but never downloaded, so no
second cache family, RAM accounting or preloading exists.

## Alternatives considered

- **Always refresh on open**: simple, but a request per click and a visible
  change on every open; fails the "fresh opens without network" rule.
- **Per-field or adaptive intervals** (returning series daily, ended series
  monthly): better freshness for airing shows, but a policy per status that
  R9 does not need yet.
- **`append_to_response=season/1,season/2,…`**: one request for everything,
  but downloads every season eagerly and hits TMDB's 20-append limit.
- **`w342` detail poster and `w780` backdrops**: sharper, at the cost of a
  second and third image family in both caches. Deferred until visual
  quality demands it.

## Consequences

### Positive

- Opening a fresh title costs no network at all; offline use is the same as
  online use minus Refresh.
- One timestamp rule, testable with a fake clock.
- A 404 or outage can never shrink the library.

### Negative / risks

- A returning series can be up to a week stale unless the user presses
  Refresh.
- The detail poster is soft on high-DPI screens.
- Detail writes run on the UI thread (like R8's add), one transaction each:
  informally 4–9 ms per commit on a file database, dominated by the disk
  sync (`metadata::tests::informal_detail_timings`).

## Validation

`metadata::tests` (freshness boundaries, persistence, refresh, failure keeps
data), `tmdb::tests` (endpoint paths, mapping, malformed payloads),
`detail::tests` (never fetched → request, fresh → none, stale → background,
manual → always, failure keeps cache, 404 keeps title, late answers dropped,
offline after one fetch).

## Revisit trigger

Users see stale airing series; a language setting; a need for credits,
backdrops or higher-resolution artwork; measured UI-thread cost of commits.
