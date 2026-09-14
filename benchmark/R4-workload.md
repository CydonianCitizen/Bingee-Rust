# R4 shared workload and measurement contract

Status: workload frozen; the full execution remains incomplete. Core automation is
`scripts/r4-core.ps1`; summarize a completed run with `scripts/r4-summarize.ps1`.
Both run from the checkout root in PowerShell 7 on Windows. Core automation uses
only Git, Rust/Cargo, PowerShell, and Windows process/system APIs. No profiler,
installation, OS tuning, application feature change, or dependency pruning is required.

## Immutable inputs

- Rust baseline: `95feb454864907a29bcf84f629278dc861e10336` (Milestone 3).
- `src/library.rs` at that commit defines all 1,000 deterministic records,
  stable IDs 1–1000, exact strings, progress values, and ordering. R1 has no
  separate milestone file; its implemented contract is the generator, its tests,
  README, and R1 section of `IMPLEMENTATION_PLAN.md`.
- SQLite file: `target/release/bingee-spike.db`, 196,608 bytes, SHA-256
  `61918DB3B71A4B0F77154ABB498268F8D7F327469355B569AAD993B58C69FEC3`.
  Keep this exact local file for Avalonia. Do not reseed or modify it for a comparison.
  The file is intentionally not added to Git. Hash equality identifies the required copy.
- All 100 committed JPEGs under `benchmark/assets/posters/`, verified against
  `SHA256SUMS`. Manifest SHA-256:
  `4BE13BC484B971585C04CA4322629CECC837DB3BB823715FE72F69C1AFD0978C`.
  Each image is 240×360 RGB, baseline JPEG, quality 85. Use the committed bytes.
- Poster index: `(local_id - 1) % 100 + 1`. Each poster serves ten records.
- Explicit LRU budget: 12,582,912 bytes, charged at width × height × 4;
  capacity at these dimensions: 36 images / 12,441,600 estimated bytes.
- Existing Slint internal image behavior, default features, renderer selection,
  synchronous SQLite and poster loading, and ListView virtualization remain intact.
- Three panes, preferred 1280×800 logical client pixels; 80-pixel rows,
  40×60 thumbnails, 160×240 detail image. Verify actual client size and DPI before
  accepting interaction samples. Do not maximize or resize during a run.
- Search: trimmed Unicode-lowercased literal substring in title/original title,
  stable ID order, no debounce. Selection survives filtering when still present,
  otherwise first result, otherwise none. Clearing retains the current matching ID.
- Fixed query sequence: `HARBOR`, `NÖRDLICHE`, `zzzz`, empty string. An empty
  query restores 1,000 records. Compare exact result IDs against the Rust oracle,
  not only result counts.

## Common controls

Use the same Windows installation, hardware, monitor, scaling, power state,
database and assets for Avalonia. Record exact versions and actual backend.
Keep Defender enabled. Do not change power settings. Close unrelated benchmark
processes and avoid concurrent builds, scans, or other deliberate heavy work.
No debugger; record any profiler separately. Run all required correctness checks
before measurements. Use only the release binary matching the recorded commit,
and preserve its SHA-256. Instrumented code needs its own immutable identity;
never label uncommitted executable changes as the original baseline binary.

Use unique run directories; retain failures. Median averages the middle pair
for even sample counts. Percentiles use nearest rank. Report sample count,
median, p95, minimum and maximum as applicable. Do not drop outliers solely for
being slow. Record why an invalid run was excluded.

## Startup and idle

Core suite: 20 separate processes, each exited before the next. OS executable,
database and asset caches are warm: these are **process-cold, OS-cache-warm
launches**, not reboot-cold launches. Timer begins immediately before
`Start-Process`, including its overhead. Endpoint: `WaitForInputIdle`, then
nonzero main-window handle, exact `Bingee Desktop` title, and `Responding`.
Poll at 10 ms. This is a readiness proxy, not confirmation that posters are
painted or an input callback has completed. Keep one second outside the timed
interval before requesting close; refresh the process/window handle first.
Require graceful process exit; forced closure invalidates that trial.

Idle: three fresh processes, five-second settle, 30 seconds untouched, sampled
once per second. CPU is the change in `TotalProcessorTime`; report seconds,
percent of one logical CPU, and percent of all logical CPUs. Report
`WorkingSet64` and `PrivateMemorySize64` in bytes. Record cursor focus/blinking
separately when observable; do not infer idle repaint behavior from CPU alone.

## Interaction runs: three fresh processes

This protocol is defined but has not been executed by the core script. A
working desktop connection is required. Start with search empty and list
focused; verify the visible list and detail state. Keys are separated by 40 ms;
settle five seconds before each checkpoint, ten seconds at E.

| Point | Exact action before sampling |
| --- | --- |
| A | Launch, initial visible posters loaded, settle five seconds |
| B | Down three times, Up three times, settle five seconds |
| C | Home, PageDown 130 times, End, PageUp 130 times, Home, End; settle |
| D | Repeat the complete C sequence three more times; sample after each pass |
| E | Home, settle ten seconds |
| F1 | Focus search, replace query with `HARBOR`, return focus to list, settle |
| F2 | Replace query with `NÖRDLICHE`, return focus to list, settle |
| F3 | Replace query with `zzzz`, verify empty state, return focus to list, settle |
| F4 | Clear query, verify 1,000 records, return focus to list, settle |

Each pass starts with Home. The R3 script ended at End and began later passes
with PageDown, so those initial steps did no work; that sequence is not reused.

At every checkpoint, record UTC, process ID, commit, working set, private bytes,
cache entries, estimated bytes, hits, misses, evictions, failures, and uncached
loads. F12 emits existing counters. Record the stderr offset before F12 and
require a newly appended complete counter line within two seconds. Never reuse
the last line from a previous checkpoint. Mark missing responses invalid.
Verify cache bytes ≤12,582,912, entries ≤36, failures=0, and a substantial
increase in misses/evictions during each traversal. Preserve all raw counters.

## Soak

After F, repeat six cycles in each process: clear search; Home; PageDown×130;
End; PageUp×130; Home; run the four queries above; Down×3, Up×3; Home.
Settle five seconds and sample all memory/cache metrics after each cycle.
Record actual accesses from hits+misses, not estimates from key count. Require
thousands of accesses and repeated evictions. Plot or tabulate every cycle.
Assess plateau versus continued private-byte growth without requiring the OS
working set to shrink. An unbounded trend requires investigation, not a pass.

## Latency measurements still required

Use the exact release application path where possible; do not relabel the R2/R3
ignored timing test executables as the release GUI binary. Those tests are
informal historical context only. If an opt-in instrumentation path is added,
identify its exact source revision and binary hash and quantify its overhead.
Do not change default workload or add debounce.

- Search: three process runs × 50 repetitions of each fixed query. Time request
  to completed model/detail property update. Separately time SQL plus owned
  Rust row construction, result replacement/model reset, and detail update where
  feasible. Deferred row realization/painting is not included unless measured.
- Selection: three process runs × 50 each: repeat selected cached row; select
  an evicted poster; select after complete traversal; select after `HARBOR`.
  Record actual hit/miss counters around each sample. Endpoint must say model
  property update or rendered frame. Never infer visual latency from setters.
- SQLite: three processes × 50 samples each of existing DB open, fetch all,
  `HARBOR`, `NÖRDLICHE`, `zzzz`. Existing open includes app schema/count
  transaction; report separately from raw connection open if both measured.
  Query duration includes row mapping unless separately instrumented. Record
  first call versus warm calls; do not flush OS caches silently.
- Posters: 100 first-path decodes per process; three processes. Cycle all 100
  images to overflow internal cache before forced-decode samples. Prime a path
  before measuring Slint internal hits (the first R3 hit sample was unprimed).
  Prime the explicit 36-entry cache before 500 explicit hits. Measure 50
  nine-new-image batches per process, with controlled cache state. Report each
  dimension/format/size range and raw sample; do not call summed medians a batch
  measurement. Shared handles mean cache memory estimates cannot simply be added.

## Motion and optional profiling

Observe release motion continuously: slow wheel, fast wheel, PageDown/PageUp,
Home/End, searches and clicks. Screenshots do not prove smoothness. Record
observer, interval, and visible hitches. Optional local video stays outside Git.

Run `SLINT_DEBUG_PERFORMANCE=refresh_lazy,console` separately for idle and
interaction diagnostics. Full-speed refresh changes workload and must never be
mixed with baseline CPU/memory. Record only metrics actually emitted. If no
reliable frame timestamps are captured, report aggregate FPS and state that
individual late frames remain unknown. See [Slint diagnostics](https://docs.slint.dev/latest/docs/slint/guide/development/debugging_techniques/).

VMMap, if already installed, can separate committed-region types and their
working sets; capture startup A and cache-saturated D on the same process/run.
Do not install it without approval. WPR being present does not establish that
heap tracing, WPA or usable symbols are available. Record tooling limitations.
See [Microsoft VMMap](https://learn.microsoft.com/en-us/sysinternals/downloads/vmmap).
Separate application heap, decoded buffers, Slint cache, renderer/driver,
SQLite, mapped images, stacks and retained allocation only where evidence
supports attribution. Keep all unproven attribution explicitly unexplained.
