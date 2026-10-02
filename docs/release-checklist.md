# Bingee Desktop release checklist

Target: `0.1.0-rc.1`. Reviewed 2 October 2026. HEAD, local origin/main and live GitHub main: `6e24b2a109c8fa041075adcbcf1001bc4646cd82`. R16 passed for that revision. Local focus/target fixes, tests and reports remain uncommitted/unpushed. R17 is PARTIAL; R18 remains suspended. Status applies only to named source/evidence scopes.

Latest R17 evidence: `docs/run-reports/2026-10-02-r17-native-blocker-followup.md`; prior scoped evidence remains in `docs/run-reports/2026-10-02-r17-release-validation.md`. Historical checkpoints are retained. Current limitations are in `docs/known-limitations.md`.

| Category | Check | Status |
| --- | --- | --- |
| Source | R16 corrections present in pushed main; clean at validation entry | PASS |
| CI | Windows normal build/tests/release for pushed `6e24b2a` | PASS |
| CI | macOS normal build/tests/release; alias assertion resolved | PASS |
| CI | Linux normal build/tests/release | PASS |
| Packages | Windows installer/ZIP, PE/resources/version, manifest and all portable checksums | PASS |
| Runtime | Downloaded Windows package and installer process/file smoke | PASS: this host, isolated empty profile |
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
| CI | Corrected candidate native workflows and new package/smoke verification | BLOCKED: user commit/push required |
| Migrations | Fresh/v1–v5 chain, preserved state, reopen, foreign keys and rollback matrix | PASS: actual-file regressions in final three suites; failure steps 2–5 |
| Backup | Early/current/additive V1, large fixture and semantic restart journey | PASS: compatibility regressions, three release timing processes, new complete Settings restore/offline journey |
| Runtime | Active-refresh/export crash recovery | PASS: observed active writes before process kill; original state, integrity/FK and lock reacquisition; no power-loss claim |
| Performance | Startup, both idle modes, pages/writes/query plans | Prior measurements retain scope; final empty/populated native usable-startup observations pass separately; no new first-paint timing benchmark |
| Resources | Full-product release soak, private memory, cache/handles/threads and refresh stress | PASS within bounded headless scope: three 100-cycle processes; native idle sampled separately; no indefinite-leak or native interaction-footprint claim |
| Accessibility | Native semantics, focus and scaling | PASS bounded sanity scope: useful roles/names, focus and native 125% Slint scaling checked, including controlled-provider Discover. Screen-reader behavior not directly verified; no OS monitor-DPI/full audit claim; transient disabled-add focus deviation recorded |
| Security/privacy | Failure matrix and credential/path/backup/JSON/outbound audit | PASS within bounded source/fake-server sanity scope; no penetration-test/native permission-prompt claim |
| Dependencies | Runtime/build/dev direct/transitive version/license inventory | PASS: unchanged lockfile, 653 all-target versions, 388 Windows packages, 325 linked crates; distribution-license choice remains external |
| Licensing | Existing Slint/TMDB attribution implementation | PASS |
| Distribution | Owner licensing/intended-use, signing/notarization and distribution decisions | BLOCKED: external decisions |
| R18 | Optional stabilization/polish after R17 technical PASS | SUSPENDED: not started; close technical RC gate first |
| Documentation | Current R16 evidence, R17/R18 blocker records and canonical limitations | PASS |

**TECHNICAL RC STATUS: NOT TECHNICALLY READY FOR RC.** The pushed revision's
R16 passes. Corrected source/tests/reports remain uncommitted/unpushed;
exact-candidate CI/artifacts are absent. Native keyboard paths, populated
close/restart and informal
usable startup now pass within recorded scope. Screen-reader and actual OS
monitor-DPI changes are not claimed.

**PUBLIC DISTRIBUTION STATUS: PUBLIC DISTRIBUTION NOT READY.** Technical
validation remains incomplete; unresolved licensing, signing, notarization
and distribution decisions are independently retained. No intent or public
distribution authorization is inferred. No public release or R19 work occurred.
