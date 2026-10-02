# R18 RC stabilization checkpoint — 1 October 2026

**NOT TECHNICALLY READY FOR RC.** Product polish has not started. Path A's
explicit stop condition applies: the local keyboard correction requires a new
pushed native CI run, and this continuation is prohibited from commit/push.
The correction, complete local gate results and required next validation are
saved. No public release or R19 work occurred.

## 1. Starting R17 verdict

The most recent report at task steering was
`2026-09-30-r17-gate-status.md`: **NOT STARTED**, technical NOT READY and public
distribution NOT READY. There was no completed successful R17 result to use
for Path B. The preceding R16 validation was still running when this R18
request arrived. It subsequently completed genuinely: both workflows and all
three platform jobs passed on `6e24b2a`, and downloaded artifact inspections
plus required Windows/Linux process smoke passed. See the new R16 report.
R17 then began as the inherited prerequisite and found the keyboard blocker.

## 2. Inherited technical blockers

The eight requested categories are recorded in
`2026-10-01-r17-technical-validation.md`: data integrity, runtime stability,
memory/resources, performance, packaging, cross-platform, UI/accessibility,
and security/privacy. Most are outstanding final acceptance evidence, not
observed application faults. The previous macOS alias and Linux loader
failures are now fixed and natively verified; they are removed from current
blocker lists without rewriting historical reports.

## 3. Blocker corrected locally

The sidebar `NavItem` offered a pointer click and screen-reader default action,
but no keyboard focus target. Two headless regressions, using actual Slint
Tab and Enter/Space events, both failed against the pushed UI. The narrow
component fix adds forwarded focus, a visible accent border, and Enter/Space
activation of its existing callback. Both tests pass after the correction.
No database/network logic, new feature, dependency or architecture changed.

## 4. UI consistency

Only the proven missing sidebar keyboard focus indication was changed.
Spacing, page hierarchy, button sizing and product layout were not redesigned.
The general twelve-page polish audit is BLOCKED by the technical prerequisite.

## 5. Navigation and focus

Keyboard-only access to sidebar Home now passes both regression cases. The
same shared component serves every sidebar destination; no page callback was
replaced. Full Tab order, every destination, focus traps and cross-page media/
season/episode selection remain final audit items. No mouse-driven test was
used as evidence for these two keyboard cases. Actual OS keyboard and
screen-reader interaction were not available or claimed.

## 6. Error, empty and loading states

No copy or asynchronous-state behavior was changed. The comprehensive error,
empty/loading/refresh audit is BLOCKED before product polish. Existing tests
continue to cover cached-state preservation and safe failure behavior, but
they are not relabelled as a newly completed full-product failure matrix.

## 7. First-run experience

No onboarding system or first-run product change was introduced. Fresh empty
profile package smoke passed within its process/file scope. A general
first-run clarity audit awaits the technical gate.

## 8. Settings and About

No control group, attribution or diagnostics was removed. No unresolved
license/business claim was added. Full Settings/About polish is BLOCKED.

## 9. Release metadata and naming

Application name remains `Bingee Desktop`; executable/technical ID remains
`bingee-desktop`. Cargo version remains the single application source:
`0.1.0-rc.1`. All three pushed artifact manifests match it and the exact SHA.
Windows installer ProductVersion matches; macOS short version is `0.1.0`,
numeric bundle version `10002`, and bundle ID is unchanged. No arbitrary
version bump or package filename change was made.

## 10. Backup/restore UX

The existing Settings copy explains that backup includes Library, tracking,
ratings and history and excludes token, images and logs. It was inspected,
not rewritten. The full backup UX/failure/compatibility campaign is pending.

## 11. Restore confirmation and diagnostics

The existing separate “Confirm replace with selected backup” action remains
connected to the restore-confirm callback; no extra dialog was added.
Database/cache/log paths, schema, storage checks and application version remain
available. No credentials were exposed. This source inspection is not a
complete live restore or diagnostics UX audit.

## 12. Rendering

The two regression cases draw the actual AppWindow at 1280×800 using Slint's
software renderer. They prove event-driven navigation, not a visual-layout
audit. No manual image inspection, 1700×1100/1920×1080 final-page review,
high-DPI assessment or live smoothness verification is claimed. Those polish
items remain BLOCKED. The detail-poster limitation was not reopened without
evidence.

## 13. Performance

No optimization or architectural change was attempted. No formal final
startup, page, season or tracking timings are claimed. R4 frozen inputs were
verified unchanged. Final R17 measurements and post-polish spot-checks remain
BLOCKED; historical timing data was not used to invent a regression verdict.

## 14. Documentation and changelog

Updated README, implementation plan, release checklist, current R16 milestone
status and release-engineering licensing evidence. Added the canonical
`docs/known-limitations.md`, with only ACCEPTED FOR RC, POST-RC, EXTERNAL
DECISION and RELEASE BLOCKER classifications. Historical run reports remain
unchanged. The new R16/R17/R18 records distinguish pushed evidence from local
correction. CHANGELOG and broader installation/privacy/backup copy polish are
BLOCKED with the Product Polish phase; they were not presented as completed.

## 15. Correctness gates

All requested commands passed for the revised local source:

| Command | Result |
| --- | --- |
| `cargo fmt --check` | PASS |
| `cargo check --locked` | PASS |
| `cargo test --locked` | PASS: 216 passed, 13 ignored |
| `cargo test --all-features --locked` | PASS: 216 passed, 13 ignored |
| `cargo test --features benchmark-fixture --locked` | PASS: 216 passed, 13 ignored |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS |
| `cargo clippy --all-targets -- -D warnings` | PASS |
| `cargo clippy --all-targets --features benchmark-fixture -- -D warnings` | PASS |
| `cargo build --release --locked` | PASS |

Focused before/after proof is in `r18-stabilization-20261001/keyboard-regression.txt`.
The evidence directory also contains local gate results, per-source candidate
hashes and frozen R4 checks: database
`61918DB3B71A4B0F77154ABB498268F8D7F327469355B569AAD993B58C69FEC3`,
poster manifest `4BE13BC484B971585C04CA4322629CECC837DB3BB823715FE72F69C1AFD0978C`,
all 100 poster bytes matching the manifest. `Cargo.lock` is unchanged.

## 16. Packaging smoke

Pushed `6e24b2a`: Windows, macOS and Linux native build/package/manifest
verification PASS. Windows downloaded portable/installer smoke PASS; Linux
20-second Xvfb smoke PASS. macOS native process and all interactive GUI checks
were explicitly NOT PERFORMED. This evidence does not validate the local UI
correction. The corrected Windows portable ZIP and installer were also built
locally, with README provenance explicitly marked “plus uncommitted changes”.
Local candidate portable and installer smoke both exited 0: fresh/reopen,
live lock contention, idle hard kill/restart, corrupt-file preservation,
fixture refusal, install/reinstall/uninstall and retained empty v5 profile.
All 696 portable checksums matched. These results are recorded separately in
the evidence directory; they do not prove a realistic nonempty upgrade.
ZIP warnings about pre-1980 license-file timestamps are metadata normalization,
not build failures; packaging exited 0. No package engineering was redesigned.

## 17. Remaining technical blockers and required revalidation

The local correction still requires a user commit/push followed by both native
workflows on the new SHA, per-platform step review, new artifact byte/manifest/
resource/version checks and required Windows/Linux process smoke. Then complete
the outstanding formal R17 data, failure, resource/performance, rendering,
keyboard/accessibility and security/privacy acceptance matrix. Only when those
technical blockers close may R18 Product Polish proceed. No unchanged old
workflow run can certify the revised source. No Git write or permission bypass
was attempted in this continuation. Main remains at the pushed base with local
source/documentation modifications; generated packages/scratch logs stay ignored.

## 18. External release decisions

Application/Slint distribution license basis, TMDB intended use/agreement,
Inno Setup commercial-license decision if applicable, macOS signing and
notarization, and Windows signing/distribution policy remain external decisions.
No intent, purchase, agreement or public distribution authorization is inferred.
Those decisions are independent of the technical keyboard defect and missing
validation. No accepted/post-RC issue was unnecessarily expanded.

## 19. Final verdict

**NOT TECHNICALLY READY FOR RC**

**PUBLIC DISTRIBUTION NOT READY**

This is a stabilization checkpoint with a ready local correction and explicit
revalidation boundary, not a completed polish milestone or public release.
All running Cargo and Bingee checks finished, source/lockfile hashes remained
identical to the tested candidate, and evidence JSON parsed successfully.
The user's later shutdown instruction is handled only after this checkpoint
and evidence are saved; application closure is not forced.
