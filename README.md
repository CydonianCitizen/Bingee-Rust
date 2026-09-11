# Bingee Desktop — Technology Spike

This repository starts as a controlled comparison between two independent
desktop implementations of Bingee:

- `spike/rust-slint` — Rust + Slint, implemented with a coding agent.
- `spike/csharp-avalonia` — C# + Avalonia, implemented manually with assisted
  guidance.

The spike is **not** the production application. Its purpose is to compare the
two stacks on the same UX slice, dataset, and benchmark protocol before
choosing the production stack.

## Product constraints

Bingee Desktop is desktop-first (not a port of Bingee Android), local-first,
cross-platform (Windows, macOS, Linux), highly responsive, conservative in
RAM/CPU use, and designed to scale to large personal libraries without eagerly
loading everything into memory.

## Shared spike slice

Both implementations must eventually provide the same test slice:

1. desktop shell with sidebar, library pane, and detail pane
2. 1,000 deterministic fake media items
3. local search
4. media selection and detail update
5. a small SQLite-backed query
6. lazy/virtualized list behavior
7. bounded image/poster caching
8. the measurements in `BENCHMARK_SPEC.md`

Never compare benchmark results from different workloads.

## Working with Claude Code

Instructions live in `CLAUDE.md`, which Claude Code loads automatically. The
rest of the setup:

| File | Role |
| --- | --- |
| `.claude/settings.json` | Permissions and hooks. Denies commits, pushes, and destructive git; prompts for `cargo add` and `cargo run`; formats Rust after edits. |
| `.claude/rules/rust-slint.md` | Coding rules that load only when `.rs`, `.slint`, or `Cargo.toml` files are in play. |
| `.claude/skills/` | `/milestone`, `/check-rust`, `/benchmark-run`, `/adr`. |
| `.claude/agents/spike-reviewer.md` | Read-only reviewer subagent, used before calling a milestone done. |
| `docs/milestones/` | One brief per milestone. |

Typical session:

```text
/milestone R0          # start the milestone in plan mode
…                      # implement
/check-rust            # fmt, check, test, clippy
```

`.claude/settings.json` contains `permissions.allow` rules, so Claude Code asks
you to trust the workspace the first time you open this repo. Review the rules
in that file before accepting.

The `cargo fmt` hook runs through bash; on Windows it needs Git Bash. Delete the
`hooks` block from `.claude/settings.json` if you would rather not use it.

Prompts blocked by a deny rule are intentional. If one gets in your way, change
the rule deliberately rather than working around it in a session.

## Rust spike

See `IMPLEMENTATION_PLAN.md` for the full roadmap and `docs/milestones/` for
milestone briefs. R0, R1, and R2 are implemented.

### Prerequisites

- Rust stable via [rustup](https://rustup.rs) (developed with 1.98.1).
- A C compiler, because SQLite is compiled from source (`rusqlite`'s `bundled`
  feature). The platform toolchains below already provide one.
- Windows: the MSVC C++ build tools (Visual Studio Build Tools, "Desktop
  development with C++").
- Linux: the system libraries Slint's winit/femtovg backend needs (fontconfig,
  xkbcommon, Wayland/X11 development packages) and gcc or clang. Not yet
  verified on this branch.
- macOS: Xcode command line tools. Not yet verified on this branch.

### Run

```bash
cargo run            # debug build
cargo run --release  # release build (no console window on Windows)
```

### Checks

```bash
cargo fmt --check
cargo check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
```

### Current scope: R2

A desktop shell with three panes over a 1,000-record library stored in SQLite.
R2 changed where the data comes from, not what the user sees:

- **Sidebar**: Home, Library, Discover, Calendar, Statistics, Settings.
  Only Library is active; the other entries are not wired up yet.
- **Library**: search field, result count, and a virtualized list of the
  matching titles. Each row has a gradient-and-initials poster placeholder,
  title, type, year, original title, and progress. A search with no matches
  shows a "No titles found" empty state.
- **Detail**: the selected title with type/year, original title, progress, and
  overview, or "No title selected" when the results are empty.
- **Errors**: if the database cannot be opened, created, seeded, or queried,
  the library pane shows the error in red instead of an empty library.

Code layout:

- `src/library.rs`: plain Rust domain types, the dataset generator, the
  selection policy, and the R1 in-memory search (now test-only, kept as the
  reference oracle). No Slint or SQLite types.
- `src/db.rs`: the SQLite schema, the seed, and the SQL search, with their
  tests. No Slint types.
- `src/main.rs`: the database path, `LibraryView` (a `slint::Model` over the
  current results that owns the connection), and the window callbacks.
- `ui/app-window.slint`: layout, visuals, and keyboard handling.

#### Database (spike only)

This is **not** the Bingee Desktop schema. It is one table sized for the R2
workload. See `docs/adr/0002-sqlite-via-rusqlite-bundled.md`.

- **Crate**: `rusqlite` 0.40 with the `bundled` feature, which compiles SQLite
  3.53.2 into the executable. No ORM, migrations, pool, or async runtime.
- **Location**: `bingee-spike.db` next to the executable, so
  `target/debug/` or `target/release/` during development. That directory is
  git-ignored and removed by `cargo clean`. Debug and release builds therefore
  keep separate, identically seeded databases. Delete the file to reseed. This
  location is spike behavior, not the production data directory.
- **Schema**: one `media` table: `local_id` (the stable R1 id), `title`,
  `original_title`, `year`, `media_type` (`'Movie'`/`'TV'`), `progress_state`
  (`'planned'`/`'watched'`/`'episodes'`), `progress_value`/`progress_total`
  (episodes watched/total, NULL otherwise), `overview`, and `search_text`.
  `CHECK` constraints reject unknown `media_type`/`progress_state` values, and
  the row mapper returns an error rather than guessing.
- **Seed**: on every launch, one transaction creates the table if it is
  missing and inserts the 1,000 `generate_library()` records only if the
  table is empty. A database that already has rows is left untouched, and a
  failed seed leaves no partial data.
- **Lifecycle**: `main` opens one `rusqlite::Connection` and moves it into
  `LibraryView`, which is the only owner. It is closed when the window closes.
  There is no global connection.
- **Limitations**: no migrations (after a schema change, delete the file). No
  personal-state/metadata split, external ids, seasons, genres, or ratings.
  `search_text` is derived data that every writer must keep in sync; the seed is
  the only writer in R2. Open, schema, and seed errors all appear as a single
  "Could not load the library" message with SQLite's text; they are not
  separated by step.

#### Dataset

`generate_library()` returns exactly 1,000 records with ids 1–1000 in id order.
Ids 1–10 are the R0 sample titles. Ids 11–1000 are built from the record index
`k` (0–989) with integer arithmetic only, so there is no RNG:

- The title is an adjective/noun pair picked by `(k * 389 + 17) % 1000`, a
  one-to-one mapping over the 25 × 40 word table, so every title is unique.
- The original title is the German counterpart of the same pair, for example
  "Pale Harbor" / "Blasse Hafen".
- Year, Movie/TV kind, progress, and overview come from other fixed modulo
  formulas; see `generated_record`.

A test pins sample values, so any change to the shared benchmark dataset is
deliberate. The Avalonia spike must reproduce this generator exactly.

#### Search and selection

- Search is local, case-insensitive substring matching on the title and the
  original title, run on every edit with no debounce. Results keep dataset
  (id) order, and an empty or whitespace-only query shows all 1,000 records.
  Accent folding is not done: "shogun" finds Shōgun only because its original
  title is "Shogun".
- Since R2 the search is SQL: `WHERE instr(search_text, ?1) > 0 ORDER BY
  local_id`, with the query trimmed and lowercased in Rust and bound as a
  parameter. SQLite's `lower()`/`LIKE` fold only ASCII, so the lowercased key
  is computed in Rust (`str::to_lowercase`, as in R1) and stored at seed time.
  "NÖRDLICHE" therefore matches "Nördliche". `instr` is a literal match, so `%`
  and `_` are not wildcards. A test checks that the SQL results equal the R1
  in-memory search (same ids, same order) for a fixed query set. There are no
  intentional differences.
- Selection is stored as the item's stable id, never as a list index.
- After each search, the selected id is kept if it is still in the results.
  Otherwise the first result is selected, or nothing if there are no results.
  The detail pane is rebuilt from the selected id every time, so it cannot show
  an item the filter removed.
- The filter path per keystroke, synchronously on the UI thread:
  - runs the cached prepared statement;
  - maps each matching row to an owned `MediaItem` (at most 1,000);
  - replaces the result `Vec` and resets the model. Slint then drops its
    current row components and instantiates only the ones that fit the viewport
    (nine at the default window size; checked in R2 with a temporary
    `row_data` trace), calling `row_data` for each.

  UI rows (`MediaRow`) are built only for rows the ListView shows, plus the
  detail row. In an informal release-build measurement, fetching all 1,000
  records took about 2–3 ms (median), and a no-match query about 0.3 ms. That
  is well inside a frame, so there is no worker thread. See
  `docs/measurements/R2-sqlite-informal.md`. If a search fails, the previous
  results stay and the error is shown.

#### Large list

`LibraryView` implements `slint::Model` and is bound to a `for` repeater
directly inside the std `ListView`. The Slint 1.17 compiler turns that into a
virtualized repeater: it creates components only for the rows in the viewport,
drops the ones that scroll out, and creates new ones as rows scroll in. Rows
still on screen are updated in place. There is no pool that recycles
components. The current results (up to 1,000 `MediaItem`s) stay in Rust
memory; only visible rows exist as components and `MediaRow` values. Loading
every matching record is a deliberate R2 simplification. For much larger
libraries, the view would hold ids only and load visible rows by id.

#### Keyboard

| Key | Effect |
| --- | --- |
| Tab / Shift+Tab | Move focus between the search field and the list (the cycle has only these two stops). |
| Up / Down / PageUp / PageDown / Home / End | In the list: move the selection and scroll it into view. |
| Down or Enter | In the search field: move focus to the list. |
| Esc | In the search field or the list: clear the search. |

Clicking a row selects it and focuses the list. A focused list shows an accent
outline on the selected row.

**Not implemented yet:** the production database schema, migrations, TMDB or
any network access, poster loading or caching, sidebar navigation, and
benchmarking (R4). The only numbers so far are the informal R2 SQL timings,
which make no comparative claims.

### Slint licensing (spike assumption only)

Slint 1.17 is offered under GPL-3.0-only, the Slint Royalty-free Desktop,
Mobile, and Web Applications License 2.0, or a commercial license. This spike
is an internal, undistributed evaluation and assumes the Royalty-free 2.0 terms
for evaluation purposes only. That license's attribution condition applies on
distribution: show the `AboutSlint` widget in an About screen, or put the
"Made with Slint" badge on the download page. Any build shared outside the
team must do one of these first.

This is **not** a decision about the Bingee Desktop product license, which
remains open (see `docs/adr/0001-rust-slint-spike.md`).
