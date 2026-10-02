# R17 technical validation checkpoint — 1 October 2026

**NOT TECHNICALLY READY FOR RC.** R17 began only after genuine R16 PASS for
`6e24b2a`. It is partial, not a completed final RC campaign. The latest R18
instruction takes Path A and requires stopping before product polish when a
project-owned correction needs a new pushed native CI run that this session
cannot perform. That condition now applies.

## Candidate identity

Base HEAD/origin/main/GitHub main:
`6e24b2a109c8fa041075adcbcf1001bc4646cd82`.
Application `0.1.0-rc.1`, schema v5, Backup format V1.
Cargo.lock SHA-256:
`677DE18AACBAAACB0A11DCC60054CCE2338E964D84030943808DF2ED5459CC79`.
The revised local candidate has exactly one production UI correction in
`ui/app-window.slint` and two regression cases in `src/main.rs`; it is not
the CI-tested source. Per-source hashes are retained in the R18 evidence
directory's `candidate-identity.json`. Reports and current documentation also
change locally. No dependencies, lockfile, database schema or backup contract
changed. No product feature or performance framework was introduced.

## Observed blocker and minimal correction

`NavItem` had pointer and accessibility default actions but no keyboard focus
target. Actual synthesized Tab plus Enter/Space on the headless AppWindow
could not reach Home. Both new tests failed against the pushed production UI:
0 passed, 2 failed. This violates the required keyboard-only major-page gate.

The shared component now forwards focus to a FocusScope, shows an accent
focus border and invokes the existing navigation callback on Enter or Space.
Both focused cases then passed: 2 passed, 227 filtered out. Existing mouse
and accessibility actions remain connected to the same callback. This is
Slint event delivery on an offscreen window, not actual OS keyboard or
screen-reader proof. General accessibility and every cross-page media/season/
episode navigation path still require final audit.

## Inherited technical evidence gaps

The previous R17 record was NOT STARTED because R16 failed. Those R16 failures
are now closed with actual native evidence. Remaining items below are missing
acceptance evidence, not newly observed defects or performance regressions.

| Category | Remaining RELEASE BLOCKER |
| --- | --- |
| Data integrity | Final complete fresh/v1–v5 preservation/reopen/foreign-key/rollback matrix; early/current/additive Backup V1; full fake-TMDB tracking/refresh/backup/restore/offline journey |
| Runtime stability | Active metadata-refresh and backup-export crash/restart cases, final failure matrix and realistic nonempty upgrade; idle kill/lock/empty-profile reinstall already pass within R16 scope |
| Memory/resources | Release full-product multi-process soak with per-cycle private/working memory, cache ownership, handles and threads; refresh/search/poster/backup resource stress |
| Performance | Independent startup distribution, both idle modes, representative pages/writes, large backups and final query-plan evidence |
| Packaging | Locally changed UI candidate needs new Windows, macOS and Linux packages/manifests and required smoke |
| Cross-platform | Both native workflows must run for the new pushed SHA; R16 PASS remains valid only for the original source |
| UI/accessibility | Sidebar bug fixed locally; full keyboard/rendering/high-DPI and semantics audit not complete |
| Security/privacy | Final token/path/untrusted-backup/JSON/temp-file/TLS/outbound-data audit and failure matrix not complete |

No accepted, post-RC or external decision was changed into an observed
technical fault. `docs/known-limitations.md` is the current canonical list.

## Correctness and bounded claims

All nine requested local correctness gates passed on the revised source:
format, locked check, default/all-feature/fixture tests, all three Clippy
configurations with warnings denied, and locked release build. Each full test
configuration reports **216 passed, 13 ignored**. Ignored performance/render
tests were not silently treated as executed. Frozen R4 database hash and the
poster manifest plus every one of 100 poster hashes remain unchanged.

Raw correctness results, source hashes and failing-before/passing-after proof
are in `r18-stabilization-20261001/`. No formal R17 startup, idle CPU, memory,
soak, latency, query-plan, high-DPI or smoothness numbers are claimed. R16
process samples and historical R4/R15 measurements are not relabelled.

## Required revalidation

After a user commit/push containing the local source correction and reports,
inspect `cross-platform.yml` and `release-artifacts.yml` on the exact new SHA.
Confirm all platform jobs, all three manifests/structures/resources/versions,
Windows portable/installer smoke and Linux prerequisite/Xvfb smoke. Keep
macOS structural verification separate from its unperformed native launch.
Then freeze that candidate and execute the remaining R17 matrix above before
R18 product polish. Do not rerun an unchanged old SHA as proof of the fix.

**TECHNICAL RC STATUS: NOT TECHNICALLY READY FOR RC.**

**PUBLIC DISTRIBUTION STATUS: PUBLIC DISTRIBUTION NOT READY.** The technical
campaign is incomplete. Licensing, intended use, signing, notarization and
distribution remain independent external decisions, not the reason for the
keyboard defect or a substitute for technical proof. No release was published.
