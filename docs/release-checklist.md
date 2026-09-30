# Bingee Desktop release checklist

Target: `0.1.0-rc.1`. Review date: 1 October 2026. GitHub main, HEAD and origin/main remain `ceb10794eccc356268b402ff9a6cadf1844848b5`; local R16 fixes await a user commit/push. Current evidence: `docs/run-reports/2026-10-01-r16-revalidation.md` and its job/step JSON. Detailed artifact and local-test evidence remains in `docs/run-reports/2026-09-30-r16-ci-closure.md` and its raw directory; R17 status remains in `docs/run-reports/2026-09-30-r17-gate-status.md`. R16 is PARTIAL; R17 is NOT STARTED. `BLOCKED` means the item has not passed; it is not an implicit waiver.

| Category | Check | Status |
| --- | --- | --- |
| Code | R15 local correctness and data hygiene | PASS |
| Tests | 214 Rust tests passed, 13 ignored on Windows; format and check pass | PASS |
| Tests | Normal cross-platform build and tests for pushed `ceb1079` | BLOCKED: Windows/Linux pass; macOS path-alias test fails; local fix awaiting native CI |
| Tests | Revised source and workflow native CI revalidation | BLOCKED: fixes cannot be committed/pushed by this session |
| Migrations | Full fresh/v1–v5 migration chain under R17 | BLOCKED: R17 has not started |
| Backup | R15 export safety and existing restore regression | PASS |
| Backup | R17 early/current V1 and large fixture checks | BLOCKED: R17 has not started |
| Packages | Windows native installer/ZIP, downloaded hashes/manifest/resources/version | PASS for exact pushed CI artifacts |
| Runtime | Windows downloaded-artifact and local portable/installer process smoke | PASS on this host; no interactive/clean-PC/nonempty-upgrade claim |
| Packages | macOS native `.app`, structure, resources, version and manifest | BLOCKED: test failed before bundle creation; local fix pending CI |
| Runtime | macOS packaged launch | BLOCKED: no bundle created |
| Packages | Linux native tarball, ELF, resources, desktop entry and archive structure | BLOCKED: tarball built and structure checks reached launch; final manifest/upload skipped after smoke failure |
| Runtime | Linux packaged Xvfb smoke | BLOCKED: missing libxkbcommon-x11; CI prerequisite fix pending native revalidation |
| Packages | Signing and notarization for technical RC | NOT APPLICABLE: technical gate permits unsigned artifacts |
| Licensing | Slint/TMDB About attribution implementation | PASS |
| Licensing | Application license and intended-use decisions; applicable agreements | BLOCKED: external decision |
| Privacy | Final outbound-data audit | BLOCKED: R17 has not started |
| Security | Final token, path, backup and JSON audit | BLOCKED: R17 has not started |
| Performance | Formal release startup, idle CPU, memory, soak and page timings | BLOCKED: R17 has not started |
| Accessibility | Final keyboard, focus, roles and names audit | BLOCKED: R17 has not started |
| Documentation | R16 actual CI results, fixes and separate licensing matrix | PASS for documentation; R16 technical gate remains BLOCKED |
| Documentation | R17 gate-status report | PASS: saved and rechecked; execution remains NOT STARTED because R16 gate failed |
| Documentation | Final R17 evidence and known-limitations acceptance | BLOCKED: R17 has not started |

## Known limitations

| Item | Classification |
| --- | --- |
| Windows artifacts unsigned | ACCEPTED FOR RC testing; public distribution decision pending |
| macOS signing/notarization absent | ACCEPTED FOR RC technical validation; Gatekeeper limits distribution |
| Microsoft VC++ runtime not bundled | ACCEPTED FOR RC with documented prerequisite; clean-PC smoke pending |
| Live TMDB credential smoke unavailable | POST-RC if fake-server and error-path R17 checks pass |
| R4 interactive measurements incomplete | ACCEPTED FOR RC as historical gap; R17 requires fresh final-product measurements |
| Native OS notifications absent | ACCEPTED FOR RC; local Updates page is shipped scope |
| Release events retained indefinitely | ACCEPTED FOR RC; Updates display is bounded |
| Application license and intended-use choice | EXTERNAL DECISION; PUBLIC-RELEASE BLOCKER |
| macOS directory-alias test failure prevents bundle; local assertion fix unpushed | RELEASE BLOCKER for technical R16 gate |
| Linux X11 smoke failure; runtime dependency fix and final manifest unvalidated | RELEASE BLOCKER for technical R16 gate |
| Revised local source not revalidated in native CI | RELEASE BLOCKER for technical R16 gate |
| R17 final validation not run | RELEASE BLOCKER for technical RC |

**TECHNICAL RC STATUS: NOT TECHNICALLY READY FOR RC.** Windows passes independently; macOS and
Linux fixes require native revalidation, then R17 must execute.

**PUBLIC DISTRIBUTION STATUS: PUBLIC DISTRIBUTION NOT READY.** Unresolved licensing, signing,
notarization and distribution decisions remain separate from the technical
gate. No commercial/non-commercial intent or distribution authorization is
assumed. No release has been published by this continuation.
