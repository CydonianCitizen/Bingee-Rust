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
milestone briefs. R0 and R1 are implemented.

### Prerequisites

- Rust stable via [rustup](https://rustup.rs) (developed with 1.98.1).
- Windows: the MSVC C++ build tools (Visual Studio Build Tools, "Desktop
  development with C++").
- Linux: the system libraries Slint's winit/femtovg backend needs (fontconfig,
  xkbcommon, Wayland/X11 development packages). Not yet verified on this branch.
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

### Current scope: R1

A desktop shell with three panes over a 1,000-record in-memory library:

- **Sidebar**: Home, Library, Discover, Calendar, Statistics, Settings.
  Only Library is active; the other entries are not wired up yet.
- **Library**: search field, result count, and a virtualized list of the
  matching titles. Each row has a gradient-and-initials poster placeholder,
  title, type, year, original title, and progress. A search with no matches
  shows a "No titles found" empty state.
- **Detail**: the selected title with type/year, original title, progress, and
  overview, or "No title selected" when the results are empty.

Code layout:

- `src/library.rs`: plain Rust domain types, the dataset generator, search, and
  the selection policy. No Slint types; all unit tests live here.
- `src/main.rs`: `LibraryView`, a `slint::Model` over the current results, plus
  the callbacks that connect it to the window.
- `ui/app-window.slint`: layout, visuals, and keyboard handling.

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
  order, and an empty or whitespace-only query shows all 1,000 records.
  Accent folding is not done: "shogun" finds Shōgun only because its original
  title is "Shogun".
- Selection is stored as the item's stable id, never as a list index.
- After each search, the selected id is kept if it is still in the results.
  Otherwise the first result is selected, or nothing if there are no results.
  The detail pane is rebuilt from the selected id every time, so it cannot show
  an item the filter removed.
- The filter path per keystroke:
  - lowercases the query once;
  - scans the 1,000 precomputed lowercase search keys;
  - allocates one `Vec<usize>` of matching indices;
  - resets the model. Slint then drops its current row components and
    instantiates only the ones that fit the viewport (about nine at the default
    window size), calling `row_data` for each.

  Domain records are never cloned. UI rows (`MediaRow`) are built only for
  rows the ListView shows, plus the detail row.

#### Large list

`LibraryView` implements `slint::Model` and is bound to a `for` repeater
directly inside the std `ListView`. The Slint 1.17 compiler turns that into a
virtualized repeater: it creates components only for the rows in the viewport,
drops the ones that scroll out, and creates new ones as rows scroll in. Rows
still on screen are updated in place. There is no pool that recycles
components. All 1,000 `MediaItem` records stay in Rust memory; only visible
rows exist as components and `MediaRow` values.

#### Keyboard

| Key | Effect |
| --- | --- |
| Tab / Shift+Tab | Move focus between the search field and the list (the cycle has only these two stops). |
| Up / Down / PageUp / PageDown / Home / End | In the list: move the selection and scroll it into view. |
| Down or Enter | In the search field: move focus to the list. |
| Esc | In the search field or the list: clear the search. |

Clicking a row selects it and focuses the list. A focused list shows an accent
outline on the selected row.

**Not implemented yet:** SQLite, TMDB or any network access, poster loading or
caching, sidebar navigation, and benchmarking. No performance claims are made
for this build.

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
