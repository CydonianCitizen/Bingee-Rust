# ADR-0006: Production storage: per-user data directories, schema v1, `user_version` migrations

- Status: Proposed
- Date: 2026-09-14

## Context

R6 turns the R0–R5 spike into the production foundation. Until now the app
seeded 1,000 fake titles into an unversioned one-table database beside the
executable (ADR-0005). The product needs:

- writable data in the per-user locations each OS expects, never beside the
  executable and never relative to the working directory;
- a real, versioned schema that separates cached metadata from library
  membership, and makes TMDB Movie and TV ids impossible to confuse;
- upgrades that cannot leave a half-migrated file, and startup failures that
  never delete or recreate user data;
- the R4 benchmark workload kept reproducible, but out of normal startup.

Constraints: local single-user desktop app, one process, Windows/macOS/Linux,
safe Rust, a small dependency graph (no ORM, migration framework, pool, or
async runtime), no telemetry.

## Decision

**Data directories** are resolved by `src/paths.rs` from the platform's
standard environment, with `std` only:

| | data (database) | cache | logs |
| --- | --- | --- | --- |
| Windows | `%LOCALAPPDATA%\Bingee Desktop\data` | `…\Bingee Desktop\cache` | `…\Bingee Desktop\logs` |
| macOS | `~/Library/Application Support/Bingee Desktop` | `~/Library/Caches/Bingee Desktop` | `~/Library/Logs/Bingee Desktop` |
| Linux, other Unix | `$XDG_DATA_HOME/bingee-desktop` (`~/.local/share`) | `$XDG_CACHE_HOME/bingee-desktop` (`~/.cache`) | `$XDG_STATE_HOME/bingee-desktop` (`~/.local/state`) |

`BINGEE_HOME=<absolute dir>` replaces all three with `<dir>/data`,
`<dir>/cache`, `<dir>/logs`, for development builds and tests that must not
touch a real library. Relative values are rejected (`BINGEE_HOME`, `HOME`,
`LOCALAPPDATA`) or ignored (`XDG_*`, as the XDG spec requires), so no path can
depend on the working directory. The database is `<data>/bingee.db`.

Windows uses the non-roaming `%LOCALAPPDATA%`: a live SQLite file must not be
synchronized by roaming profiles.

**Schema v1** (`src/database.rs`), all tables `STRICT`:

- `media`: canonical local metadata. `local_media_id INTEGER PRIMARY KEY
  AUTOINCREMENT` (never reused), `media_type` (`'movie'`/`'tv'`), `title`,
  `original_title`, `release_date` (ISO `YYYY-MM-DD`), `overview`,
  `poster_path`, `backdrop_path`, `runtime_minutes`, `metadata_updated_at`
  (Unix seconds, UTC). All nullable except id, type and title.
- `external_refs`: provider identity. `PRIMARY KEY (source, media_type,
  external_id)`, so TMDB movie 603 and TMDB TV 603 are two different keys and
  a duplicate triple is rejected. A composite foreign key
  `(local_media_id, media_type) → media` makes the ref's type match the media
  row it points to; deleting cached media cascades to its refs. `CHECK`s keep
  the key canonical (lowercase `source`, TMDB ids as plain positive decimals).
- `library_entries`: membership, separate from metadata.
  `local_media_id INTEGER PRIMARY KEY → media ON DELETE RESTRICT`, `added_at`
  (Unix seconds, UTC). Metadata can be cached without being in the library,
  and removing cached metadata cannot silently drop a library entry.

Nothing else: no seasons, episodes, ratings, watch history, releases, or
settings table until a milestone needs them.

**Migrations**: an ordered list of SQL steps in Rust; step *n* produces
version *n*. `PRAGMA user_version` holds the version and `PRAGMA
application_id` (`0x42696E67`, "Bing") marks the file as Bingee's. On open, one
`BEGIN IMMEDIATE` transaction checks the file and applies every pending step,
then sets `user_version`; any failure rolls all of it back. The app refuses,
without writing, a file that is newer than it knows, belongs to another
application, or is a non-empty unversioned database (such as the spike's
`bingee-spike.db`). A failed open is never "repaired" by deleting or
recreating the file.

**Connection**: one `Database` value owns one `rusqlite::Connection`. It is
opened in `main` and moved into the library view. There is no global, no pool.
`PRAGMA foreign_keys = ON` is set and verified on every open. The default
rollback journal is kept (one file to copy or back up).

**Benchmark fixture**: the generator, the spike schema and the poster mapping
move to `src/fixture/`, compiled only for tests and the opt-in
`benchmark-fixture` Cargo feature (`r4-measurement` implies it). A default
build contains no fake data. The fixture keeps the ADR-0005 portable paths
(`<exe dir>/data/bingee-spike.db`, `assets/posters/`), which never overlap the
production directories.

**Diagnostics**: a plain-text log at `<logs>/bingee-desktop.log`, written
only at startup, migrations, fatal failures and panics. It is rotated to
`.log.1` above 1 MiB. Nothing leaves the machine.

## Alternatives considered

- **`directories`/`dirs` crate**: correct, small, and uses the Windows
  known-folder API. Rejected for now: it is a new runtime dependency (plus
  `dirs-sys`, `option-ext`) for about 40 lines of `std` that the tests can
  cover on all three OS policies from one host. Revisit if a platform case
  appears that environment variables do not cover.
- **`rusqlite_migration` / `refinery` / an ORM**: more machinery than one
  version integer and an array of SQL strings. They also bring their own
  version tables.
- **One transaction per migration step**: leaves a file at an intermediate
  version after a failure. One transaction for all pending steps leaves it
  either unchanged or fully current.
- **`UNIQUE(source, external_id)`**: rejected by requirement. TMDB movie and TV
  ids share one integer space.
- **A runtime flag for the fixture** (`--benchmark`): the fake data would ship
  in every release binary. The Cargo feature keeps it out.
- **`%APPDATA%` (roaming) on Windows**: roaming sync of a live SQLite database
  risks conflicts and quota problems.

## Consequences

### Positive

- A fresh install starts with an empty, versioned library. Upgrades are
  all-or-nothing, and user files are never deleted automatically.
- Movie/TV id collisions and type mismatches are rejected by SQLite itself.
- Path policy is a pure function of the environment, tested for every OS.
- R4 remains reproducible through an explicit build (`--features
  benchmark-fixture` or `r4-measurement`).

### Negative / risks

- Environment variables, not the Windows known-folder API: a machine with a
  missing or relative `LOCALAPPDATA` gets a configuration error instead of a
  fallback.
- macOS uses the display name, not a bundle identifier, for its folders.
  Moving to a bundle id later would need a one-time move.
- No single-instance guard: two running copies share one file. `BEGIN
  IMMEDIATE` and SQLite's locking keep migrations safe, but later write paths
  must handle `SQLITE_BUSY`.
- Every future schema change must be a new migration step. Editing a shipped
  step is forbidden.

## Validation

Unit tests in `src/database.rs` and `src/paths.rs`: fresh → v1, repeated open
leaves the file byte-identical, foreign keys on, duplicate triple rejected,
Movie/TV same id accepted, type mismatch rejected, failed migration rolls back
to the previous version, corrupt, foreign and newer files refused and left
byte-identical, data survives reopening, per-OS path resolution, relative
paths rejected, fixture paths separate. `scripts/smoke-windows-package.ps1`
checks fresh, second and corrupt-database starts of the packaged executable
from an unrelated working directory.

## Revisit trigger

A second process or sync feature (needs WAL and busy handling), a macOS
`.app` bundle with a bundle id, a packaging format with its own data
conventions (MSIX, Flatpak), or a platform report that the environment-based
resolution picks the wrong folder.
