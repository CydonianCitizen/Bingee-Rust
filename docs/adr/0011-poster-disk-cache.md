# ADR-0011: Production posters: TMDB image configuration and the disk cache

- Status: Proposed
- Date: 2026-09-14

## Context

Library and Discover rows show 40×60 thumbnails and the detail pane a
160×240 poster. Library posters must survive restarts and work offline.
The R3 pipeline read synthetic files synchronously; it stays in the
benchmark fixture untouched. TMDB image URLs are `base_url` + size + file
path, with the base from `/3/configuration` (checked 2026-09-14:
<https://developer.themoviedb.org/reference/configuration-details> and
<https://developer.themoviedb.org/docs/image-basics>;
`secure_base_url` `https://image.tmdb.org/t/p/`, poster sizes `w92`, `w154`,
`w185`, `w342`, `w500`, `w780`, `original`).

## Decision

- **One size: `w185`** (185×278) for rows and the detail pane. A row needs
  40×60 (80×120 at 200 %) and the detail pane 160×240, so `w185` is sharp in
  rows at any scale and in the detail pane at 100 %, slightly soft at 200 %.
  About 10–20 KB on disk and 154 KB decoded per poster. `original` is never
  used for thumbnails.
- **Configuration**: `tmdb::ImageConfig` holds the base URL and the size. It
  starts from TMDB's documented default and is replaced by `/3/configuration`
  once per session, after the token checks out. Nothing that opens the
  library waits for it; downloads use whatever is current. Image downloads
  themselves carry no token.
- **Key**: a poster is identified by its TMDB file path and size, never by
  the title's id: two titles with the same path share one file, and TMDB
  movie 100 and TV 100 with different posters can never meet. Paths are
  accepted only if they match `/` + 1–64 of `[A-Za-z0-9_-]` + `.jpg`,
  `.jpeg` or `.png`; anything else is treated as "no poster". The file name
  is `tmdb-w185-<stem>.<ext>`: injective, no separators, no traversal.
- **Disk cache**: `<cache dir>/posters/`. Bytes are stored as TMDB sent them.
  A download is decoded (validated) in memory first, written to
  `.tmp-<pid>-<n>-<name>`, flushed and synced, then renamed over the final
  name. A partial file therefore never carries a final name. Temp files
  older than an hour are removed at startup. A cached file that no longer
  decodes is deleted and fetched again. No BLOBs in SQLite; the database
  keeps only the TMDB path.
- **Order**: RAM → disk → HTTP, all off the UI thread except the RAM lookup.
  Loads run on the R7 worker pool; an in-flight set ensures one transfer per
  key however many rows ask.
- **Failures** degrade to the placeholder, never to an error page. No
  poster path or an invalid path: nothing requested. HTTP 404 and undecodable
  bytes: not retried this session. Network, timeout, server and disk-write
  errors: retried after 60 s at the earliest (a failed disk write still shows
  the decoded image). Each key's first failure is logged once.
- **No garbage collection** in R8: files stay after a title is removed.
  Disk cache GC is deferred until ownership rules exist.

## Alternatives considered

- **Hard-coded base URL only**: works today, but ignores TMDB's documented
  source of truth. The default is kept only as a fallback.
- **Two sizes (`w92` rows, `w342` detail)**: better detail sharpness at
  200 %, twice the downloads, files and cache entries.
- **Hash of the path as file name**: needs a stable hash (none in `std`), and
  still has to reject junk to be safe; the strict whitelist makes a hash
  unnecessary.
- **SQLite BLOBs**: bloats the database and its backups with re-downloadable
  data.
- **Decode on the UI thread (R3 style)**: fine for local fixture files, but
  validation must happen before a download is kept, and that belongs on a
  worker.

## Consequences

### Positive

- Posters of library titles are offline after one successful download and
  survive restarts; the database never waits for images.
- Crashes, truncated transfers and corrupt data cannot poison the cache.

### Negative / risks

- The cache grows until GC exists (roughly 15 KB per title).
- Posters with unusual path characters are not shown.
- Detail posters are slightly soft on 200 % displays.

## Validation

Tests with a fake image server: first download, disk hit, restart disk hit,
corrupt, truncated and 404 responses, missing path, malicious paths,
duplicate simultaneous requests, atomic write, temp-file cleanup.

## Revisit trigger

A detail view that needs larger images (R9), disk usage reports, or TMDB
changing its image URL scheme.
