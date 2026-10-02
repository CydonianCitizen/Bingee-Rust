# R17 native blocker follow-up — 2 October 2026

Technical verdict: **NOT TECHNICALLY READY FOR RC**.

Public distribution verdict: **PUBLIC DISTRIBUTION NOT READY**.

R16 remains PASS. R18 remains SUSPENDED and was not started. This run addresses
only corrected-source identity, native input/rendering/startup and graceful
close. Evidence is in `r17-blocker-followup-20261002/` beside this report.

## Source and CI identity

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

## Remaining work and next step

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
