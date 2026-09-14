# R4 Rust + Slint — partial baseline, not milestone completion

> **Status, 13 September 2026:** R4 is frozen as **PARTIAL /
> ENVIRONMENT-BLOCKED**. See [STATUS.md](STATUS.md). The report below is
> preserved as written on 11 September 2026, including the branch name and
> repository state of that time. Development has since moved to `main`.

The unchanged R3 release executable retains its valid startup-proxy and idle
resource baseline. R4 continuation adds separate, opt-in release latency
evidence. **R4 is incomplete:** the frozen A–F interaction checkpoints,
six-cycle soak in three processes, and continuous-motion verification remain
blocked by the unavailable desktop connection. These are required, not waived.

Source commit: `95feb454864907a29bcf84f629278dc861e10336`, branch
`spike/rust-slint`. The initial core run had no R4 application instrumentation,
optimization, dependency, feature, commit or push. The user committed R3 while baseline checks
were underway; the agent created the requested branch. At measurement time,
all tracked release inputs and poster assets matched HEAD. New R4 scripts,
protocol and reports remain uncommitted.

Environment: [environment.md](environment.md). Exact shared workload and pending
procedures: [benchmark/R4-workload.md](../../../benchmark/R4-workload.md).

## Correctness gate

Every required command passed before the valid measurement run. Complete logs
and exit codes are in `raw/20260911T192701561Z/gate.json` and `gate-0.txt` through
`gate-6.txt`:

| Command | Outcome |
| --- | --- |
| `rustc --version` | 1.98.1; exit 0 |
| `cargo --version` | 1.98.1; exit 0 |
| `cargo fmt --check` | exit 0 |
| `cargo check` | exit 0 |
| `cargo test` | 30 passed, 0 failed, 2 ignored; exit 0 |
| `cargo clippy --all-targets --all-features -- -D warnings` | exit 0, no Clippy warnings |
| `cargo build --release` | exit 0 |

The two ignored tests are the existing informal R2/R3 timing tests; they were not
run or republished as formal R4 timings. Missing/corrupt-image error text in the
correctness log is expected from the fallback test. Initial sandbox toolchain
calls emitted `warn: could not canonicalize path C:\Users\thoma`; elevated
recorded gate logs did not. No product source changed after the gate.

Final verification also passed all five requested Cargo checks; logs are in
`raw/final-verification/`. The independent review reproduced the statistics and
found one reusable-script issue: an arbitrary `-Exe` could select a debug or
stale artifact. The current runner restricts it to the newly built release path
and rejects target-directory/target environment overrides. The recorded valid
run already used that exact default release artifact; results are unaffected.
Its original runner and hash are retained beside raw results.

### Continuation correctness and executable identity

The five requested commands were rerun before new measurements, all exit 0:
`cargo fmt --check`, `cargo check`, `cargo test`,
`cargo clippy --all-targets --all-features -- -D warnings`,
`cargo build --release`. Tests: 30 passed, 0 failed, 2 historical timing tests
ignored. Logs: `raw/continuation-baseline-gate/`. The original release SHA-256
was again `AEAF7F294E5D829CF2F693E768F79BC8AF6A1BB0416954296965F78218B9F35E`.
Its exact bytes remain in `target/release/bingee-r4-frozen.exe` (not committed).

The opt-in harness is documented in [ADR-0004](../../adr/0004-r4-opt-in-measurement.md).
It adds only an application Cargo feature `r4-measurement`, a gated entry
point/module, and feature access to the existing test oracle. It does not
change any Slint feature. The original `connect`, `set_query`, `show_selection`,
`select_row`, SQLite implementation, poster implementation, UI markup, schema,
assets, dimensions, and cache budget remain unchanged. The ordinary entry
point is unchanged when the measurement feature is disabled. No optimization
or dependency was added; `Cargo.lock` is unchanged.

All five requested commands passed again after the harness changes, including
Clippy with all features. Additional `cargo test --features r4-measurement`
passed 30 tests with 2 ignored; `cargo build --release --features r4-measurement`
passed. Logs: `raw/continuation-instrumented-gate/`. One preliminary feature
check failed with E0425/E0596 and was corrected before the passing gate; see
`development-errors.txt`. No measurements were taken behind a failing gate.

| Artifact | SHA-256 |
| --- | --- |
| Frozen original, startup/idle and future keyboard memory protocol | `AEAF7F294E5D829CF2F693E768F79BC8AF6A1BB0416954296965F78218B9F35E` |
| Default release rebuilt after gated source additions | `DB4A35DE850C353C97E05FCBC30BD55AB604D7CA70C6673D2646CB61E624BCB7` |
| Opt-in release latency executable | `DB1CFDEB8A47CCD4AB28B89A4ACC59DF15DC63FE1300E751F71CDB39D6DE67E3` |
| Final ordinary release executable, measurement feature disabled | `4A12F3863BE136A1F78C62C3670EB3278E1C879C64CA26D65835ACC34DD90D1F` |

The latency binary is a new build containing the opt-in harness, timers, and
assertions, not the original frozen artifact. It is 17,000,960 bytes; the
73,728-byte instrumentation increase is not a product footprint measurement.
The final default build is 16,927,232 bytes, the same size as the frozen binary.
Binary inspection proves its `.text`, `.data`, `.pdata`, and `.reloc` sections
are byte-identical to the frozen binary. Only 35 bytes differ: seven Rust
`src/main.rs` source-line locations shifted by the 12 inserted lines, COFF/debug
timestamps, and the CodeView PDB GUID. Exact offsets and attribution are in
`raw/continuation-instrumented-gate/pe-identity.json`,
`default-byte-differences.json`, and `default-difference-attribution.json`.
The changed default hash therefore does not indicate changed application
machine code. Source equivalence for the opt-in workload is additionally
established by the retained patch and unchanged operation bodies. Every search
sample compares complete returned records against the R1 oracle, checks the
selection policy, and checks detail/poster identity after callback completion.
SQLite samples compare complete records too. Timer overhead is measured
separately; compiler layout effects and harness scheduling overhead are not
quantified. These limitations must accompany comparison numbers.

All 70 pre-existing raw files were read and hashed. Their preservation manifest
is `raw/continuation-baseline-gate/preserved-inputs.json`. Valid startup, idle,
footprint, dependency, environment, and idle-render measurements were neither
discarded nor repeated.

## Measured facts: startup and idle

Twenty fresh processes, true exit between samples, warm OS file caches. Timer
includes Start-Process overhead. Endpoint is input-idle plus the exact responsive
`Bingee Desktop` main window, polled every 10 ms. This is **not paint completion
or first proven input handling**. No reboot or file-cache flushing occurred.

| Startup proxy | Milliseconds |
| --- | ---: |
| n | 20 |
| Median | 654.5962 |
| p95, nearest rank | 672.2140 |
| Minimum | 635.9026 |
| Maximum | 712.7205 |

Three fresh idle processes, five-second settle, at least 30 seconds untouched:

| Run | Interval s | CPU seconds | One-core CPU % | Whole-machine CPU % | Final WS MiB | Final private MiB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 30.3954 | 0.062500 | 0.20562 | 0.01714 | 125.4102 | 166.8906 |
| 2 | 30.3601 | 0.015625 | 0.05147 | 0.00429 | 125.3281 | 167.5820 |
| 3 | 30.3750 | 0.031250 | 0.10288 | 0.00857 | 125.6250 | 167.9844 |

CPU uses cumulative process CPU time differences, not Task Manager sampling.
Whole-machine percentages divide by 12 logical CPUs. Memory is Windows
`WorkingSet64` and `PrivateMemorySize64`. Private bytes decreased by 40,960
bytes within each idle interval. This is not a navigation soak result.

Inference: launch looks plausibly quick from the proxy, and idle CPU is
effectively negligible. “Feels immediate” still needs direct observation.

## Formal memory checkpoints and soak

| Requirement | Status |
| --- | --- |
| A startup idle | Process memory measured above; fresh F12 cache counters and initial painted state not captured, so A is incomplete |
| B first screen | Not measured |
| C long navigation | Not measured |
| D repeated churn | Not measured |
| E return to top | Not measured |
| F fixed searches | Not measured |
| Six-cycle soak × three processes | Not measured |

Do not substitute the R3 informal ~194 MiB plateau for missing R4 checkpoints.
The passing cache tests establish the explicit bound in tested paths, but do not
establish process-memory stability or leak absence during the requested soak.

Continuation attempted Computer Use initialization, retry, and reset/retry.
All three returned the same unavailable native-pipe error, retained in
`raw/continuation-baseline-gate/desktop.json`. No frozen interaction run was
started. The programmatic navigation used to prepare selection timing is not
substituted for the exact keyboard protocol, fresh F12 checkpoints, or soak.
No A–F counter/memory CSV exists because those measurements were unavailable.

| Soak cycle | Process 1 private bytes / cache size | Process 2 | Process 3 |
| --- | --- | --- | --- |
| 1 | Not collected | Not collected | Not collected |
| 2 | Not collected | Not collected | Not collected |
| 3 | Not collected | Not collected | Not collected |
| 4 | Not collected | Not collected | Not collected |
| 5 | Not collected | Not collected | Not collected |
| 6 | Not collected | Not collected | Not collected |

**Stability/leak conclusion: unknown under the required repeated-use workload.**
Idle evidence and bounded cache tests cannot establish a process-private plateau.
There is no basis yet for the requested “no evidence of an unbounded leak under
this workload” conclusion. VMMap remains optional; the missing soak is blocking.

## Memory breakdown: evidence and limits

The historical R2/R3 private-memory delta of approximately 30–32 MiB remains
**unattributed by category**. No VMMap snapshots or heap/virtual-allocation trace
was collected. VMMap was not located in checked paths; deeper VMMap analysis
requires an available installation, or installation approval. WPR is present,
but WPA/heap analysis was not established. Nothing was installed or configured.

| Category | Evidence | Attribution limit |
| --- | --- | --- |
| Explicit decoded image cache | Source charges 36×240×360×4 = 12,441,600 bytes at capacity | Budget accounting, not process-private attribution |
| RGB pixel payload | 36×240×360×3 = 9,331,200 bytes (8.899 MiB) | Logical maximum for these cache entries; allocation overhead and other holders separate |
| Slint internal cache | Resolved Slint 1.17.1 `graphics/image/cache.rs:51` sets 5 MiB, weighted by actual buffer bytes | Shared image handles overlap the explicit cache; adding both budgets double-counts shared buffers |
| Executable/DLL mapping | Per-idle-process module names and ModuleMemorySize recorded | Image virtual sizes are not private committed bytes or resident sizes |
| Renderer/driver | FemtoVG/OpenGL diagnostic; NVIDIA OpenGL DLL loaded, image span 48,934,912 bytes | Cannot call that span additional private RAM, nor assign the R2/R3 delta to GPU memory |
| Rust heap, SQLite, decoder temporaries, retained allocator pages, stacks, other private regions | No category-level profiler data | Unknown |

The 12 MiB budget is intentionally RGBA-sized; JPEG RGB payload is smaller.
Slint retains reference-counted buffers, so explicit eviction need not free a
buffer still used elsewhere. These source facts explain why a cache budget
alone cannot explain process memory. They do **not** measure the remaining
delta. No subtraction of logical cache bytes from process-private deltas is
presented as proven allocation attribution.

## Search, selection, SQLite and poster latency

New R4 samples: `raw/latency-20260911T201032397Z/`, 20:10:32–20:12:46 UTC on
11 September 2026. Nine separate release processes: three SQLite, three poster,
three GUI. All exited successfully; no samples excluded. Environment matches
the frozen Windows/CPU/toolchain, AC online, Balanced plan, Defender enabled.
No profiler/debugger was attached by the runner, no cache flush, no concurrent
build or deliberate heavy workload. New environment and immutable input hashes
are retained in that directory. All three GUI runs report 1280×800 physical and
logical client pixels, Slint scale factor 1. This is application-reported
effective scaling, not an independently queried monitor DPI.

Build: `cargo build --release --features r4-measurement`. Run:
`benchmark/scripts/r4-latency.ps1`; summarize with
`benchmark/scripts/r4-latency-summary.ps1 -RunDirectory docs/measurements/R4-rust-slint/raw/latency-20260911T201032397Z`.
The runner uses the separately retained `target/release/bingee-r4-latency.exe`.
Its source snapshot, source hashes, patch, executable hash and process records
are colocated with samples. The normal release executable was rebuilt without
the feature after measurements, exit 0; the measured binary remains retained.

All tables below use **milliseconds**, pooled across three processes, median
averaging the middle pair and p95 by nearest rank. Raw nanoseconds and minima
are preserved. No historical R2/R3 timing is relabelled as R4.

### Search: request to completed callback/property update

Three GUI processes × 50 repetitions of each case. Each repetition restores
with an empty request, then runs the fixed `HARBOR`, `NÖRDLICHE`, `zzzz`, empty
sequence. Callbacks run on the actual GUI event loop at nominal 40 ms intervals,
after a five-second startup wait. The timer starts immediately before
`invoke_query_changed` and ends on return. It includes synchronous SQL/owned
record construction, selection preservation, model reset, detail property
updates and reveal-row invocation. It excludes text input delivery, query-string
construction, deferred row realization, layout and painting. The LineEdit
typing path is not simulated. **This is not painted-frame latency.**

| Query | Result rows | n | Median ms | p95 ms | Max ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| Restore / empty | 1,000 | 150 | 1.611050 | 2.377700 | 4.304100 |
| `HARBOR` | 25 | 150 | 0.728350 | 1.256300 | 2.202300 |
| `NÖRDLICHE` | 39 | 150 | 0.634300 | 1.105600 | 1.873500 |
| `zzzz` | 0 | 150 | 0.481550 | 0.760800 | 1.063400 |
| Clear | 1,000 | 150 | 1.293300 | 1.750600 | 2.391400 |

SQL/owned-row construction is measured separately below. Rust model replacement
and Slint property setters are not independently timed inside these callbacks;
the original operation bodies were left intact. Do not subtract independent
SQL medians from these medians to claim a measured model-reset cost. The measured
update path is effectively instantaneous at this scale; visible response still
requires observation.

### Selection: callback entry to completed detail property update

Three GUI processes × 50 samples per case. Timing wraps the original
`invoke_row_selected` callback, including Rust selection, row lookup and detail
construction, poster lookup/load, and Slint setters. Reveal-row for subsequent
painting occurs outside the interval. Rendered-frame completion is not measured.

| Case | n | Median ms | p95 ms | Max ms | Measured cache hits / misses |
| --- | ---: | ---: | ---: | ---: | ---: |
| Repeat selected cached row | 150 | 0.009600 | 0.015000 | 0.040900 | 150 / 0 |
| Explicit poster-cache miss | 150 | 0.860800 | 0.931900 | 1.255100 | 0 / 150 |
| After long navigation | 150 | 0.045700 | 0.153100 | 1.298100 | 147 / 3 |
| After `HARBOR` filtering | 150 | 0.040450 | 0.120300 | 0.181600 | 150 / 0 |

For a controlled miss, paths 2–100 churn both caches immediately before selecting
row 0; setup is outside the timer. For the navigation cohort, one complete
programmatic traversal precedes 50 selections of rows 950–999 per process:
row 0, 130 eight-row forward moves clamped at 999, row 999, 130 reverse moves,
row 0, row 999. These call the real row-selected/reveal-row functions with
event-loop turns between moves. They are preparation for latency, not evidence
that PageDown/PageUp input works. After filtering, selections cycle through
the 25 `HARBOR` rows twice. Counters surround each timed callback; selection,
detail strings/progress, and mapped poster path are checked after each sample.

### SQLite: existing frozen database, query plus owned-row mapping

Three fresh processes × 50 calls per case. Existing open includes
`Connection::open` plus the app's schema/count transaction; it does not seed.
Raw connection-only open was not measured. Each query calls the unchanged
`db::search`, including parameter lowercasing, SQL stepping and owned
`MediaItem` construction. SQL execution and row mapping are not separated.

| Case | n | Median ms | p95 ms | Max ms |
| --- | ---: | ---: | ---: | ---: |
| Existing database open | 150 | 0.686050 | 0.851200 | 1.569000 |
| Fetch all 1,000 | 150 | 1.225300 | 1.348600 | 1.565700 |
| `HARBOR`, 25 rows | 150 | 0.216450 | 0.398400 | 0.419000 |
| `NÖRDLICHE`, 39 rows | 150 | 0.243650 | 0.462700 | 0.497000 |
| `zzzz`, no rows | 150 | 0.181850 | 0.333600 | 0.352500 |

OS caches are warm. Each of the 50 existing-open samples uses a new connection.
Queries then use another connection whose page cache was warmed by the open
count transaction. Its first fetch-all prepares the cached statement; subsequent
queries reuse that statement. First samples are included in the statistics and
listed separately in `sqlite-first-calls.csv` (15 rows). First existing-open
durations across processes: 1.569000, 1.034200, 1.004000 ms; first fetch-all:
1.323000, 1.105400, 1.248200 ms. No reboot-cold SQLite result is claimed.
These results provide no reason to add async/threaded SQLite for 1,000 records.

### Posters: unchanged local JPEG set and cache budget

100 RGB JPEGs, 240×360, quality 85, 20,496–23,500 bytes; no asset changes.
Three fresh poster processes, with no preceding application image load.

| Case | n | Median ms | p95 ms | Max ms |
| --- | ---: | ---: | ---: | ---: |
| First path load / local decode | 300 | 1.050650 | 1.225100 | 1.596800 |
| Forced re-decode | 1,500 | 0.917000 | 1.197000 | 1.764800 |
| Primed Slint internal-cache hit | 1,500 | 0.036900 | 0.054300 | 0.107400 |
| Bingee explicit-cache hit | 1,500 | 0.000100 | 0.000200 | 0.001000 |
| Nine new images, actual timed batch | 150 | 7.429050 | 8.197500 | 11.029100 |

First-path samples cover each of the 100 paths once per process. A complete
100-image sweep precedes 500 cyclic forced-decode samples. The approximately
25 MiB RGB pool exceeds Slint's 5 MiB path LRU. The internal-hit path is explicitly
primed before all 500 timed hits. A full 36-entry Bingee cache is primed before
500 oldest-entry hits. Each nine-image batch follows a full 100-path cache
sweep ending at path 100, then loads paths 1–9 while retaining the nine handles;
setup is excluded. The batch includes actual `PosterCache::get` work and the
small handle-vector allocation, not layout/GPU upload/painting. Every batch
recorded nine explicit misses and nine evictions, with no failed loads.

Slint hit/decode classification is inferred from controlled path history and
the resolved 1.17.1 LRU implementation, not internal Slint counters. Explicit
hit/miss counts come from Bingee's actual counters. Low-level first/forced-load
CSV miss fields mark the intended decode class, not Bingee cache activity.
OS file caches remain warm: “decode miss” does not mean a physical disk read.

The largest measured batch is 11.029100 ms before rendering. This does not by
itself establish a 16.7 ms painted-frame budget. It provides no latency-only
reason to add background decoding, but motion/frame evidence remains necessary.

### Timer overhead and precision

9,000 empty timer-pair samples: median 0 ns, p95 100 ns, max 100 ns. Durations
show 100 ns quantization. Values are not overhead-subtracted. The 100 ns median
explicit-cache hit is near the timer floor, so it should be read as sub-microsecond
work, not a precise 100 ns cost. Assertion/CSV writes occur after timing; their
between-sample effects and the feature build's code layout are not subtracted.

## Render behavior and continuous visual verification

Separate diagnostic run:
`raw/20260911T192701561Z/lazy-20260911T193115911Z/`.
Slint reported `release; Backend: FemtoVG renderer with OpenGL backend`.
First one-second aggregate: 4 FPS. All subsequent 34 aggregates: 0 FPS. The
recorded idle interval follows a five-second settling period and lasts 30
seconds. This supports no continuous idle repaint in that run.

Focus and cursor blinking were not visually observed; there is no basis for
claiming a focused input caret behaved a particular way. Diagnostics were
separate from baseline memory/CPU. Full-speed refresh was not enabled.

Wheel, fast wheel, PageDown/PageUp, Home/End, search and selection motion were
**not observed in R4**. No screen recording or per-frame time samples exist.
No frame median/p95/p99 or scrolling FPS is invented. R3's historical smoothness
observation and 58–61 FPS aggregates are not new R4 verification.

The continuation's programmatic callback runs do not change that conclusion.
No continuous video, human motion observation, new aggregate scrolling FPS,
or reliable per-frame timestamps were captured. Slow/fast wheel, PageDown/PageUp,
End/Home, search typing, mouse row selection, keyboard row selection, poster
pop-in, input lag, virtualization poster correctness and visible detail matching
remain unobserved in R4. Callback correctness assertions establish model state,
not visible presentation. This acceptance item remains incomplete.

## Disk footprint and dependencies

| Component | Bytes |
| --- | ---: |
| Release executable | 16,927,232 |
| Existing SQLite DB | 196,608 |
| 100 benchmark JPEGs | 2,188,318 |
| Required application payload sum | 19,312,158 |
| Executable delta from recorded R2 | +93,696 |
| Executable delta from recorded R3 | 0 |

JPEGs range from 20,496 to 23,500 bytes. The payload sum excludes OS DLLs,
debug PDB, toolchain/build output, source, documentation, and measurement logs.
It is **not a tested installer or relocatable package**: poster paths still
refer to the build checkout. Installed footprint is not established until R5.
Disk bytes and memory bytes are distinct metrics.

Dependency inventory: two runtime dependencies, one build dependency, one dev
dependency; 330 Windows-target resolved package identities including the app.
Exact versions/features and raw tree locations are in [environment.md](environment.md).
No features were pruned.

## Anomalies and raw records

- `raw/20260911T192512956Z/`: invalid initial trial. Startup proxy 725.0611 ms;
  graceful close timed out, so the process was killed and the sample marked
  invalid. The early endpoint lacked the exact-title check and the cached
  main-window handle was not refreshed before close. A startup helper window
  is a plausible explanation, not proven. No idle samples were collected.
  A read-only tool-location scan overlapped this already-invalid attempt.
- The valid runner adds the exact title check, refreshes before close and waits
  one second outside the startup timer. All 20 valid samples closed gracefully.
  No slow sample from the valid run was excluded.
- `raw/20260911T192701561Z/`: complete core run, environment, gate logs, startup
  CSV, 90 one-second idle samples, three idle summaries, modules, dependency
  trees, completion and statistics JSON, and separate lazy diagnostics.
- Desktop automation failed three times including reset; no fabricated UI data.
- R3 review found stale-counter acceptance and truncated later traversals in its
  script, an unprimed first Slint-hit timing, and a calculated batch estimate
  described too strongly. R3 was preserved; the R4 contract corrects the methods.
- `raw/continuation-baseline-gate/`: fresh passing baseline gate, original
  executable identity, preservation manifest for all 70 prior files, and three
  failed desktop-connection attempts. Zero A–F/soak runs started; these are
  unavailable measurements, not silently discarded trials.
- `raw/continuation-instrumented-gate/`: passing default and feature checks,
  preliminary development-check errors, all three build identities, and binary
  difference attribution. The development compile failure is not a latency run.
- `raw/latency-20260911T201032397Z/`: **9 valid process runs, 0 invalid/excluded**.
  All raw timing samples, including first calls and maxima, remain included.
  `runs.json` records PID, UTC boundaries, exit status and validity.
  `verification.json` confirms source/executable/database identity was unchanged
  throughout measurement. No stderr errors occurred in any of the nine runs.
  Independent JavaScript recomputation matches all 20 pooled groups and confirms
  preservation of all 70 prior raw files and all measured source hashes. See
  `independent-verification.json`: 7,050 workload samples plus 9,000 timer-pair
  samples, 16,050 CSV rows in total.

### Exact new raw-result locations

All paths below are under `docs/measurements/R4-rust-slint/`:

| Evidence | Location |
| --- | --- |
| Original core evidence, preserved | `raw/20260911T192701561Z/` |
| Original invalid startup, preserved | `raw/20260911T192512956Z/` |
| Original final gate, preserved | `raw/final-verification/` |
| Continuation baseline gate and original hash | `raw/continuation-baseline-gate/gate.json`, `executable.json` |
| Desktop failure and preservation record | `raw/continuation-baseline-gate/desktop.json`, `preserved-inputs.json` |
| Post-instrumentation gates | `raw/continuation-instrumented-gate/gate.json`, `feature-gate.json`, `default-final-build.json` and accompanying `.txt` logs |
| Instrumented source/executable/environment | `raw/latency-20260911T201032397Z/environment.json`, `instrumentation.patch`, `r4_latency.rs`, `runner.ps1` |
| SQL raw samples | `raw/latency-20260911T201032397Z/sqlite-1.csv`, `sqlite-2.csv`, `sqlite-3.csv` |
| Poster/cache raw samples | `raw/latency-20260911T201032397Z/posters-1.csv`, `posters-2.csv`, `posters-3.csv` |
| Search/selection raw samples | `raw/latency-20260911T201032397Z/ui-1.csv`, `ui-2.csv`, `ui-3.csv` |
| GUI client geometry/scaling | `raw/latency-20260911T201032397Z/ui-1.geometry.txt`, `ui-2.geometry.txt`, `ui-3.geometry.txt` |
| Process status and final identity check | `raw/latency-20260911T201032397Z/runs.json`, `verification.json`; per-process `.complete` and `-stderr.txt` files |
| Pooled statistics, minima and cache deltas | `raw/latency-20260911T201032397Z/latency-statistics.json`, `latency-table.md` |
| First SQLite calls, also included in statistics | `raw/latency-20260911T201032397Z/sqlite-first-calls.csv` |
| A–F / soak / continuous-motion raw results | **Absent: desktop connection unavailable** |

## Qualitative answers and potential optimization experiments

| Question | R4 answer |
| --- | --- |
| 1. Is idle CPU effectively negligible? | Yes: 0.00429–0.01714% of this 12-logical-CPU machine in three 30-second intervals |
| 2. Does private memory stabilize under repeated use? | Unknown: the required soak has not run |
| 3. Does the explicit poster cache stay bounded? | Bound is enforced by unchanged code and passing tests; fresh formal A–F/soak counters remain missing |
| 4. Is there evidence of a memory leak under the tested workload? | Required repeated-use workload is unmeasured; neither a leak nor its absence can be concluded from idle sampling |
| 5. Is local search effectively instantaneous? | Yes for the measured callback/model/property update: worst 4.304100 ms; painted response unverified |
| 6. Does SQLite need async/threaded execution? | No evidence that it does at 1,000 records: fetch-all max 1.565700 ms; retain synchronous design |
| 7. Does poster decoding need background execution? | Timings alone do not justify it: nine-image batch max 11.029100 ms; rendering and motion remain unverified |
| 8. Is release scrolling visually smooth? | Unknown: no continuous R4 visual observation |
| 9. Is startup fast enough? | Process-ready proxy median 654.5962 ms is plausibly fast enough for desktop startup; first paint/input and subjective feel unverified |
| 10. Is this a credible lightweight baseline? | Promising measured candidate, but not yet accepted: private-memory stability and continuous interaction evidence are required |

Potential optimization experiments, **not implemented**: investigate unique
decoded-buffer retention across both caches; profile renderer/driver versus
application allocations; consider worker decoding only if measured batch/frame
latencies justify it; evaluate optional Slint features later; evaluate release
link/strip settings later. Any such experiment needs a separate result and must
not replace this unchanged baseline. “Acceptable baseline” is a future judgment
after missing evidence; “already minimal/optimized” is not supported.

## Avalonia comparison values and remaining acceptance

Use the exact search, selection, SQLite and poster tables above, with their
sample counts, warm-cache assumptions and callback/decode endpoints. Do not
compare them against painted-frame or OS-input-to-frame timings. Preserve the
separate frozen and instrumented executable identities in any comparison.

Additional frozen Rust values Avalonia must match in procedure:

| Metric | Exact Rust baseline |
| --- | --- |
| Process-cold / OS-warm startup, n=20 | Median 654.5962 ms; p95 672.2140 ms; max 712.7205 ms |
| Idle run 1, 30.3954454 s | CPU 0.0625 s; WS 131,502,080 bytes; private 174,997,504 bytes |
| Idle run 2, 30.3600891 s | CPU 0.015625 s; WS 131,416,064 bytes; private 175,722,496 bytes |
| Idle run 3, 30.3749885 s | CPU 0.03125 s; WS 131,727,360 bytes; private 176,144,384 bytes |
| Explicit cache configuration | Budget 12,582,912 estimated bytes; capacity 36 images / 12,441,600 estimated bytes; formal saturation counters pending |
| Dataset/results | 1,000 total; `HARBOR` 25; `NÖRDLICHE` 39; `zzzz` 0; compare exact ordered records/IDs, not counts alone |
| Client size for latency | 1280×800 logical and physical pixels; Slint effective scale factor 1 |
| Original executable | 16,927,232 bytes |
| DB / posters / payload | 196,608 / 2,188,318 / 19,312,158 bytes; not an installer |
| Idle rendering, separate diagnostic | Initial aggregate 4 FPS; subsequent 34 one-second aggregates 0 FPS |
| Post-navigation private bytes and six-cycle plateau | No formal R4 values yet; no valid cross-stack comparison possible |

**Final R4 acceptance: INCOMPLETE / BLOCKED on desktop evidence.** Search,
selection, formal SQLite and formal poster/cache timing sets are now recorded.
A–F memory/counters, the six-cycle × three-process soak, and continuous motion
verification remain required. Neither the milestone specification nor the user
designates those items non-blocking. VMMap attribution and individual frame
timestamps are optional/limited, not substitutes for the missing evidence.

Next step: restore desktop control, use the retained frozen original executable,
verify that session's geometry/focus, and collect only the missing memory/soak
and continuous-motion evidence. Do not repeat valid core or latency measurements.
No R5 work, application optimization, dependency addition, commit or push occurred.

Final repository state: branch `spike/rust-slint`, HEAD
`95feb454864907a29bcf84f629278dc861e10336`; tracked changes are limited to
`Cargo.toml`, `src/main.rs`, and the oracle's configuration gate in
`src/library.rs`. New continuation files are `benchmark/r4_latency.rs`,
`benchmark/scripts/r4-gate.ps1`, `r4-latency.ps1`, `r4-latency-summary.ps1`,
ADR-0004, and the additional raw records. This summary and environment report
were updated. Pre-existing untracked R4 files remain preserved and uncommitted.
No user-visible application behavior changed. `git diff --check` passes;
Git's CRLF-conversion notices are informational. The only remaining acceptance
work is the desktop-dependent evidence described above.
