# R4 status: baseline frozen, interactive acceptance blocked by environment

**Final R4 status (13 September 2026): PARTIAL / ENVIRONMENT-BLOCKED.**

The non-interactive R4 baseline is complete and frozen. Three required
interactive items could not run because no desktop-control session was
available. This is a limitation of the measurement environment. It does not
point to an application defect, and it does not mean those items passed.

R5 (packaging and cross-platform sanity) may proceed on this basis. It does not
turn R4 into a pass. The missing items stay required before any cross-stack
memory or motion comparison.

Details and exact tables: [summary.md](summary.md) (the measurement report as
written on 11 September 2026). Environment: [environment.md](environment.md).
Workload contract: [benchmark/R4-workload.md](../../../benchmark/R4-workload.md).

## Baseline identity

| Item | Identity |
| --- | --- |
| Source commit | `95feb454864907a29bcf84f629278dc861e10336` (Milestone 3) |
| Opt-in harness | `r4-measurement` feature, `benchmark/r4_latency.rs` ([ADR-0004](../../adr/0004-r4-opt-in-measurement.md)). It was uncommitted at measurement time; the exact source hashes are in `raw/latency-20260911T201032397Z/environment.json`, and the patch is in `instrumentation.patch` |
| Frozen original release executable | 16,927,232 bytes, SHA-256 `AEAF7F294E5D829CF2F693E768F79BC8AF6A1BB0416954296965F78218B9F35E` |
| Opt-in latency executable | 17,000,960 bytes, SHA-256 `DB1CFDEB8A47CCD4AB28B89A4ACC59DF15DC63FE1300E751F71CDB39D6DE67E3` |
| Default release rebuilt after the gated additions | SHA-256 `DB4A35DE850C353C97E05FCBC30BD55AB604D7CA70C6673D2646CB61E624BCB7` |
| Final ordinary release, feature disabled | SHA-256 `4A12F3863BE136A1F78C62C3670EB3278E1C879C64CA26D65835ACC34DD90D1F` |
| Frozen SQLite database | 196,608 bytes, SHA-256 `61918DB3B71A4B0F77154ABB498268F8D7F327469355B569AAD993B58C69FEC3` |
| Poster manifest `SHA256SUMS` | SHA-256 `4BE13BC484B971585C04CA4322629CECC837DB3BB823715FE72F69C1AFD0978C` |
| Toolchain / Slint | rustc and cargo 1.98.1, `x86_64-pc-windows-msvc`; Slint 1.17.1, FemtoVG renderer with OpenGL backend (measured diagnostic) |
| Machine | Windows 11 Home 10.0.26200, Intel Core i5-10500H (6C/12T), 23.8 GiB RAM |

The retained executables and the frozen database live only under the
git-ignored `target/release/` (`bingee-r4-frozen.exe`, `bingee-r4-latency.exe`,
`bingee-r4-default.exe`, `bingee-desktop.exe`, `bingee-spike.db`). They are not
in Git, and `cargo clean` deletes them. The frozen executables load posters from
the absolute checkout path they were built in, so they need this checkout to
stay at its current location. Moving it makes them show placeholders.

## Measured and complete

All values below come from the raw files. Units are milliseconds unless stated
otherwise. Nothing was re-measured for this freeze.

| Measurement | n | Median | p95 | Max | Endpoint / notes |
| --- | ---: | ---: | ---: | ---: | --- |
| Startup proxy | 20 | 654.5962 | 672.2140 | 712.7205 | Process-cold, OS-cache-warm; `Start-Process` → input-idle + responsive `Bingee Desktop` window. Min 635.9026. Not first paint |
| Search restore / empty (1,000 rows) | 150 | 1.611050 | 2.377700 | 4.304100 | Callback entry → return (SQL, model reset, detail setters). Not a painted frame |
| Search `HARBOR` (25 rows) | 150 | 0.728350 | 1.256300 | 2.202300 | Same |
| Search `NÖRDLICHE` (39 rows) | 150 | 0.634300 | 1.105600 | 1.873500 | Same |
| Search `zzzz` (0 rows) | 150 | 0.481550 | 0.760800 | 1.063400 | Same |
| Search clear (1,000 rows) | 150 | 1.293300 | 1.750600 | 2.391400 | Same |
| Selection, cached poster | 150 | 0.009600 | 0.015000 | 0.040900 | Callback → detail property update |
| Selection, poster-cache miss | 150 | 0.860800 | 0.931900 | 1.255100 | Same, includes one decode |
| Selection after long navigation | 150 | 0.045700 | 0.153100 | 1.298100 | Same |
| Selection after `HARBOR` | 150 | 0.040450 | 0.120300 | 0.181600 | Same |
| SQLite existing open | 150 | 0.686050 | 0.851200 | 1.569000 | `Connection::open` + schema/count transaction |
| SQLite fetch all 1,000 | 150 | 1.225300 | 1.348600 | 1.565700 | Query + owned-row mapping, warm caches |
| SQLite `HARBOR` | 150 | 0.216450 | 0.398400 | 0.419000 | Same |
| SQLite `NÖRDLICHE` | 150 | 0.243650 | 0.462700 | 0.497000 | Same |
| SQLite `zzzz` | 150 | 0.181850 | 0.333600 | 0.352500 | Same |
| Poster first-path load / decode | 300 | 1.050650 | 1.225100 | 1.596800 | `Image::load_from_path`, 240×360 JPEG |
| Poster forced re-decode | 1,500 | 0.917000 | 1.197000 | 1.764800 | Slint internal LRU overflowed |
| Slint internal-cache hit | 1,500 | 0.036900 | 0.054300 | 0.107400 | Primed path |
| Bingee explicit-cache hit | 1,500 | 0.000100 | 0.000200 | 0.001000 | At the 100 ns timer floor |
| Nine new posters, one batch | 150 | 7.429050 | 8.197500 | 11.029100 | Nine misses + nine evictions per batch. Not layout/upload/paint |

| Idle run (5 s settle, ~30.4 s untouched) | CPU s | % of one core | % of machine | Final working set, bytes | Final private, bytes |
| --- | ---: | ---: | ---: | ---: | ---: |
| 1 | 0.062500 | 0.20562 | 0.01714 | 131,502,080 | 174,997,504 |
| 2 | 0.015625 | 0.05147 | 0.00429 | 131,416,064 | 175,722,496 |
| 3 | 0.031250 | 0.10288 | 0.00857 | 131,727,360 | 176,144,384 |

| Footprint (not an installer) | Bytes |
| --- | ---: |
| Release executable | 16,927,232 |
| Seeded SQLite database | 196,608 |
| 100 benchmark JPEGs | 2,188,318 |
| Required payload sum | 19,312,158 |

Also complete: the correctness gates before every measurement set
(`raw/20260911T192701561Z/gate.json`, `raw/continuation-baseline-gate/`,
`raw/continuation-instrumented-gate/`, `raw/final-verification/`), the
environment inventory, the dependency inventory (2 runtime, 1 build and 1 dev
dependency; 330 resolved package identities on the Windows target), the timer
overhead (9,000 pairs: median 0 ns, p95 and max 100 ns), and a separate idle
render diagnostic (first one-second aggregate 4 FPS, then 34 × 0 FPS).

## Missing because of the environment

| Required item | State |
| --- | --- |
| A–F interactive memory checkpoints (three fresh processes) | Not run |
| Six-cycle soak × three processes (private-memory plateau) | Not run |
| Continuous-motion verification (wheel, PageUp/PageDown, Home/End, typing, clicks) | Not observed |

The reason is the same for all three. The frozen protocol needs real keyboard
input and visual observation. On 11 September 2026 the desktop-control helper
failed on the initial call, a retry and a reset/retry in the original R4
session ([environment.md](environment.md)). All three continuation attempts
failed the same way (`raw/continuation-baseline-gate/desktop.json`), with
`Computer Use native pipe is unavailable: failed to connect native pipe: The
system cannot find the file specified. (os error 2)`. On 13 September 2026, a
single availability probe found no desktop-control tool in the session at all.
It was not retried. No A–F, soak or motion data exists, and none was
substituted.

## Unknown / not proven

- **Process-private memory plateau** under repeated navigation and search.
- **Final leak assessment.** No leak has been demonstrated by available
  evidence, but the required multi-cycle private-memory plateau test could not
  be performed because interactive desktop control was unavailable.
- **Explanation of the R2→R3 private-memory delta** (about 30–32 MiB). No
  VMMap or heap trace exists. Cache budgets bound decoded-image accounting but
  do not attribute process memory.
- Painted-frame latency, first-paint time, visual scrolling smoothness, and
  input-to-frame latency. No R4 measurement covers them.

## Conclusions supported by the evidence

- Local search is effectively instantaneous for the frozen workload. The worst
  callback-to-property-update sample is 4.304100 ms, well below a 16.7 ms
  frame. Painted response was not measured.
- SQLite query cost does not justify async or threaded execution at 1,000
  records. The worst fetch-all is 1.565700 ms.
- Measured poster decoding does not currently justify background decoding. The
  worst nine-poster batch is 11.029100 ms before rendering. Motion/frame
  evidence is still missing, so this is not a smoothness claim.
- Idle CPU is effectively negligible: at most 0.01714% of the 12-logical-CPU
  machine over about 30 seconds. The idle diagnostic shows no continuous
  repaint.
- The explicit poster cache is bounded by design (12,582,912 estimated bytes,
  36 posters at 240×360) and by passing unit and headless-render tests. That is
  a code-level bound, not a process-memory result.
- Startup is well below one second on the measured readiness proxy (median
  654.5962 ms). First paint and subjective feel were not measured.
- No formal process-level leak conclusion can be made without the soak.

Not supported and not claimed: "no memory leak", "scrolling is smooth", or any
Rust-versus-Avalonia statement.

## Evidence audit, 13 September 2026

Read-only checks, no re-measurement:

- All 70 pre-continuation raw files match `raw/continuation-baseline-gate/preserved-inputs.json`.
- The ten measured source inputs match the hashes in
  `raw/latency-20260911T201032397Z/environment.json`.
- The four retained executables and the frozen database under `target/release/`
  match the SHA-256 values above.
- All 100 posters match `SHA256SUMS`, whose own hash matches the contract.
- Recomputing the pooled latency statistics from the nine raw CSVs (20 groups,
  16,050 rows) reproduces `latency-statistics.json` exactly.
- Every file reference in `summary.md`, `environment.md` and
  `benchmark/R4-workload.md` resolves.

`.gitattributes` marks `docs/measurements/**/raw/**` and the poster manifest
`-text`. Git therefore stores and checks out their exact bytes (65 CRLF files,
36 LF files, 35 single-line or empty files), and the recorded hashes stay
verifiable on any platform and any `core.autocrlf` setting. The hashed source
files were LF-encoded at measurement time, matching their Git blobs. On a
Windows checkout with `core.autocrlf=true`, hash them from `git show
<commit>:<path>` instead of from the working tree.

## Comparison contract for the Avalonia spike

[benchmark/R4-workload.md](../../../benchmark/R4-workload.md) is internally
consistent and unchanged: 1,000 deterministic records, the frozen SQLite file,
the 100 posters and their manifest, the 12,582,912-byte explicit cache charged
at width × height × 4, the four fixed queries, the A–F and soak navigation
sequences, the callback/decode latency endpoints, the startup proxy and the
memory checkpoints. No parameter was changed.

One new fact makes the database requirement easier to meet. On 13 September
2026, the R5 package's first launch seeded a fresh database whose SHA-256 is
exactly the frozen `61918DB3…FEC3` (same generator, SQLite 3.53.2, Windows
x64; `docs/measurements/R5-packaging-cross-platform/raw/smoke-20260913T221112Z/smoke.json`).
The contract file can therefore be regenerated by launching that build once.
The hash, not the file's origin, identifies it.

## Scripts after the move to `main`

The Git model changed after measurement. On 13 September 2026 `spike/rust-slint`
was merged into `main` (same commit) and deleted. The branch-name assertions in
`benchmark/scripts/r4-core.ps1` and `r4-latency.ps1` were removed for that
reason. Nothing else in them changed. The exact runner copies that produced the
data stay in `raw/20260911T192701561Z/runner.ps1` and
`raw/latency-20260911T201032397Z/runner.ps1`. `r4-latency.ps1` still requires
HEAD to be the baseline commit, so it only reproduces the frozen context.

R5 moves the application's database to `<executable dir>/data/` and resolves
posters from `<executable dir>/assets/posters/` first. The R4 scripts assume the
R3-era layout: the database beside the executable and posters from the build
checkout. A post-R5 build run through them is **not** the frozen baseline.
Collect the missing A–F, soak and motion evidence with the retained
`target/release/bingee-r4-frozen.exe`, following `benchmark/R4-workload.md`,
and do not repeat the completed measurements.
