# R17 release validation — 2 October 2026

**NOT TECHNICALLY READY FOR RC**

**PUBLIC DISTRIBUTION NOT READY**

**Later 2 October follow-up:** Current evidence/status is in
[`2026-10-02-r17-native-blocker-followup.md`](2026-10-02-r17-native-blocker-followup.md).
Original observations below retain their original candidate identity.
Targetable native validation is now available: six final populated installed
graceful closes pass and empty/populated usable startup was observed. Three
native-discovered defects were corrected with regression proof; final suites
pass 221 tests, 16 ignored, with all nine gates and Windows package/smoke.
The additional ignored manual fixture separately passed native Discover
search/result/add and Library opening with a controlled provider.
Corrected source/tests/reports remain uncommitted/unpushed; exact-source native
CI/artifacts remain blocked. Technical/public verdicts are unchanged.
R18 remains suspended.

The remaining technical blockers are corrected-source native CI, native
keyboard/focus/accessibility verification, and an unresolved populated native
close timeout. Local correctness, deterministic product/data/failure tests,
bounded performance/resource measurements and Windows package preservation
have evidence. External licensing/signing decisions do not determine the
technical verdict. R18 remains suspended. No release was published.

## 1. Branch and source identity

Branch `main`. HEAD, local `origin/main` and live GitHub main are all
`6e24b2a109c8fa041075adcbcf1001bc4646cd82`. The sidebar correction, validation
tests and report updates are **uncommitted and unpushed**. The worktree was
already dirty at entry; its existing work was preserved. No commit, push,
tag, reset, restore, stash, checkout or `gh` command was used.

The final candidate is that base commit plus the exact inputs recorded in
[candidate-identity.json](r17-continuation-20261002/candidate-identity.json),
not the clean base commit. Its final source verification covers 53 build,
source, UI, script and workflow files.

| Identity | Value |
| --- | --- |
| Application | `bingee-desktop 0.1.0-rc.1` |
| Schema | 5 |
| Backup contract | V1 |
| Cargo.lock SHA-256 | `677DE18AACBAAACB0A11DCC60054CCE2338E964D84030943808DF2ED5459CC79` |
| Rust | `rustc 1.98.1 (48a229cea 2026-09-01)` |
| Cargo | `cargo 1.98.1 (797e8a9bc 2026-08-05)` |
| Final Windows executable SHA-256 | `A2AF9139B9204D060263A87E5420DC44AE4EADA7F68DC23FEC063CEC39D5E158` |

The older [R17 raw evidence](r17-validation-20261001/candidate-identity.json)
matched every input at entry. This continuation added one ordinary test to
`src/r17_validation.rs`; production code, the existing soak/crash functions,
dependencies, schema and UI remained unchanged. Earlier timings/resources
are measurements **before that test addition**. Final gates and the rebuilt
Windows package are **after it**. The executable was rebuilt and its new byte
identity is recorded separately; earlier numbers are not claimed to measure
these exact executable bytes. Source comparisons and final hashes establish
the unchanged production behavior for reuse, without rerunning expensive
unchanged measurements. The earlier candidate was also a dirty local tree.

### Live CI inspection

Both current workflow runs are successful **only for the pushed base**:

| Workflow | Windows job | macOS job | Linux job | Result |
| --- | --- | --- | --- | --- |
| [cross-platform, 36785367532](https://github.com/CydonianCitizen/Bingee-Rust/actions/runs/36785367532) | 110125376692 | 110125376636 | 110125376800 | Each PASS |
| [release-artifacts, 36785367485](https://github.com/CydonianCitizen/Bingee-Rust/actions/runs/36785367485) | 110125376633 | 110125376464 | 110125376791 | Each PASS |

Platform jobs and their steps were inspected, including build/test/package,
manifest and configured smoke checks. Expected other-platform skipped steps
are not failures. [current-ci.json](r17-continuation-20261002/current-ci.json)
retains job/step results and source revisions. There is no pushed correction
and therefore no new correction CI. **Cross-platform non-regression is
unverified for the sidebar fix.** R16 PASS remains valid for its original
revision; it was not repeated or relabelled as a revised-candidate PASS.

## 2. Files created or modified

This continuation adds the ordinary complete-journey regression in
`src/r17_validation.rs`, this report, and
`docs/run-reports/r17-continuation-20261002/` evidence. It updates the release
checklist, canonical limitations, README and roadmap references. Existing
changes in `src/main.rs`, `src/backup.rs`, `ui/app-window.slint`, R16/R17/R18
reports and licensing documents were retained. Evidence scripts are validation
helpers, not production instrumentation or features. Generated packages and
isolated test profiles remain under ignored `dist/` and `target/`.

## 3. Behavior and validation coverage

No additional product behavior was implemented. The existing NavItem fix
forwards focus to a FocusScope, draws its focus border, and uses the existing
page activation callback for Return/Space. Both retained regression cases
now exercise Library, Discover, History, Statistics, Calendar, Updates,
Settings, About and return to Home using synthesized Tab plus activation.
Correct page activation and bounded reachability pass. Existing Library,
detail, season/episode and Discover keyboard regressions pass.

The sidebar tests use a bounded search through Tab/activation sequences;
their trace includes 24 steps to Statistics and up to 39 to Updates. They
prove reachable destinations with no observed trap in those sequences, not
a reviewed native focus order. Some existing list/detail tests initially
focus a list with a pointer action. Native predictable focus, whole-product
keyboard-only traversal, screen-reader announcements and high-DPI behavior
remain unverified. Source roles/names review and existing 1280×800/1700×1100
offscreen renders are limited evidence. No native smoothness claim is made.

The new `r17_search_track_refresh_backup_restore_offline` test runs two fresh
Slint UI sessions against the same actual database file. It performs fake-TMDB
movie and TV search/add/detail/episode loading, personal tracking and ratings,
manual metadata refresh, Settings export, a later mutation, file validation,
confirmed restore and safety-copy verification. Complete Backup V1 semantic
equality and `quick_check` pass. The second session has no token; local pages,
two watch events and three episodes reopen correctly with zero new HTTP
requests. Posters in this focused journey use the existing offline fake;
poster/network/automatic-refresh stress is covered separately by the soak
and existing regressions.

### Final failure/data matrix

All named ordinary regressions execute in the final three full suites.
Detailed test names and source review are retained in
[audit.md](r17-validation-20261001/audit.md) and the full gate logs.

| Case | Evidence/result |
| --- | --- |
| Fresh and v1/v2/v3/v4 to v5 migration | Actual-file preservation, runtime backfill, reopen, integrity and foreign-key regressions PASS |
| Failed migration steps 2–5 | Rollback/preserved prior file and later successful retry PASS |
| Corrupt, foreign and future-schema databases | Refusal/preservation PASS; package corrupt-file smoke also PASS |
| Current, early and additive Backup V1 | Semantic round trips, omitted optional defaults, unknown fields, file restore/reopen PASS |
| Invalid/future/duplicate/dangling backups | Rejection and original-state preservation PASS |
| Backup write/restore failures | No-overwrite publication, hard-link/fallback, interrupted-copy cleanup, safety copy and transaction rollback PASS |
| Credentials | Missing/invalid/unavailable store, save/remove errors, redaction and no plaintext fallback PASS |
| HTTP/stale responses | Fake 401/404/429/server errors, invalid JSON, timeout/offline, cached fallback and stale-answer tests PASS |
| Personal writes | Committed UI state on failure, bulk rollback, metadata/personal-data firewall PASS |
| Active process crash | Export temporary-file write and metadata-refresh SQLite-journal write observed before process kill; reopen original state, lock reacquisition, integrity and FK checks PASS |
| Multiple instances | Current Windows package live-owner contention and subsequent recovery PASS |
| Product regression | New search/track/refresh/export/restore/offline restart test PASS |

The active-crash test is explicitly executed in the earlier release evidence,
not counted as run merely because it is ignored by normal suites. Forced
process termination is not simulated power loss or exhaustive crash-point
coverage.

### Windows package and installed upgrade

Current package creation with `-RequireInstaller` PASS. The packaged executable
matches the release build. All **696** portable checksum entries were verified.
Executable 28,834,816 bytes; ZIP 13,729,310 bytes; installer 10,021,453 bytes.
Exact file hashes are in
[package-identity.json](r17-continuation-20261002/package-identity.json).
Packaging records `6e24b2a... plus uncommitted changes`, not a clean SHA.

Current portable smoke PASS: fresh/reopen, live contention, idle kill/restart,
corrupt-file preservation, external profile paths and unchanged package.
The fixture-only flag correctly exits 1 in the normal package; ordinary
smoke app exits are 0. This uses an isolated empty profile.

The pushed R16 installer (SHA-256
`7001223B00B92DABD3DC339C9E8860AFDC18D6C5FEE949E37A7C627B7FBB4110`)
was installed into a workspace test directory, opened a 1,000-title v5
profile, then upgraded by the current installer. The installed executable
matched the current build; the populated database was byte-identical across
upgrade, current launch, controlled stop and uninstall. Result and profile
hash are in [nonempty-upgrade.json](r17-continuation-20261002/nonempty-upgrade.json).

**Populated graceful close did not pass.** `CloseMainWindow()` requested close,
but both the pushed installed binary and corrected installed binary remained
alive after 10 seconds and required controlled termination. A separate host
launch exposed no targetable Bingee window to Computer Use; a later close
request returned false and it remained alive for another 30 seconds. Empty
portable close passes and earlier populated startup runs recorded normal
exit. Root cause is unresolved: the current evidence does not distinguish
desktop/input-delivery conditions from a populated native lifecycle defect.
The bounded upgrade data-preservation PASS is **not** a graceful-close PASS.
Only owned test processes/installations were stopped/uninstalled.

## 4. Commands and exact outcomes

All nine final project gates PASS (exit 0):

| Command | Result |
| --- | --- |
| `cargo fmt --check` | PASS |
| `cargo check --locked` | PASS |
| `cargo test --locked` | 218 passed, 0 failed, 15 ignored |
| `cargo test --all-features --locked` | 218 passed, 0 failed, 15 ignored |
| `cargo test --features benchmark-fixture --locked` | 218 passed, 0 failed, 15 ignored |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS |
| `cargo clippy --all-targets -- -D warnings` | PASS |
| `cargo clippy --all-targets --features benchmark-fixture -- -D warnings` | PASS |
| `cargo build --release --locked` | PASS |

[local-gates.json](r17-continuation-20261002/local-gates.json) records elapsed
times and exact suite result lines; `final-gate-0.txt` through `final-gate-8.txt`
retain full output. An initial pre-journey nine-gate run also passed at
217 passed/15 ignored. Relative to the stated 216/13 baseline, the inherited
early/additive V1 regression adds one ordinary test, the new complete journey
adds one ordinary test, and the inherited R17 soak/crash tests add two ignored
tests: **216 + 2 = 218; 13 + 2 = 15**. Ignored tests are not silently counted
as executed by the normal gates.

Additional current commands:

- `cargo test --locked sidebar_navigation_is_reachable -- --nocapture`:
  2 passed, 0 failed, 231 filtered, exit 0.
- `cargo test --locked the_library_selection_drives_the_pane_and_it_is_keyboard_usable -- --nocapture`:
  1 passed, exit 0.
- Focused complete journey: final 1 passed, exit 0; final suites repeat it.
- `./scripts/package-windows.ps1 -RequireInstaller`: exit 0.
- `./scripts/smoke-windows-package.ps1 -WorkDir <isolated workspace directory>`:
  exit 0, full result in `portable-smoke.txt`.
- `nonempty-upgrade.ps1 -WorkDir <isolated workspace directory>`: final exit 0
  for install/upgrade/data preservation/uninstall, with both controlled stops
  and `native_graceful_close_verified=false` recorded.
- Portable checksums and frozen R4 hashes: PASS; 696 and 100 entries respectively.
- `cargo metadata --locked --offline --format-version 1 --filter-platform x86_64-pc-windows-msvc`:
  PASS; final package inventory matches the unchanged lockfile.

Failures were not hidden: the first new journey compile attempt referenced a
nonexistent callback (exit 101), then its fake search reply lacked
`total_results` and timed out (exit 101). Both were test-only mistakes fixed
before final verification. Unfiltered offline Cargo metadata failed at absent
foreign-target `aes v0.9.3` (exit 101); Windows filtered metadata and complete
653-package lockfile inventory verification pass. The first checksum helper
misread the sha256sum `*` marker (exit 1); corrected parsing verified all
unchanged R4 files. The first upgrade helper assumed a nonexistent adjacent
manifest (exit 1); it now verifies the previously inspected artifact record.
Sandboxed installer attempts exited 4 with Start Menu/uninstall-key access
denied. Host installation succeeded, but repeated populated close checks
failed after 10 seconds (exit 1). Their failure is retained; the final scoped
upgrade run records controlled termination instead of waiving that failure.

### Reused release measurements

Host: Windows 11 Home 10.0.26300 x86-64; Intel Core i5-10500H, 12 logical
processors; 24,983,332 KiB visible RAM. Native release executable and release
test processes, Rust/Slint versions as above; no debugger/profiler was
attached by the recorded measurement scripts. No cross-stack or platform
performance comparison is made. Raw scripts, per-run logs, sample counts and
summaries remain in `r17-validation-20261001/`.

Startup measures launch to responsive titled main-window detection, **not
observed first usable paint**, with ten separate exited processes per mode.
OS cache was uncontrolled; p95 is nearest-rank (the maximum with n=10).
Idle samples use 5-second settling and one approximately 30-second sample per
mode, process CPU time divided by wall time (one-core percentage).

| Native startup mode | Median / p95 ms | Idle one-core CPU | Final private / working-set bytes |
| --- | --- | --- | --- |
| Empty | 1,231.51 / 2,115.11 | 0.05168% | 211,861,504 / 184,500,224 |
| Populated, refresh off | 1,230.65 / 1,277.68 | 0.10307% | 226,156,544 / 195,190,784 |
| Populated, refresh enabled | 1,209.08 / 1,280.32 | 0.00000% measured | 227,323,904 / 195,125,248 |

Idle handles remained 543, 572 and 570–572; threads 28–29, 29–30 and 29–30.
The enabled mode had no supplied valid credential, so it does not establish
native idle behavior after credentialed live refresh. Native full-product
interaction footprint is not inferred from headless process measurements.

Three independent deterministic software-rendered 1280×800 product soaks
each completed 100 cycles over all nine pages, search/list/detail/personal
writes, posters and refresh, plus ten export/restore comparisons and final
file reopen equality. Fixture: 1,000 media, **17,505** episodes (partial
coverage explains the difference from a nominal 17,550) and 10,000+ events.
All exits 0; durations 53.642, 49.589 and 49.713 seconds; external process
sampling every 200 ms yielded 259, 239 and 240 samples.

| Soak run | Final-half private median bytes | Private / working-set peak bytes | Final-half handles / threads |
| --- | --- | --- | --- |
| 1 | 24,004,608 | 52,912,128 / 61,001,728 | 178–179 / 9–11 |
| 2 | 24,250,368 | 55,152,640 / 64,032,768 | 178–179 / 9–11 |
| 3 | 24,383,488 | 60,772,352 / 63,627,264 | 178–179 / 9–11 |

Final cache was 462,870 bytes and 307 fake requests per process; automatic
refresh considered/refreshed 100 requests with zero failures. Existing cache
stress also exercises 234 downloads/153 evictions under the 12 MiB limit.
Run 3 late per-cycle samples (65–99) span private 22,573,056→25,772,032 bytes,
working set 42,143,744→44,969,984, 179 handles and 9 threads. These short,
finite workloads show bounded observed resources, not indefinite leak freedom
or a native GPU/window resource soak.

Page timings below include software paint and each page's mixed soak actions,
100 samples/page/process. Discovery includes a fake search wait; Library
includes queries, selection and writes. These are not isolated navigation
latencies or native frame times.

| Page | Three-run median range ms | Largest observed ms |
| --- | --- | --- |
| Home | 72.06–77.20 | 309.74 |
| Library | 39.76–42.02 | 191.72 |
| Discover | 8.79–8.96 | 67.17 |
| History | 4.23–4.90 | 30.68 |
| Statistics | 57.77–60.51 | 87.52 |
| Calendar | 27.35–29.41 | 64.08 |
| Updates | 5.74–6.30 | 26.49 |
| Settings | 4.91–5.28 | 26.19 |
| About | 4.54–4.82 | 27.92 |

Separate release timing tests each ran in three processes. UI callback plus
software-render timings: 100 rapid episode clicks median 7.797–8.473 ms,
maximum 19.366; 20 bulk 250-episode writes median 16.370–18.760 ms, maximum
21.339; 50 movie toggles median 7.257–7.966 ms, maximum 10.826; 50 rating
changes median 7.090–7.749 ms, maximum 8.500. These measure actual synchronous
write/UI work without claiming native input-to-present latency.

Domain-only Home/Calendar warm operations were 33.58–36.89 / 8.73–9.40 ms
(20 operations per process). Statistics cold-statement-cache first open plus
software paint was 39.123–61.831 ms; warm selection medians 25.977–33.612 ms,
maximum 35.734. First-process metadata read medians were 0.160 ms movie,
0.262 ms 60-season series, 0.300 ms 250-episode season, 0.587 ms two season
reads; corresponding commit medians 4.856/5.868/16.088 ms. Library query
medians and all repetitions remain in the logs; these are cached/in-memory
query scopes, not cold disk or keystroke-to-paint timings.

Large Backup V1 output: 6,319,476 bytes. Export 23.8–30.9 ms, serialization
9.3–10.7, parse/validation 32.0–35.5, restore 430.3–455.6 across three release
processes. Safety/preservation is tested separately. SQLite 3.53.2 static
SELECT/WITH plans with NULL bindings and both Library sort variants were
captured; indexed joins and expected folded-sort scans/temp B-trees were
reviewed. This does not cover every dynamic query/value distribution. No new
performance threshold or unsupported smoothness PASS was invented.

## 5. Dependencies and frozen inputs

No dependencies added or updated. Direct resolved versions: image 0.25.10,
keyring 4.2.0, rusqlite 0.40.2, rfd 0.17.2, serde 1.0.229,
serde_json 1.0.151, slint/slint-build 1.17.1, ureq 3.4.2. Build/dev/runtime
kinds and complete name/version/license inventories are recorded separately.
All **653** lockfile package versions match the prior all-target inventory;
Windows resolves 388 packages and packages 325 linked runtime crates. Runtime
dependencies cover UI/rendering/platform integration, SQLite, JSON, images,
HTTP/TLS, secure storage and native dialogs; compiler/build-only and dev
dependencies are not relabelled as shipped runtime crates. Licenses and
notices were packaged; owner distribution-license choices remain external.

Frozen R4 DB SHA-256:
`61918DB3B71A4B0F77154ABB498268F8D7F327469355B569AAD993B58C69FEC3`.
Poster manifest SHA-256:
`4BE13BC484B971585C04CA4322629CECC837DB3BB823715FE72F69C1AFD0978C`.
All 100 poster checksums match. No frozen input changed.

## 6. Assumptions and security/privacy scope

Unchanged production source supports reusing earlier local deterministic
evidence; it does not imply new remote CI or native accessibility proof.
Fake provider traffic tests application behavior and failure handling without
requiring a live credential. Actual first-paint/keyboard evidence needs a
targetable desktop. Installer access denials are distinguished from application
failures; the subsequent populated close timeout has no established cause.

The final bounded source review found no new application security/privacy
defect. Keyring-only real-token storage/redaction, finite TLS HTTP deadlines
and body limits, parameterized SQL/typed backup validation, safe export and
transactional restore, poster key/path restrictions and local rotating logs
are covered by the retained audit and passing regressions. Personal tracking
and backups have no upload path; provider queries intentionally send search
text/IDs. API transport is HTTPS; provider image configuration can accept
HTTP, so all-image HTTPS enforcement is not claimed. The backup metadata
size precheck is not a concurrent-file-change or constant-memory guarantee.
This is sanity review, not a penetration test, vulnerability-database scan,
native secure-store prompt audit or screen-reader certification.

## 7. Genuine technical RC blockers

1. **Corrected-source native CI/artifacts are absent.** Both workflows must
   pass on the source containing the correction and final tests/reports, with
   individual Windows/macOS/Linux job inspection and required package/smoke
   checks. Existing base-SHA success cannot certify the fix.
2. **Native keyboard/focus/accessibility and usable-startup verification remain
   incomplete.** Headless page reachability, source semantics and renders do
   not establish predictable OS focus/no trap, full keyboard-only list/detail
   interaction, native high-DPI rendering or first usable paint. Computer Use
   returned only the Codex window, including after a host test launch; no
   forbidden Codex input or guessed window target was used.
3. **Populated native close/restart outcome is unresolved.** Both installed
   base/current candidates timed out after a successful close request. Empty
   close and data-preserving controlled restart pass. Establish input delivery
   and native lifecycle on a targetable desktop before deciding whether an
   application correction is needed; this is not yet attributed to the fix.

No polish preference, optional feature, licensing choice, signing or
notarization requirement is included in this technical blocker list.
External owner decisions still independently prevent public distribution.

## 8. Smallest corrective plan and stop

Have the authorized owner commit/push the frozen local correction and tests;
this session cannot do so under `.claude/settings.json`. Inspect both new
workflows' three platform jobs and required artifacts/smokes on that exact
SHA. On a targetable native desktop, run the nine-page keyboard-only sequence,
list/detail regression, focus/semantics/rendering checks and observed usable
startup; reproduce populated close/restart and resolve only a proven defect.
If production source changes, remeasure only affected cases and rerun the nine
gates, preserving prior evidence as belonging to its prior source identity.

Retain valid data/soak/backup/timing evidence within its stated scope. When
these technical blockers close, update the R17 decision/checklist and mark
R18 as the next optional stabilization/polish phase, then stop. **R18 was not
started in this continuation.** PC shutdown is handled only after evidence
and reports have been saved, as explicitly requested by the user.
