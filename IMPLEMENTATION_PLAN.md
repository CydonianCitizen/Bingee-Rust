# Bingee Desktop — Implementation Plan

## Goal

Bingee Desktop is a Rust + Slint local-first desktop media tracker for
Windows, macOS and Linux.

Milestones R0–R5 were a technology evaluation: a small Rust + Slint prototype
compared with a C# + Avalonia prototype (separate repository). That phase is
closed. Its sections below are kept as they were written, and its evidence
stays in `docs/measurements/` and `benchmark/`. From R6 on, this repository is
the canonical Bingee Desktop implementation and this plan is its production
roadmap. The R4 workload remains available as a benchmark fixture for
performance regression checks.

This file holds deliverables and exit criteria. The working brief for a
milestone lives in `docs/milestones/<id>.md`; start one with `/milestone R0`.

---

## Milestone R0 — Bootstrap + visible desktop shell

### Goal

Produce the smallest cross-platform Rust + Slint application that already looks recognizably like Bingee Desktop.

### Deliverables

- Rust stable application.
- Slint integrated using the recommended Rust build workflow.
- Window launches successfully.
- Desktop-first three-pane shell:
  - left navigation/sidebar;
  - center library/content pane;
  - right details pane.
- Small deterministic fake media model in Rust.
- Selecting a fake title updates the detail pane if practical without unnecessary complexity.
- No network.
- No SQLite.
- No remote posters.
- No production caching.
- No OS-specific behavior unless strictly needed to launch.
- Basic README run instructions.

### Exit criteria

- `cargo run` opens Bingee Desktop.
- Build/check/tests pass.
- The app is visibly desktop-first rather than a mobile layout stretched horizontally.
- No unnecessary dependencies.

---

## Milestone R1 — Representative fake-data interaction

### Goal

Reach the same interaction surface that will later be reproduced in Avalonia.

### Deliverables

- 1,000 deterministic fake media records generated locally.
- Search/filter interaction.
- Selection updates details.
- Library presentation remains responsive.
- Keyboard focus/navigation behavior is considered.
- UI does not eagerly allocate expensive per-item resources.

### Exit criteria

- Search and selection work with 1,000 items.
- No network or database is needed.
- Initial memory behavior can be measured reproducibly.

---

## Milestone R2 — SQLite vertical slice

### Goal

Test the real local-first data path without building the full Bingee schema.

### Deliverables

- SQLite integration with a deliberately small schema.
- Seed/import of the deterministic benchmark dataset.
- One realistic library query with sorting/filtering.
- Database work kept off the UI hot path where necessary.
- No ORM added unless justified by measured development value.

### Exit criteria

- App can show/query library data from SQLite.
- Query behavior is deterministic and benchmarkable.
- UI remains responsive.

---

## Milestone R3 — Poster pipeline and bounded cache

### Goal

Test the resource-intensive part most likely to dominate a media application.

### Deliverables

- Fixed local benchmark poster assets first.
- Lazy decoding/loading.
- Explicit bounded in-memory cache.
- Cache behavior documented.
- No remote TMDB dependency required for benchmark validity.

### Exit criteria

- Scrolling does not decode the whole poster set.
- Memory does not grow without bound during repeated navigation.
- Same asset set can be used by the Avalonia spike.

---

## Milestone R4 — Benchmark instrumentation

### Goal

Collect comparable numbers, not impressions.

### Deliverables

Measurements defined by `BENCHMARK_SPEC.md` for:

- cold startup;
- idle memory;
- memory after library interaction;
- CPU while idle;
- search/filter latency;
- selection/detail latency;
- SQLite query latency;
- package/binary footprint.

### Exit criteria

- Every result states OS, architecture, build profile, commit SHA, dataset, procedure, and sample count.
- Results are reproducible.
- No Rust-vs-C# conclusion is made from mismatched workloads.

### Status

Partial / environment-blocked (13 September 2026). The non-interactive
baseline is frozen. The interactive memory checkpoints, soak, and motion
verification remain required. See `docs/measurements/R4-rust-slint/STATUS.md`.

---

## Milestone R5 — Packaging and cross-platform sanity

### Goal

Ensure the architecture is genuinely desktop cross-platform rather than accidentally Windows-only.

### Deliverables

- documented build/run procedure;
- Windows package/build sanity;
- macOS/Linux assumptions documented and CI/build checks added when practical;
- platform-specific code isolated.

### Exit criteria

- No core design depends on a Windows-only API.
- Remaining cross-platform risks are explicit.

### Status

PASS locally / cross-platform CI pending (13 September 2026). Windows is
built, tested, packaged and launch-checked. The CI workflow for macOS and
Linux has not run yet. See `docs/milestones/R5.md`.

---

## Stop condition (R0–R5, historical)

After R5, pause feature development.

Compare Rust/Slint against the Avalonia spike using the agreed scorecard before starting production Bingee features such as TMDB, full library schema, watch progress, statistics, backup/restore, calendars, or notifications.

The pause ended in September 2026: Rust + Slint was chosen, and R6 started
the production application.

---

## Milestone R6 — Production foundations

### Goal

Turn the benchmark-oriented spike into a clean production foundation: real
user-data locations, a versioned schema with migrations, a coherent error
model, local diagnostics, correct About/licensing, and an app that starts with
an empty real library. No TMDB.

### Deliverables

- Normal startup without fake data; the R4 fixture kept as an explicit,
  opt-in benchmark build.
- Per-user data, cache and log locations on Windows, macOS and Linux, behind
  one testable path policy.
- Schema v1 (`media`, `external_refs`, `library_entries`) with `PRAGMA
  user_version` migrations that run in one transaction.
- Identity `(source, media_type, external_id)` enforced by SQLite.
- An application error model and a startup error page; a failed open never
  deletes or recreates the database.
- A local log file (no telemetry) for startup, migrations and failures.
- Navigable shell (Home, Library, Discover, Calendar, Statistics, Settings,
  About); Library, Settings and About functional, the rest placeholders.
- About with version, Slint attribution, license status and data locations.
- A settings module for future preferences.
- Tests for migrations, constraints, corrupt/foreign/newer databases, and path
  resolution. ADR-0006, `docs/milestones/R6.md`, updated README.

### Exit criteria

- Production startup contains no fake library data.
- Correct cross-platform user-data paths are used.
- Schema v1 is explicit and migrated transactionally.
- Movie/TV external-id collision is impossible by schema.
- Startup database failure is not silently destructive.
- Benchmark fixtures remain reproducible and separate.
- About/attribution is accessible.
- Build, tests and clippy pass.
- Documentation matches behavior.

### Status

PASS (14 September 2026): committed as `2e4f01d`; the `cross-platform` CI run
passed on Windows, Ubuntu and macOS. See `docs/milestones/R6.md`.

---

## Milestone R7 — TMDB integration and remote search

### Goal

Search TMDB for movies and TV series from Discover, with the user's own TMDB
credential kept securely, without ever blocking the UI. Results stay
transient: no Add to Library, no poster cache, no details, no tracking.

### Deliverables

- TMDB API Read Access Token: validate, save, replace, remove; kept only in
  the OS credential store; never logged or shown in full.
- A TMDB client with isolated DTOs mapped to a provider-independent
  `MediaSearchResult`; identity `(source, media_type, external_id)`.
- Network I/O off the UI thread, with finite timeouts and differentiated
  errors (invalid credential, rate limit, timeout, offline, server,
  malformed, unexpected).
- Discover: debounced search over movies and TV, stale-response protection,
  paging with Load more, and a clear state for every case.
- Mock-HTTP tests; no live TMDB needed. ADR-0007, ADR-0008, ADR-0009,
  `docs/milestones/R7.md`, README.

### Exit criteria

- The token is securely stored and never logged or committed.
- Remote validation, Movie search and TV search work.
- Movie/TV id collisions remain impossible.
- Network runs off the UI thread; debounce works; stale results cannot
  overwrite newer ones; paging is correct; failures are differentiated.
- Local data stays usable without a network.
- Automated tests need no live TMDB, and all checks pass.
- Search results remain transient.

### Status

PASS (14 September 2026): committed as `847a257`; the `cross-platform` CI
run passed on Windows, Ubuntu and macOS. Live TMDB smoke test not run (no
credential available). See `docs/milestones/R7.md`.

---

## Milestone R8 — Real local-first library

### Goal

Turn transient TMDB results into persistent library data: search, Add to
Library, restart, and use the library offline with locally cached posters.
No watch progress, seasons, episodes, ratings or detail endpoints.

### Deliverables

- Add to Library as one SQLite transaction, idempotent by the full identity
  `(source, media_type, external_id)`; Remove from Library that removes
  membership only.
- Library read from SQLite alone: local search, Movie/TV filter, sort by
  recently added or title; empty, no-match and error states.
- Discover shows "In Library" per result, from one query per result set.
- Production posters: TMDB image configuration, `w185`, an atomic disk cache
  in the cache folder, deduplicated background downloads, a bounded decoded
  RAM cache, key-addressed binding so a late poster never lands on another
  row, and nonfatal failures.
- Keyboard use of Discover (list navigation, Enter adds) and the Library
  controls.
- ADR-0010, ADR-0011, ADR-0012, `docs/milestones/R8.md`, README.

### Exit criteria

Search TMDB → add a movie and a series → restart → they are there, posters
cached where downloaded → TMDB unreachable → restart → the library is fully
usable. Also: idempotent add, no Movie/TV id collision, batched membership,
transactional add, non-destructive remove, SQLite-backed search/filter/sort,
persistent disk cache, bounded RAM cache, no wrong-row posters, nonfatal
poster failures, no local-data damage from network failures, all checks pass.

### Status

PASS locally / cross-platform CI pending (14 September 2026). See
`docs/milestones/R8.md`.

---

## Next: R9 — Details and refresh

Movie and TV detail endpoints, a metadata refresh policy, and the first
tracking data. See the R9 prerequisites in `docs/milestones/R8.md`.
