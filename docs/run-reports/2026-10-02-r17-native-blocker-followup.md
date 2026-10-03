# R17 native blocker follow-up — 2 October 2026

Current technical verdict: **TECHNICALLY READY FOR RC** for corrected pushed
`3ab08c32a2fef16cf8555e9db467f681c1dbd1a2`.

Current public distribution verdict: **PUBLIC DISTRIBUTION BLOCKED BY EXTERNAL DECISION**.

The corrected-source CI closure at the end of this report updates the final
verdict. Earlier sections retain the original native follow-up's observations
and source state; its unpushed-source/CI blocker is historical and now resolved.

R16 remains PASS. R18 remains SUSPENDED and was not started. This run addresses
only corrected-source identity, native input/rendering/startup and graceful
close. Evidence is in `r17-blocker-followup-20261002/` beside this report.

## Source and CI identity (historical native follow-up)

Branch is `main`. HEAD, local `origin/main`, and live GitHub main are
`6e24b2a109c8fa041075adcbcf1001bc4646cd82`. The worktree was already dirty:
sidebar correction, R17 tests and current reports were uncommitted and
unpushed. They remain so. **R17 remains blocked.** `.claude/settings.json`
denies commit, push, tag and `gh`; no such operation was attempted.

All 53 inputs in the preceding report's candidate manifest matched at entry;
the entry release executable SHA-256 was
`A2AF9139B9204D060263A87E5420DC44AE4EADA7F68DC23FEC063CEC39D5E158`.
`source-comparison.json`, `local-identity.json` and `entry-git-status.txt`
retain the comparison. Additional production changes are episode/Updates
focus indication, Updates row accessibility semantics, and synchronous title
loading before selecting an episode target. Three ordinary regressions were
added to `src/main.rs`. A test-only ignored native keyboard fixture was added
to `src/r17_validation.rs` for controlled-provider Discover validation.

The intermediate episode-only executable is
`DD0D0513F00858D2D82FA61589E231C020233224D0C1524144678B902CCA9B6A`.
The final release, portable and installed executables all hash to
`B324D6FE7A8C7FCAF06B4025F839D36C9296BC87A2177ABA49C84EB74BEE3016`.
`final-candidate-identity.json` retains all 53 final input hashes; only
`src/main.rs`, `src/r17_validation.rs` and `ui/app-window.slint` differ from
the entry manifest. The last change is excluded from production builds.

The public API was read directly, without `gh`. Latest successful
`cross-platform` run 36785367532 and `release-artifacts.yml` run 36785367485
both identify **the old pushed SHA**, not this local correction. Individual
Windows, macOS and Linux jobs and their actual steps were inspected and saved
in `remote-ci.json`. Normal format/check/test/Clippy/release steps passed on
all three systems. Windows installer/package structure and manifest passed;
macOS bundle, Mach-O/plist/resources/archive structure and manifest passed;
Linux package prerequisites, structure and Xvfb smoke passed. macOS native
launch is not a configured step. Windows runtime smoke is separate local
evidence. These old successes are not proof for the corrected candidate.

## Native environment and regression

Windows native windows were targeted through the computer-use plugin's Sky
API. Host-visible launches with isolated `BINGEE_HOME` profiles worked;
the earlier sandbox process was untargetable and was cleaned up separately.
No personal populated profile or real credential was used. Fixture copies
contain 1,000 titles and 10,000 watch events. Native input observations and
UIA trees are retained in `native-entry-trace.json` and the final trace.

On the entry executable at `SLINT_SCALE_FACTOR=1.25`, initial Tab focused Home
and reverse Tab reached the episode list without a selected row. That list
had no visible focus until Down selected an episode. This is a reproduced
defect, independent of the old close timeout. The smallest fix draws the
accent border around the episode area while it has focus and no selected row;
selected rows retain their existing indication. No selection, watched state,
shutdown, poster architecture or worker ownership changed.

The focused test failed before the fix with zero changed pixels (exit 101),
then passed after the fix (exit 0). It uses Tab/reverse Tab without pointer
focus, verifies a visible render change without changing selection, and
verifies Down selects the first episode.

The populated Updates list also lacked distinct focus indication and exposed
its actionable rows only as text. Native Return on an S0 E1 update opened the
right title but incorrectly retained Season 1. Focused tests reproduced both
failures: zero changed focus pixels and `Season 1` instead of `Specials`
(exit 101 each). Updates now uses the existing list-item role, useful name,
selection/default-action semantics and accent focus border. Empty Updates
also has a border. The navigation callback explicitly loads the requested
title before consulting season/episode models; Slint's deferred `selected-id`
handler had left the previous title's models available. Both focused tests
pass after the fixes. Final installed native retests show the named Updates
option, visible outline, correct Specials E1 target and Library selection
focus. Calendar opens that same correct local target.

## Native keyboard, accessibility, scaling and startup

No pointer action established initial content focus. Initial Tab reaches
Home with an accent outline. Pointer use was limited to resizing and native
titlebar close. All nine sidebar entries are distinguishable named buttons.
Tab/reverse Tab order is deterministic on exercised paths; list-to-sidebar
transitions escape normally. Return/Space activate the implemented buttons;
arrows change Library items, seasons, episodes, History selection and dates.

| Surface/path | Native result and source scope |
| --- | --- |
| Home, Library, Discover, History, Statistics, Calendar, Updates, Settings, About | All reached/activated by keyboard across entry/intermediate sessions. Unchanged components reused at their named source identity. Final changed Updates and target-navigation paths retested on the final installed executable. |
| Library → item → season → episode | PASS on intermediate executable: arrows change title/season; episode focus visible without preselection; Space/Return toggle the isolated episode while retaining focus. |
| History → target | PASS for shipped watch-selection behavior: visible selection/Down progression. Return does not open details; current History supports selection/removal. No navigation feature added. |
| Calendar → episode | PASS on final installed executable: Right changes October 2 to 3, Left returns to 2, Return opens Specials E1; Library focus is visible. |
| Updates → episode | PASS on final installed executable after fixes; useful role/name, visible focus and correct target verified. |
| Backup / Restore | PASS on intermediate executable: keyboard reaches Export/Choose; Return/Space opens dialogs; typed filename exports a real isolated backup; selection validates 1,000 titles/10,000 watches; Tab scrolls focused confirmation into view; Return completes restore and retains pre-restore snapshot. Next Tab reaches Home after the confirmation disappears. Subsequent fixes did not affect this path. |
| Discover → result → add/open | PASS in native controlled-provider fixture: Tab reaches search; typed query returns named Movie/TV options; Tab gives visible row focus; Down/Up changes preview; Space adds TV and Return adds Movie; Tab escapes completed add state to Home; Library Return and arrows open both correct titles. This uses production UI/callbacks with test-only fake TMDB responses, not a live service credential or the release executable. |

The manual native fixture executable hashes to
`9B917E811EF60AFAAC026C9D1D4ADD65D6595C4CA8BE169CE63172DF2C83422D`.
It was launched as an actual Windows window with an explicitly isolated
profile, in-memory fake credential and existing fake server; keyboard input
came through Sky, not synthesized Slint input or invoked callbacks. It uses
the same production UI and application wiring. Search/add/detail network
responses and persistence used production code. Movie and TV share TMDB ID
603 and remain distinct. Native metadata refresh and TV season/episodes
loaded correctly. The manual test passed (1 passed, 236 filtered out),
exited 0 and checked SQLite integrity after normal close. Launch/exit and
stdout/stderr records retain identity in `native-discover-*` files. There is
no direct Open button in Discover's shipped preview: added titles are opened
through Library. After add, focus temporarily stays on disabled In Library
without its prior outline; the next Tab visibly reaches Home, and reverse
Tab reaches the result. This is recorded as a transient focus deviation;
no focus trap or actionable invisible control was observed.

UIA exposes useful sidebar names, Library/season/episode option names,
Settings token description, refresh and Backup/Restore button names. Custom
FocusScope focus sometimes reports the window region despite a visible row
outline. Native file-dialog focus/value reports sometimes lag filename focus
shown in screenshots; resulting files/status confirmed delivery. Empty pages
without rows can retain region focus until next Tab reaches a named control.
These deviations are retained; no full keyboard/accessibility audit PASS or
WCAG certification is claimed. No screen-reader output capture was available:
**screen-reader behavior not directly verified**.

Native 125% Slint scaling covered Library/poster placeholders/detail,
Calendar, Statistics and Settings. Sidebar/focus outlines and inspected
layouts remained legible; long Statistics labels use intended ellipsis;
Settings scrolls focused confirmation into view. Real downloaded poster
bitmaps were unavailable in the offline fixture. This is
`SLINT_SCALE_FACTOR=1.25` on a native window, not an OS monitor DPI change or
all-monitor proof. Poster architecture was unchanged.

Bounded native empty/populated launches showed rendered UI and accepted
initial Tab with visible Home focus; no blank/frozen state was observed.
Final observations are in `final-empty-usable-startup.json` and
`final-populated-usable-startup.json`. These are informal observations;
observer/shell latency is included, not first-paint performance timing.
Earlier formal window-detection timings retain their original definition.

## Populated graceful close and restart

**previous timeout not reproduced on targetable native desktop**.

The final actual installed executable completed six populated repetitions:

| Preference | Repetition | Path before close | Request-to-observed-exit |
| --- | --- | --- | ---: |
| Off | 1 | Library/detail, Updates and Calendar at 125% | 368 ms |
| Off | 2 | Restart, no content interaction | 343 ms |
| Off | 3 | Restart, no content interaction; input guard refreshed | 481 ms |
| On | 1 | No content interaction at 125% | 389 ms |
| On | 2 | Restart, History, Settings and 10,000-watch Statistics | 352 ms |
| On | 3 | Restart, no content interaction | 298 ms |

Every final repetition closed the window, exited with code 0, wrote poster
statistics and `Bingee Desktop closed`, preserved its DB byte hash, and
allowed exclusive reopening of SQLite DB and profile lock. Native titlebar
Close also passed on an intermediate enabled-refresh restart. No populated
native repetition required forced termination. Entry/intermediate runs,
including episode writes and Backup/Restore, retain separate provenance;
their deliberate mutations explain DB hash changes. Final runs made none.

Warm final starts sampled 13 threads/311 handles; initial scaled installed
start sampled 19/417. No per-worker stack dump or in-flight census was needed.
Poster requests ran at startup and failed normally on synthetic paths.
Refresh On was verified in native Settings and copied preferences; no live
credentialed refresh request was induced. Shutdown logs establish event-loop
return/final logging; exclusive file access establishes released SQLite/lock
handles. No process survived to hold workers alive. No speculative shutdown
fix or permanent instrumentation was added. The prior close-request timeout
remains historical environment/input-delivery-inconclusive evidence; its
cause is not asserted. The owned test installation was uninstalled (exit 0),
preserving both populated profiles. All owned native processes were closed.

## Correctness, packages and frozen inputs

All final commands exited **0**; exact commands/durations/logs are retained
in `gates.json` and corresponding text files.

| Command | Exact result |
| --- | --- |
| `cargo fmt --check` | PASS |
| `cargo check --locked` | PASS |
| `cargo test --locked` | 221 passed, 0 failed, 16 ignored |
| `cargo test --all-features --locked` | 221 passed, 0 failed, 16 ignored |
| `cargo test --features benchmark-fixture --locked` | 221 passed, 0 failed, 16 ignored |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS |
| `cargo clippy --all-targets -- -D warnings` | PASS |
| `cargo clippy --all-targets --features benchmark-fixture -- -D warnings` | PASS |
| `cargo build --release --locked` | PASS |

Baseline change: exactly three new ordinary tests, 218 → 221; ignored count
changes 15 → 16 for the manual native fixture, which was separately run and
passed. Intermediate episode-only 219/15 and pre-fixture 221/15 gates are
separately retained. Adding the test-only fixture leaves the release binary
hash unchanged, so the matching Windows package/smoke/native evidence remains
applicable without repeating the package checks.

`scripts/package-windows.ps1 -RequireInstaller` and
`scripts/smoke-windows-package.ps1 -WorkDir <isolated target path>` exited 0.
All 696 portable hashes match; PE machine is x64, subsystem GUI, notices/
licenses/README exist. README names `0.1.0-rc.1` and explicitly records
`6e24b2a... plus uncommitted changes`. Artifact sizes/hashes are retained in
`package-validation.json`; installed/release executable hashes match.
Installer, native installed runs and owned uninstall exited 0. Portable smoke
covers fresh/reopen/lock contention/idle forced-restart/corrupt preservation
and fixture-flag refusal; its kills are not graceful-close evidence.

Frozen R4 DB remains
`61918DB3B71A4B0F77154ABB498268F8D7F327469355B569AAD993B58C69FEC3`.
Poster manifest remains
`4BE13BC484B971585C04CA4322629CECC837DB3BB823715FE72F69C1AFD0978C`;
all 100 poster hashes match (`frozen-r4.json`).

Before-fix test failures are expected and retained. An initial new test used
the wrong SharedToken type (compile exit 101), corrected to existing
`SharedToken::default()` before reproducing failures. A native helper first
double-applied the UTC offset to a JSON date; its original record is preserved,
the conversion was repaired, and later records include UTC exit times. A
package helper incorrectly required a PE ProductVersion string absent in
both intermediate/final binaries (exit 1); it was corrected to the actual
package/UI version contract. A read of local `PACKAGE-MANIFEST.json` exited 1:
that file is CI-generated; local artifact hashes were saved separately. One
native input guard rejected a key until state refresh; only successful
delivery was timed. These helper failures are not app shutdown failures.
The final status-count helper initially treated `??` as PowerShell wildcard
characters; it was corrected to literal `StartsWith("??")` and asserted the
actual eight tracked modified/twelve untracked entries. Final verification
confirms all 53 source hashes, all nine successful gates, unchanged release
identity and `git diff --check` exit 0.

## Scope preserved

The prior report's migration, Backup V1 compatibility, failure matrix, bounded
soak, page/write timings, cache bounds, refresh stress and Windows populated
upgrade/uninstall preservation evidence is retained with its original source
identity. Entry source manifest matches establish that evidence's starting
scope. Subsequent fixes affect focus/accessibility rendering and target
navigation; they do not establish new exact-binary performance measurements.
Unchanged expensive exercises were not rerun. The mandated correctness and
Windows package/smoke checks are rerun because production UI source changed.

No dependencies were added or updated. Cargo.lock remains the prior resolved
inventory (653 all-target versions, 388 Windows packages, 325 linked crates).
Slint licensing/intended use, TMDB agreement, signing, notarization and public
distribution decisions remain separate external decisions. They do not cause
the technical failure.

## Remaining work and next step (historical native follow-up)

This run modified `src/main.rs`, `src/r17_validation.rs`, `ui/app-window.slint`,
`docs/release-checklist.md`, `IMPLEMENTATION_PLAN.md`,
`docs/known-limitations.md` and the latest prior R17 report; it created this
report and its raw evidence/helper directory. Other entry worktree changes
were preserved. Branch remains `main`, with eight modified tracked paths and
12 untracked path entries (including prior reports/tests). No commits,
branches, pushes, dependency changes or R18 work were made. The final
source manifest still matches the source on which final gates were run.

The authorized owner must commit/push the corrected source, tests and reports;
then inspect both native workflows and individual package/smoke steps for
that exact SHA on Windows, macOS and Linux. This is the remaining genuine
technical blocker. Native Discover is now verified with a controlled provider;
screen-reader and OS monitor-DPI claims remain absent. No R18 work or release publication is
authorized by this run.

## Corrected-source CI closure — 2 October 2026

### Source and individual job results

This continuation starts on clean `main`. HEAD, local `origin/main` and live
GitHub `main` all identify **`3ab08c32a2fef16cf8555e9db467f681c1dbd1a2`**,
subject `R17: close native focus and lifecycle blockers`, parent
`6e24b2a109c8fa041075adcbcf1001bc4646cd82`. The pushed diff includes
episode/Updates visible focus and accessibility semantics, synchronous title
loading before episode-target selection, the three ordinary regression tests,
the manual native test fixture, and R17 report/checklist/plan updates.
All 53 current source inputs match `final-candidate-identity.json` from the
native follow-up. Cargo.toml/Cargo.lock, frozen fixture/benchmark and
measurement paths have no change from the parent. No dependency changed.

Both workflows completed successfully on that exact corrected SHA, attempt 1:

| Platform | cross-platform run 37064096145 | release-artifacts run 37064096196 |
| --- | --- | --- |
| Windows x64 | PASS, job 111027502242 | PASS, job 111027502382 |
| macOS arm64 | PASS, job 111027501990 | PASS, job 111027502520 |
| Linux x64 | PASS, job 111027502357 | PASS, job 111027502479 |

Runs: [cross-platform](https://github.com/CydonianCitizen/Bingee-Rust/actions/runs/37064096145)
and [release-artifacts](https://github.com/CydonianCitizen/Bingee-Rust/actions/runs/37064096196).
Individual step records and decoded job logs were inspected, not just workflow
conclusions. Saved records and log excerpts are in `r17-ci-closure-20261002/`.
No success from the parent or an older SHA certifies the corrected candidate.

Every cross-platform job passes `cargo fmt --check`, `cargo check --locked`,
`cargo test --locked`, all three configured Clippy feature combinations with
`--locked` and `-D warnings`, and `cargo build --release --locked`.
Each package job passes format/check, default and all-feature tests, all-feature
Clippy, and its platform's release build through the packaging script.
Every CI test invocation reports **221 passed, 0 failed, 16 ignored**.
Fixture-only tests retain the existing local evidence; the workflows do not
configure a separate fixture-only test invocation. CI uses rustc 1.99.0
`(b940084d7 2026-09-28)` and cargo 1.99.0 `(5f94df478 2026-08-27)` with the
unchanged lockfile; prior local measurements keep their original toolchain.

Windows passes compiler acquisition, Inno Setup 6.7.3 installer compilation,
portable ZIP creation and configured package structure checks. macOS passes
native app/tarball creation, plist identity/version checks, Mach-O/architecture
and linked-library inspection, required resources and archive structure.
Linux passes installed build/runtime prerequisites, tarball creation, ELF,
desktop entry, resources/archive and linked-library checks, plus the configured
20-second Xvfb launch (expected timeout 124 and created schema-v5 database).
All three manifest generation and artifact-upload steps pass. Conditional
steps for other platforms are skipped as configured; skipped launches are
not reported as validation.

### Downloaded artifacts and package identities

All three downloaded GitHub artifact ZIPs match their API/upload digests:

| Artifact | ID | SHA-256 of GitHub artifact ZIP |
| --- | --- | --- |
| `bingee-Windows-X64` | 11252796850 | `f6013c7e4ddc13e138d2b9de737971e160594736c67aebad7dda992118910c25` |
| `bingee-macOS-ARM64` | 11252323135 | `d134b251265586a06ab470c30a66bde2ea0fdb4c28920134ff8a1748b2b3afb0` |
| `bingee-Linux-X64` | 11251763409 | `ee4caa82b317b26bff8248be4b26c15b107312518fd805026115914de10a038d` |

The actual contained `PACKAGE-MANIFEST.json` files identify corrected
`3ab08c32a2fef16cf8555e9db467f681c1dbd1a2`, application `0.1.0-rc.1`, the
proper platform/architecture and rustc 1.99.0. Every package's measured size
and SHA-256 matches its manifest and package-creation log:

| Package | Bytes | SHA-256 |
| --- | ---: | --- |
| `bingee-desktop-windows-x64-setup.exe` | 10038957 | `95e4892f582e364415b46222a4f41c2db3b374f90f262ec7a922221bc0463f78` |
| `bingee-desktop-windows-x64.zip` | 13738956 | `b49b712f723a25112c4ac7052b959c6a8436c2c3cd0b29fd9625c52d8128a8d2` |
| `bingee-desktop-macos-arm64.tar.gz` | 11710443 | `087a8b92a23e88c0c8cc30956ce497e6848bbc673fad8184fb5e07bcd590736b` |
| `bingee-desktop-linux-x86_64.tar.gz` | 16900521 | `d875f029d0388b45dcae844f28c80f418a3edc191b72671af2ff4bc8d4a47495` |

Independent stdlib archive inspection passes all 696 Windows portable file
checksums, PE32+ x64 GUI format and clean-source README identity; macOS
Mach-O arm64, required resources and plist (`0.1.0`, bundle version `10002`);
Linux ELF64 x86_64 executable mode, resources and install/uninstall scripts.
Exact manifests and inspection results are saved beside the step/log records.
Downloaded binaries remain under ignored `target/r17-ci-closure-20261002/`.

The new CI Windows executable SHA-256 is
`834880d5196e117072c70c46c581ef2b6352f5725e74fc8b49f7abfcd8ba16fb`.
It differs from the earlier locally compiled/native-tested executable; this
continuation does not transfer exact-binary interactive or performance claims
to the Rust 1.99.0 build.

### Reused evidence and unperformed validation

No expensive unchanged local R17 exercise was rerun. Matching source inputs
support reuse of the original scoped migrations, Backup V1 compatibility,
complete product journey, failure matrix, soak/resource measurements, native
populated close, native keyboard validation, and local Windows package/smoke.
Earlier source/binary/toolchain and fake-provider boundaries remain explicit.
Frozen R4 DB/poster hashes remain the recorded values above; no frozen input
or dependency file changed.

The corrected workflows configure no Windows package runtime launch or macOS
native process launch. Neither downloaded binary was launched here. Linux
Xvfb smoke passes as CI process/database sanity; it is not interactive GUI
validation or a graceful-close test. No new interactive GUI session on any
platform, screen-reader output, OS monitor-DPI change, live credentialed TMDB
exercise, clean-PC prerequisite test, or exact-CI-binary performance
measurement occurred. These limits do not become new defects or expand R17.

### Commands, exceptions, final state and stop

Git status/revision/log/diff reads, 53-input SHA-256 comparison, connector
GETs for branch/runs/jobs/logs/artifacts, all final archive inspections and
`git diff --check` succeed. The final documentation/evidence check records
all six jobs, configured steps, three artifact digests and four package
manifest records as PASS.

Read-only helper failures are retained here: `git ls-remote origin
refs/heads/main` exits 128 because sandbox network access cannot connect;
the connector verifies live main instead. An accidental `gh --version`
availability probe before reading the deny rule exits 1 because gh is absent;
no gh operation executes and all subsequent GitHub reads use the connector.
Reading `.claude/commands/check-rust.md` exits 1 because the actual instruction
is `.claude/skills/check-rust/SKILL.md`, which was then read. Initial artifact
download exits 1 with a forbidden socket; permitted read-only download retries
complete with exit 0. The first Windows inspection exits 1 on digest assertion
because transfer is still running; after download completion, the same
inspection exits 0 and matches the API digest. These are inspection/environment
errors, not CI or application failures. No commit, push, tag, branch switch,
dependency change or release publication occurs.

Files modified by this continuation: `docs/release-checklist.md`, this report,
`IMPLEMENTATION_PLAN.md` and the canonical `docs/known-limitations.md` to remove
its superseded technical blocker. Raw source/run/job/log/artifact/manifest
inspection evidence is created in `r17-ci-closure-20261002/`. Application
behavior is unchanged. Branch stays `main` at the validated SHA; only these
documentation/evidence changes remain uncommitted. No assumption of public
distribution authorization or broader GUI proof is made.

**TECHNICALLY READY FOR RC.** Corrected-source CI/artifacts close the remaining
technical blocker. No new project-owned failure is found.

**PUBLIC DISTRIBUTION BLOCKED BY EXTERNAL DECISION.** Application/Slint license
basis and intended use, applicable TMDB/Inno terms, signing/notarization and
distribution policy remain unresolved under the release checklist.

Recommended next step is the owner's external distribution decisions. R18
remains suspended and was not started. This continuation stops at the final
R17 verdict; no release is published.
