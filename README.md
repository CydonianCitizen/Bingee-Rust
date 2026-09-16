# Bingee Desktop

Bingee Desktop is a Rust + Slint local-first desktop media tracker for
Windows, macOS and Linux. It keeps your library of movies and TV series in a
local SQLite database. `main` is the development branch.

**History.** Milestones R0–R5 were a technology spike: this Rust + Slint
implementation (built with a coding agent) was compared with a C# + Avalonia
implementation in a separate repository, on the same UX slice, dataset and
benchmark protocol. Rust + Slint was chosen. The spike's contract and evidence
are preserved: `BENCHMARK_SPEC.md`, `benchmark/`, `docs/measurements/`. Since
R6 this repository is the canonical Bingee Desktop implementation, and the R4
workload lives on as a benchmark fixture for regression checks.

## Product constraints

Desktop-first (not a port of Bingee Android), local-first, cross-platform
(Windows, macOS, Linux), highly responsive, conservative in RAM/CPU use, and
designed to scale to large personal libraries without eagerly loading
everything into memory.

## Status

R6 (production foundations), R7 (TMDB search), R8 (local-first library) and
R9 (cache-first details, seasons and episodes) and R10 (personal tracking,
progress and watch history) are implemented: find movies and TV series on TMDB
with your own token, add them to a library stored in SQLite, browse, search,
filter and sort it with cached posters, open each title's details — for a
series its seasons and episodes — and track what you watched: movies and
episodes, whole seasons, a personal 1–10 rating, series progress and a watch
history. Everything works offline.

**Not implemented yet:** statistics, calendar, notifications, a Home /
Continue Watching page, backup and sync. See `IMPLEMENTATION_PLAN.md` and
`docs/milestones/R6.md` to `R10.md`.

## Prerequisites

- Rust stable via [rustup](https://rustup.rs) (developed with 1.98.1).
- A C compiler, because SQLite is compiled from source (`rusqlite`'s `bundled`
  feature). The platform toolchains below already provide one.
- Windows: the MSVC C++ build tools (Visual Studio Build Tools, "Desktop
  development with C++"). Built, tested and packaged locally.
- Linux: gcc or clang, `pkg-config`, and the fontconfig development package
  (`libfontconfig-dev` on Debian/Ubuntu). That is the only system library
  needed at build time: Wayland, X11, xkbcommon and EGL/GL are loaded at run
  time, so a desktop session needs them installed. Checked by CI only.
- macOS: Xcode command line tools. Checked by CI only.

## Run Bingee Desktop

```bash
cargo run            # debug build, with a console for stderr
cargo run --release  # release build (no console window on Windows)
```

The first start creates an empty library and shows "Your library is empty".
The working directory never matters.

To keep a development build away from your real library, point
`BINGEE_HOME` at an absolute folder; data, cache and logs then go to
`<folder>/data`, `<folder>/cache` and `<folder>/logs`:

```powershell
$env:BINGEE_HOME = "$env:TEMP\bingee-dev"; cargo run   # PowerShell
```
```bash
BINGEE_HOME=/tmp/bingee-dev cargo run                  # bash
```

### Where your data lives

| | Database (`bingee.db`) | Cache | Log (`bingee-desktop.log`) |
| --- | --- | --- | --- |
| Windows | `%LOCALAPPDATA%\Bingee Desktop\data\` | `%LOCALAPPDATA%\Bingee Desktop\cache\` | `%LOCALAPPDATA%\Bingee Desktop\logs\` |
| macOS | `~/Library/Application Support/Bingee Desktop/` | `~/Library/Caches/Bingee Desktop/` | `~/Library/Logs/Bingee Desktop/` |
| Linux | `$XDG_DATA_HOME/bingee-desktop/` (default `~/.local/share/…`) | `$XDG_CACHE_HOME/bingee-desktop/` (default `~/.cache/…`) | `$XDG_STATE_HOME/bingee-desktop/` (default `~/.local/state/…`) |

Settings and About show the exact resolved paths and the database schema
version. Nothing is written beside the executable. Posters are cached in the
cache folder's `posters/` subfolder (`tmdb-w185-<name>.jpg`, about 10–20 KB
each); deleting it only means they are downloaded again. See
`docs/adr/0006-production-storage-and-schema-v1.md` and ADR-0011.

If the database cannot be opened (for example damaged, not a Bingee
database, or created by a newer version), the window shows "Your library could not be
opened" with the reason, the database and log paths, and **Try again**. The
file is never deleted, replaced or "repaired" automatically.

## Connect TMDB (Discover)

Discover searches [The Movie Database (TMDB)](https://www.themoviedb.org)
with your own **API Read Access Token**:

1. Create a free TMDB account and request an API key in your account settings
   (themoviedb.org → Settings → API).
2. Copy the long **API Read Access Token** (not the shorter "API Key").
3. In Bingee Desktop, open **Settings → TMDB**, paste it, and choose
   **Validate and save**.

Bingee checks the token with TMDB (`GET /3/authentication`) and saves it only
if TMDB accepts it. A rejected token, or one that could not be checked
because TMDB was unreachable, is not saved. **Check again** re-validates the
saved token; pasting a new one replaces it only once TMDB accepts it.

**Where the token is kept:** only in your system's credential store: Windows
Credential Manager (a generic credential named
`tmdb-api-read-access-token.bingee-desktop`), the macOS Keychain, or the Secret
Service keyring on Linux (GNOME Keyring, KWallet). Never in the database,
a settings file, the log, or Git. The app shows only its last four characters.
On Linux, a session without a running Secret Service cannot save it; Settings
then says "Secure storage unavailable".

**Removing it:** **Settings → TMDB → Remove token**. Discover stops at once;
your library and everything else are untouched. You can also delete the entry
in the credential store yourself.

**Offline:** Library, Settings, About and all local data work without a
network. Discover then says it can't reach TMDB and offers **Try again**; no
network failure touches the database.

Searching both movies and TV series sends two requests per page, 300 ms after
you stop typing, in English (`en-US`) and without adult titles. Results show
in TMDB's order, movies and series alternating, with **Load more** for further
pages. See ADR-0007 to ADR-0009.

## Your library

- **Add**: in Discover, select a result and choose **Add to Library** (or
  press Enter in the result list). The title, original title, type, date,
  overview and poster path are saved in one SQLite transaction; the result
  then shows **In Library**, and the Library page lists it at once. Adding a
  title that is already there changes nothing. TMDB movie 603 and TMDB TV
  series 603 are different titles.
- **Browse**: the Library page reads only the local database. Search matches
  titles and original titles (any case, any script); **All / Movies / TV**
  filters; the sort menu orders by **Recently added** or **Title** (titles
  sort case-insensitively by character code, so accented first letters come
  after Z). Up/Down/PageUp/PageDown/Home/End move the selection; Tab reaches
  the filters, the sort menu and the detail pane.
- **Remove**: select a title and choose **Remove** in its detail pane. Only its
  library membership goes; its saved metadata and cached poster stay, so
  adding it again is instant and keeps its identity.
- **Posters** download in the background the first time a title is on
  screen and are kept on disk. Missing or broken posters show the colored
  placeholder; they never block the library.
- **Offline**: the library, its search, filters, sort, remove, cached
  posters, cached details, seasons and episodes, Settings and About all work
  without a network or a TMDB token. Discover then says it can't reach TMDB;
  uncached posters stay placeholders.

## Details, seasons and episodes

Selecting a library title opens its details in the right-hand pane.

- **Cache first.** What is saved on this computer shows immediately; the
  network is never needed to open a title. The first time, that is what the
  search gave (title, date, overview, poster); the full details then download
  in the background and the pane updates when they are saved.
- **Movies**: poster, title and original title, year, tagline, release date,
  runtime, status, genres, overview. **Series**: first and last air date,
  status, number of seasons and episodes, episode length, genres, overview,
  and the season list. Anything TMDB does not provide is simply left out.
- **Seasons and episodes**: choose a season to see its air date, episode
  count, overview and episodes (number, name, air date, runtime; the selected
  episode shows its overview). **Specials** (TMDB season 0) are listed like
  any season. Only the season you open is downloaded — opening a long series
  costs two requests, not one per season. Each season says whether its
  episodes are saved ("Not downloaded", "8 of 10 saved").
- **Freshness**: saved details and episode lists count as fresh for **7 days**.
  Opening a fresh title sends nothing to TMDB; an older one shows its saved
  details at once and refreshes in the background. **Refresh** always asks
  TMDB for the title and the season on screen. The pane says when the details
  were last updated.
- **Failures never remove anything.** Offline, a rejected token, rate
  limiting or TMDB errors leave everything saved on screen with a short
  explanation, and Refresh / Try again stay available. If TMDB no longer has
  a title, it stays in your library with its saved details.
- **Removing** a title from the library keeps its details and episodes for a
  later re-add, like its poster.

Details use TMDB's `/3/movie/{id}`, `/3/tv/{id}` and
`/3/tv/{id}/season/{n}` in English. The detail pane uses the cached `w185`
poster; backdrops are not downloaded. See ADR-0013 to ADR-0015.

## Tracking what you watch

Bingee keeps two kinds of data apart:

| TMDB metadata (refreshable) | Your personal Bingee data (never touched by TMDB) |
| --- | --- |
| titles, dates, overviews, genres, seasons, episodes, poster paths | library membership, watched state and dates, ratings, watch history |

The detail pane shows your data in its own **Your tracking / Your progress**
box, separate from the TMDB header and its Refresh button.

- **Movies**: **Mark as watched** records the date and one history entry;
  pressing it again does not add another. **Mark unwatched** clears the state
  but keeps the history. **Watch again** records a rewatch.
- **Rating**: **Your rating** is a whole number from 1 to 10, or Not rated.
  It does not mark anything watched and is never TMDB's score.
- **Episodes**: click the circle beside an episode, or select it and press
  **Space** or **Enter**, to mark it watched or unwatched.
- **Seasons**: **Mark season watched** marks every downloaded episode of the
  season in one step (already watched ones keep their date); **Unmark** clears
  them. History is kept.
- **Progress** counts downloaded regular episodes: "12 / 24 episodes watched ·
  50%". When some episode lists are not downloaded or TMDB lists more episodes
  than are saved, it says "downloaded episodes watched · episode list
  incomplete" and never claims the series is finished. A series is **watched**
  only when every regular season is fully downloaded and watched; when TMDB
  adds an episode, it appears unwatched and the series is in progress again.
  The pane also shows the **next episode** (season order, then episode).
- **Specials** (season 0) can be marked like any episode but are counted apart
  ("1 / 8 specials watched") and never block completion.
- **Library rows** show "Watched" for movies and "12 / 24 · 50%" (or
  "… known · incomplete") for series you started.
- **History** (sidebar): recent watches, newest first, with local date and
  time. **Remove from history** (or **Delete**) removes that entry only; it
  does not unmark anything.
- **Kept safe**: refreshing from TMDB, TMDB removing an episode or title,
  removing a title from the library and adding it back, and restarts never
  change your tracking. If a change cannot be saved, the pane says so and keeps
  showing what was saved.
- **Private**: tracking works without a network or token, is never sent to
  TMDB or anywhere else, and is not written to the log.

See ADR-0016 to ADR-0019.

## Run the benchmark fixture (R4 workload)

The 1,000-title deterministic library, the spike database and the synthetic
posters are not part of a normal build. They need a Cargo feature *and* a
flag:

```bash
cargo run --release --features benchmark-fixture -- --benchmark-fixture
```

The fixture keeps the R5 portable layout: its database is
`<exe dir>/data/bingee-spike.db` (seeded on first launch; delete it to
reseed) and its posters come from `<exe dir>/assets/posters/`, or from the
checkout's `benchmark/assets/posters/` for a build-tree binary. It never
touches the per-user folders, the network or the credential store. The
sidebar footer reads "Benchmark fixture".

The opt-in R4 latency harness (ADR-0004) is the `r4-measurement` feature,
which includes the fixture:

```bash
cargo build --release --features r4-measurement
target/release/bingee-desktop --r4-latency sqlite|posters|ui <output.csv>
```

`benchmark/scripts/r4-core.ps1` and `r3-memory.ps1` build or launch the
fixture explicitly. A default build refuses `--benchmark-fixture` (exit code
1), so a benchmark cannot silently measure the empty production app. The
frozen R4 executables and database under `target/release/` are unchanged; see
`docs/measurements/R4-rust-slint/STATUS.md`. Never compare numbers from
different workloads (`BENCHMARK_SPEC.md`).

## Checks

```bash
cargo fmt --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

Tests never touch the real per-user folders, the network or the credential
store: they use temporary directories, in-memory databases, an in-memory
secret store, and a scripted HTTP server on `127.0.0.1` instead of TMDB. `.github/workflows/cross-platform.yml` runs these on
Windows, Ubuntu and macOS, plus clippy for the default and fixture-only
feature sets.

## Portable Windows package

```powershell
pwsh -NoProfile -File scripts/package-windows.ps1        # -> dist/bingee-desktop-windows-x64/
pwsh -NoProfile -File scripts/smoke-windows-package.ps1 -WorkDir $env:TEMP\bingee-smoke
```

The package holds `bingee-desktop.exe`, `THIRD_PARTY_NOTICES.txt` (with the
generated list of linked crates), `README.txt` and `SHA256SUMS.txt`. It writes
nothing to its own folder, so it may live in a read-only location. It needs
the Visual C++ 2015–2022 Redistributable (x64). The smoke test runs a fresh,
a second and a corrupt-database start from an unrelated working directory,
with `LOCALAPPDATA` redirected to a temporary folder. It is not an installer
and is not signed.

## What the app does (R10)

- **Sidebar**: Home, Library, History, Discover, Calendar, Statistics,
  Settings, About. Home, Calendar and Statistics are labelled placeholders.
- **Library**: your titles from SQLite, with search, a Movie/TV filter, a
  sort menu, a virtualized list with posters, and states for an empty
  library, no matches and database errors. The detail pane shows cached
  details with **Refresh** and **Remove**, your tracking (watched, rating,
  progress, next episode), and for series a season list with per-season
  progress and bulk actions and the selected season's episodes with watched
  marks. Rows show your progress.
- **History**: recent watches, newest first, with Remove from history.
- **Discover**: TMDB search over movies and TV series, with each result's
  type, year, poster and **In Library** state, a preview pane with **Add to
  Library**, **Load more**, keyboard navigation, and a clear state for a
  missing or rejected token, no network, rate limiting, TMDB errors and no
  results.
- **Settings**: the TMDB token (validate and save, check, replace, remove),
  where the database, data, cache and log live, the schema version, and a
  link to About.
- **About**: version, Rust/Slint/SQLite credits, the "Made with Slint"
  widget, TMDB logo and notice, license status, data locations.
- **Startup error page** instead of an empty library when the database
  cannot be loaded.

## Code layout

- `src/main.rs`: identity constants (`APP_NAME`, and `APP_ID`/`APP_VERSION`
  from Cargo), startup, the error page wiring, the `--benchmark-fixture` and
  `--r4-latency` dispatch.
- `src/paths.rs`: the per-OS data/cache/log policy (pure, tested for all
  three OSes on any host).
- `src/settings.rs`: Bingee's own settings; today only `BINGEE_HOME`.
- `src/database.rs`: `Database` (one owned connection), schemas v1 to v3,
  migrations.
- `src/library.rs`: library reads and writes: search with filter and sort,
  count, add, remove, membership, provider identity. No Slint, no network.
- `src/metadata.rs`: provider detail metadata (`MediaDetails`, `Season`,
  `Episode`, `Genre`), the freshness policy and injectable `Clock`, coverage,
  and their SQLite reads and transactional writes. No Slint, no network.
- `src/tracking.rs`: personal tracking: watched state, ratings, season bulk
  actions, watch history, coverage-aware progress, next episode, continue
  watching. SQLite only; no Slint, no network.
- `src/history.rs`: the History page.
- `src/detail.rs`: the Library detail pane (metadata and tracking): cache-first loading, refresh
  decisions, background requests, stale-answer protection, view models.
- `src/view.rs`: the `slint::Model` over search results and the selection
  logic, shared by the production library and the fixture.
- `src/error.rs`: `AppError` (kind, user message, cause).
- `src/diagnostics.rs`: the local log file and panic hook.
- `src/secrets.rs`: the redacted `Token`, the `SecretStore` trait and the
  OS credential store behind it.
- `src/tmdb.rs`: the TMDB client (ureq), its private DTOs, error mapping, and
  the scripted fake server used by tests.
- `src/search.rs`: `MediaSearchResult`/`ExternalRef` and the search
  controller (debounce generations, stale-response rules, paging). No Slint,
  threads or clock.
- `src/network.rs`: the worker pool and the handoff back to the UI thread.
- `src/poster.rs`: poster keys, the disk cache, validating decoder, bounded
  RAM cache, and the loading service shared by both lists.
- `src/remote.rs`: Discover and the TMDB settings wired to the window.
- `src/fixture/`: the benchmark fixture: `library.rs` (1,000-record
  generator and reference search), `db.rs` (spike schema, seed, SQL search),
  `poster.rs` (poster mapping and bounded LRU cache), `mod.rs` (fixture paths,
  poster loader, startup). Tests and `benchmark-fixture` builds only.
- `ui/app-window.slint`: layout, pages, visuals and keyboard handling.
- `benchmark/`: the frozen R4 workload contract, the R4 harness
  (`r4_latency.rs`), benchmark scripts, and the shared posters
  (`examples/generate_posters.rs` recreates them).
- `scripts/`: Windows packaging and smoke test.

## Database

`rusqlite` 0.40 with SQLite 3.53.2 compiled in (`bundled`), plus its
`functions` feature for the Unicode case-folding search function. No ORM,
connection pool or async runtime. One `Database` owns the only connection; the
Library page and Discover share it through `SharedDb`, used on the UI thread
only, and it closes when the window closes. Add, remove and membership rules
are in ADR-0010.

Schema v1 (ADR-0006), all `STRICT` tables:

- `media`: local metadata. `local_media_id` (never reused), `media_type`
  (`movie`/`tv`), `title`, `original_title`, `release_date` (`YYYY-MM-DD`),
  `overview`, `poster_path`, `backdrop_path`, `runtime_minutes`,
  `metadata_updated_at` (Unix seconds, UTC).
- `external_refs`: provider identity, primary key `(source, media_type,
  external_id)`. TMDB movie 603 and TMDB TV 603 are different rows; a
  duplicate triple is rejected; a ref's type must match its media row
  (composite foreign key); TMDB ids must be plain positive decimals.
- `library_entries`: membership, `local_media_id` → `media` (`ON DELETE
  RESTRICT`), `added_at`. Cached metadata is not library membership.

Schema v2 (R9, ADR-0014) adds provider metadata only:

- `media` columns `status`, `tagline`, `last_air_date`, `season_count`,
  `episode_count`, `details_fetched_at`.
- `genres (source, external_id, name)` and `media_genres`, one row per title
  and genre.
- `seasons`, keyed by `(local_media_id, season_number)` (season 0 allowed,
  series only), with the summary and the episode coverage
  (`episodes_fetched_at`, `episodes_known`).
- `episodes`, keyed by `(local_media_id, season_number, episode_number)`,
  with the TMDB episode id unique per season.

Schema v3 (R10, ADR-0016) adds personal data only, in its own tables that
reference `media` with `ON DELETE RESTRICT` and have no foreign key to
`seasons` or `episodes`, so provider reconciliation can never cascade into
them:

- `media_tracking (local_media_id, media_type, watched_at, rating)`: a movie's
  latest watch (Unix seconds, UTC) and the 1–10 rating of any title.
- `episode_tracking`, keyed like `episodes` (series, season, episode number):
  a row means watched, with `watched_at`.
- `watch_events (event_id, local_media_id, media_type, season_number,
  episode_number, watched_at)`: one row per watch, rewatches included, indexed
  by time.

Older databases are upgraded in place on first start (v1 → v2 → v3); library,
identities, details, genres, seasons, episodes and coverage are kept. A future
backup of personal data is `library_entries`, the three tracking tables and
`external_refs`, never the token, caches or logs.

Migrations: `PRAGMA user_version` is the schema version and `PRAGMA
application_id` marks the file as Bingee's. All pending steps run in one
transaction; a failure leaves the file as it was. Files that are damaged,
belong to another application (including the spike's `bingee-spike.db`), or
come from a newer version are refused without writing.

## Diagnostics

A plain-text log, `bingee-desktop.log` in the log folder above, also echoed
to stderr: startup (version, OS, architecture), the database path, migration
start/end, schema version and title count, failures with their causes,
panics, and close. For TMDB: token checks and saves (never the token), and one
line per completed search request with its generation number, type, page,
result count or error category, and duration (never the query text). For
details: whether an opened title came from the local cache and was fresh,
stale or never fetched, each detail or season request with its outcome and
duration, and answers dropped because another title or season is shown —
never the response bodies. For tracking, only failed writes or reads (never
what was watched). It is rotated to `bingee-desktop.log.1` above 1 MiB. There
is no telemetry, analytics or remote logging; no per-interaction events or
secrets are logged.

## Keyboard (Library and Discover)

| Key | Effect |
| --- | --- |
| Tab / Shift+Tab | Move focus through the search field, the list, the Library filters and sort menu, Load more, and the detail pane's Refresh, Remove, rating menu, watched buttons, season list, season actions and episode list. |
| Up / Down / PageUp / PageDown / Home / End | In a list (titles, seasons, episodes, history): move the selection and scroll it into view. Up/Down/Home/End in the season list. |
| Space or Enter | In the episode list: mark the selected episode watched or unwatched. Buttons (Mark as watched, Watch again, Mark season watched, Unmark) and the rating menu are in the Tab order. |
| Delete | In History: remove the selected entry. |
| (switching pages) | Focus moves into the Library or Discover list. |
| Down or Enter | In a search field: move focus to the list. |
| Enter | In the Discover list: add the selected result to the library. |
| Esc | In a search field (or the Library list): clear the search. |
| F12 | In the list, fixture only: print poster cache counters to stderr. |

## Benchmark fixture internals (R1–R3)

These describe the fixture, which is unchanged from R5 apart from its module
location.

- **Dataset**: `generate_library()` returns exactly 1,000 records with ids
  1–1000. Ids 1–10 are the R0 sample titles; ids 11–1000 come from integer
  arithmetic on the record index (no RNG): an adjective/noun pair picked by
  `(k * 389 + 17) % 1000`, its German counterpart as the original title, and
  year, kind, progress and overview from fixed modulo formulas. A test pins
  sample values; the Avalonia spike reproduced this generator exactly.
- **Search**: case-insensitive substring match on title and original title,
  on every edit, no debounce, id order. SQL `instr(search_text, ?1)` over a
  `search_text` column lowercased in Rust at seed time, checked against the
  in-memory reference search for a fixed query set.
- **Selection**: stored as the stable id; kept after a search when still
  present, otherwise the first result, otherwise none. Shared with the
  production library (`view::reselect`).
- **Large list**: `LibraryView` implements `slint::Model` inside the std
  `ListView`, which instantiates only the rows in the viewport. The current
  results stay in Rust memory; only visible rows become `MediaRow`s.
- **Posters**: 100 synthetic 240×360 JPEGs; record `id` shows poster
  `(id − 1) % 100 + 1`. Loaded synchronously with `Image::load_from_path`
  only for requested rows and the selection, through one LRU bounded at
  12 MiB of estimated decoded size (36 posters). A missing or corrupt file is
  logged once and shown as the placeholder. See ADR-0003 and
  `docs/measurements/R3-posters-informal.md`.
- **Debug builds** decode about 30× slower (unoptimized dependencies), so
  scrolling the fixture in a debug build is not representative.

## Licensing

Slint 1.17 is offered under GPL-3.0-only, the Slint Royalty-free Desktop,
Mobile, and Web Applications License 2.0, or a commercial license. Bingee
Desktop currently uses the Royalty-free 2.0 terms; this is the working
assumption, not a final decision, and no commercial license has been
obtained. That license's attribution condition is met by the About page,
which is a top-level sidebar entry and shows the `AboutSlint` widget.

`THIRD_PARTY_NOTICES.txt` records this, SQLite (public domain), the direct
dependencies and their licenses; the packaging script appends every linked
crate with its license expression. Full license texts of the permissive
crates are not bundled yet; that is required before public distribution.

The license of Bingee Desktop itself is not decided
(see `docs/adr/0001-rust-slint-spike.md`).

## Working with Claude Code

Instructions live in `CLAUDE.md` (local, git-ignored), which Claude Code
loads automatically. The rest of the setup:

| File | Role |
| --- | --- |
| `.claude/settings.json` | Permissions and hooks. Denies commits, pushes, and destructive git; prompts for `cargo add` and `cargo run`; formats Rust after edits. |
| `.claude/rules/rust-slint.md` | Coding rules that load only when `.rs`, `.slint`, or `Cargo.toml` files are in play. |
| `.claude/skills/` | `/milestone`, `/check-rust`, `/benchmark-run`, `/adr`. |
| `.claude/agents/spike-reviewer.md` | Read-only reviewer subagent, used before calling a milestone done. |
| `docs/milestones/` | One brief or record per milestone. |

The `cargo fmt` hook runs through bash; on Windows it needs Git Bash. Prompts
blocked by a deny rule are intentional. If one gets in your way, change the
rule deliberately rather than working around it in a session.
