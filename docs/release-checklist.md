# Bingee Desktop release checklist

Target: `0.1.0-rc.1`. Review date: 30 September 2026. Evidence: `docs/run-reports/2026-09-30-r16-technical-gate.md`. `BLOCKED` means the item has not passed; it is not an implicit waiver.

| Category | Check | Status |
| --- | --- | --- |
| Code | R15 local correctness and data hygiene | PASS |
| Tests | 214 Rust tests passed, 13 ignored on Windows; format and check pass | PASS |
| Tests | Normal cross-platform build and tests for this tree | BLOCKED: tree not pushed |
| Migrations | Full fresh/v1–v5 migration chain under R17 | BLOCKED: R17 has not started |
| Backup | R15 export safety and existing restore regression | PASS |
| Backup | R17 early/current V1 and large fixture checks | BLOCKED: R17 has not started |
| Packages | Windows installer, ZIP, portable and installer smoke | PASS |
| Packages | macOS native `.app`, structure and executable checks | BLOCKED: native CI not run |
| Packages | Linux native tarball, structure and headless smoke | BLOCKED: native CI not run |
| Packages | Signing and notarization for technical RC | NOT APPLICABLE: technical gate permits unsigned artifacts |
| Licensing | Slint/TMDB About attribution implementation | PASS |
| Licensing | Application license and intended-use decisions; applicable agreements | BLOCKED: external decision |
| Privacy | Final outbound-data audit | BLOCKED: R17 has not started |
| Security | Final token, path, backup and JSON audit | BLOCKED: R17 has not started |
| Performance | Formal release startup, idle CPU, memory, soak and page timings | BLOCKED: R17 has not started |
| Accessibility | Final keyboard, focus, roles and names audit | BLOCKED: R17 has not started |
| Documentation | R16 technical gate and licensing matrix | PASS |
| Documentation | R17 evidence and final known-limitations classification | BLOCKED: R17 has not started |

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
| macOS/Linux package and runtime unverified | RELEASE BLOCKER for technical R16 gate |
| R17 final validation not run | RELEASE BLOCKER for technical RC |
