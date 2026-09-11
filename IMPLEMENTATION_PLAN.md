# Bingee Desktop Rust + Slint — Spike Implementation Plan

## Goal

Create a small but representative Rust + Slint Bingee Desktop prototype, then compare it fairly with the C# + Avalonia prototype.

This is a technology evaluation, not yet the production roadmap.

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
- Same asset set can be used by the Avalonia branch.

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

---

## Stop condition

After R5, pause feature development.

Compare Rust/Slint against the Avalonia spike using the agreed scorecard before starting production Bingee features such as TMDB, full library schema, watch progress, statistics, backup/restore, calendars, or notifications.
