# Bingee Desktop release checklist

Target: `0.1.0-rc.1`. R17 passed for pushed
`3ab08c32a2fef16cf8555e9db467f681c1dbd1a2`. R18 is authorized bounded polish
on that base, with local changes and its own regression report. The prior R17
CI/artifact evidence below remains scoped to that exact commit; it does not
certify the unpushed R18 source. No release or tag has been published.

## Technical RC — R18 regression guard

**TECHNICAL RC PRESERVED** within the local regression scope.

| Check | Result |
| --- | --- |
| Format, locked check, three test suites, three Clippy configurations and locked release build | PASS — all nine gates |
| Each ordinary test configuration | PASS — 222 passed, 17 ignored; one added keyboard regression and one added ignored render test |
| Navigation and keyboard regression coverage | PASS in the ordinary suites |
| Off-screen layouts at all three requested sizes | PASS — 57 renders inspected; no interactive validation claim |
| Final Windows package, portable and installer smoke | PASS — exact final hashes in the report |
| Frozen R4 hashes, poster contract and dependency files | PASS — unchanged |

Current results are recorded in [the R18 report](run-reports/2026-10-03-r18-rc-polish.md).
The final local gate table and package evidence appear there. Cross-platform
publication must use a newly pushed exact source and newly inspected native
workflow artifacts; this is a provenance requirement, not a reopened R17 defect.

## Public distribution

| Decision | Status |
| --- | --- |
| Application license, Slint basis, intended TMDB use and applicable Inno Setup purchase | BLOCKED — licensing decision |
| Final dependency/license inventory and distribution channel | BLOCKED — owner distribution decision |
| Windows code signing policy | BLOCKED — Windows signing decision |
| macOS code signing and notarization policy | BLOCKED — macOS signing/notarization decision |

See [licensing matrix](release-licensing.md), [signing insertion points](release-signing.md),
[installation](installation.md) and [draft release notes](release-notes-0.1.0-rc.1.md).
Technical PASS never grants public-distribution authorization.

## Preserved R17 technical evidence
Latest R17 evidence: `docs/run-reports/2026-10-02-r17-native-blocker-followup.md`; prior scoped evidence remains in `docs/run-reports/2026-10-02-r17-release-validation.md`. Historical checkpoints are retained. Current limitations are in `docs/known-limitations.md`.

| Category | Check | Status |
| --- | --- | --- |
| Source | Corrected focus/target fixes, regression tests and R17 documents present in pushed main; clean at validation entry | PASS: `3ab08c3`, all 53 recorded source inputs match |
| CI | Windows normal build/tests/release for corrected `3ab08c3` | PASS |
| CI | macOS normal build/tests/release; alias assertion resolved | PASS |
| CI | Linux normal build/tests/release | PASS |
| Packages | Windows installer/ZIP, PE/resources/version, manifest and all portable checksums | PASS |
| Runtime | Recorded Windows package and installer process/file smoke | PASS within existing source/binary scopes; corrected CI workflow configures structure checks, not a Windows runtime launch |
| Packages | macOS native app, Mach-O arm64, plist/resources/version/archive/manifest | PASS |
| Runtime | macOS native process smoke required by current R16 workflow | NOT APPLICABLE: structural checks configured; launch NOT PERFORMED |
| Packages | Linux ELF/resources/desktop entry/archive/manifest | PASS |
| Runtime | Linux prerequisite installation and 20-second Xvfb package launch | PASS |
| Runtime | R16 live interactive GUI validation | NOT APPLICABLE to configured gate: NOT PERFORMED; no smoothness claim |
| Packages | Signing/notarization for technical RC testing | NOT APPLICABLE: unsigned artifacts permitted |
| Correctness | Final local candidate: all three test configurations | PASS: 221 passed, 16 ignored each; three ordinary regressions explain 218 → 221; one added manual native fixture explains 15 → 16 ignored and separately passed |
| Correctness | Local format/check and three Clippy configurations | PASS |
| Correctness | Local corrected candidate release build | PASS |
| Keyboard | Sidebar and local deeper paths | PASS within named native scopes; final affected paths retested. Discover search/result/add and Library opening passed in a native controlled-provider test window. |
| Packages | Local revised Windows package and all 696 portable checksums | PASS: explicit uncommitted-source provenance |
| Runtime | Local revised Windows portable smoke | PASS: isolated empty v5 profile, live contention, idle kill/restart and corrupt-file preservation |
| Runtime | Installed R16-to-current nonempty upgrade/uninstall | PASS within recorded scope: 1,000-title v5 DB byte-identical; controlled termination, not graceful-close proof |
| Runtime | Populated native graceful close/restart | PASS bounded native scope: six final installed repetitions, three per refresh mode; code 0, 298–481 ms, byte-identical DB, released DB/lock handles; no live credentialed refresh claim |
| CI | Corrected candidate native workflows and new package/smoke verification | PASS: exact `3ab08c3`; all six individual jobs and configured steps inspected; three downloaded artifact digests and four package manifest size/hash records verified |
| Migrations | Fresh/v1–v5 chain, preserved state, reopen, foreign keys and rollback matrix | PASS: actual-file regressions in final three suites; failure steps 2–5 |
| Backup | Early/current/additive V1, large fixture and semantic restart journey | PASS: compatibility regressions, three release timing processes, new complete Settings restore/offline journey |
| Runtime | Active-refresh/export crash recovery | PASS: observed active writes before process kill; original state, integrity/FK and lock reacquisition; no power-loss claim |
| Performance | Startup, both idle modes, pages/writes/query plans | Prior measurements retain scope; final empty/populated native usable-startup observations pass separately; no new first-paint timing benchmark |
| Resources | Full-product release soak, private memory, cache/handles/threads and refresh stress | PASS within bounded headless scope: three 100-cycle processes; native idle sampled separately; no indefinite-leak or native interaction-footprint claim |
| Accessibility | Native semantics, focus and scaling | PASS bounded sanity scope: useful roles/names, focus and native 125% Slint scaling checked, including controlled-provider Discover. Screen-reader behavior not directly verified; no OS monitor-DPI/full audit claim; transient disabled-add focus deviation recorded |
| Security/privacy | Failure matrix and credential/path/backup/JSON/outbound audit | PASS within bounded source/fake-server sanity scope; no penetration-test/native permission-prompt claim |
| Dependencies | Runtime/build/dev direct/transitive version/license inventory | PASS: unchanged lockfile, 653 all-target versions, 388 Windows packages, 325 linked crates; distribution-license choice remains external |
| Licensing | Existing Slint/TMDB attribution implementation | PASS |
| Distribution | Owner decisions | Tracked separately in Public distribution above |
| R18 | Bounded UX/release polish after R17 PASS | See current R18 regression report above |
| Documentation | Current R16 evidence, R17/R18 blocker records and canonical limitations | PASS |

## Corrected-source CI and artifact closure

Both runs identify `3ab08c32a2fef16cf8555e9db467f681c1dbd1a2`, attempt 1:

| Platform | cross-platform run 37064096145 | release-artifacts run 37064096196 | Uploaded artifact |
| --- | --- | --- | --- |
| Windows x64 | PASS, job 111027502242 | PASS, job 111027502382 | `bingee-Windows-X64`, ID 11252796850 |
| macOS arm64 | PASS, job 111027501990 | PASS, job 111027502520 | `bingee-macOS-ARM64`, ID 11252323135 |
| Linux x64 | PASS, job 111027502357 | PASS, job 111027502479 | `bingee-Linux-X64`, ID 11251763409 |

Runs: [cross-platform](https://github.com/CydonianCitizen/Bingee-Rust/actions/runs/37064096145),
[release-artifacts](https://github.com/CydonianCitizen/Bingee-Rust/actions/runs/37064096196).
All configured format/check/test/Clippy/release steps passed. Each CI test
invocation reports 221 passed, 0 failed, 16 ignored. Packaging passed Inno
compiler acquisition/Windows installer and ZIP creation, macOS app/tarball
creation and Linux prerequisites/tarball creation; each platform's applicable
structure, manifest and upload steps passed. Linux's configured 20-second
Xvfb launch and schema-v5 creation passed. Other platforms' conditional steps
were skipped as configured, not counted as performed validation.

Downloaded artifact digests, package sizes/SHA-256, manifest source/version,
architecture/resources, macOS plist and all 696 Windows portable checksums
passed independent inspection. Exact identities/hashes are in the latest
R17 report's CI closure and `docs/run-reports/r17-ci-closure-20261002/`.
CI used rustc/cargo 1.99.0 and unchanged Cargo.lock. Success at `6e24b2a` is
historical evidence only; it is not used to certify this corrected candidate.

Windows runtime smoke/native keyboard/close evidence is reused within its
recorded local source/binary scope. The new CI Windows executable was not
launched here; its Rust 1.99.0 build has a different hash. macOS native
process launch and interactive GUI validation remain unperformed. Linux
Xvfb smoke is not interactive GUI validation. No new interactive session,
screen-reader or actual OS monitor-DPI validation occurred in this continuation.
The scoped migration, Backup V1, full journey, failure matrix, soak/resource,
native close/keyboard and local Windows package/smoke evidence is retained;
unchanged expensive local validation was not rerun.

**TECHNICAL RC STATUS: TECHNICALLY READY FOR RC.** Exact corrected-source
CI/artifacts close the remaining technical gate. No new project-owned failure
was found. Native evidence and performance measurements retain their original
scopes; no broader GUI or exact-CI-binary performance claim is made.

**PUBLIC DISTRIBUTION STATUS: PUBLIC DISTRIBUTION BLOCKED BY EXTERNAL DECISION.**
Application/Slint licensing and intended use, applicable TMDB/Inno decisions,
signing/notarization and distribution policy remain unresolved. No intent or
public distribution authorization is inferred. No release was published.
R18 now proceeds under its separate user authorization. R19 and release publication remain out of scope.
