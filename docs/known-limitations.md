# Current release limitations

Reviewed 2 October 2026. This is the canonical current list. Historical
milestone and run reports retain their original observations. R16 technical
packaging passed for `6e24b2a`; subsequent local focus/target fixes have not been
validated by native CI. R17 is partial and R18 product polish is suspended.

| Item | Classification | Required action or scope |
| --- | --- | --- |
| Corrected source/tests/reports uncommitted/unpushed; no exact-candidate CI/artifacts | RELEASE BLOCKER | Owner commit/push, both workflows on corrected SHA, individual Windows/macOS/Linux jobs and package/smoke checks. Settings deny agent commit/push. |
| Native Discover result/add/open keyboard path | VERIFIED CONTROLLED-PROVIDER NATIVE SCOPE | Real Windows input in test-only native window: search, Movie/TV arrows, Space/Return add, escape and Library opening passed; fake provider and in-memory credential, no live service/release-executable response claim. Disabled In Library retains focus until Tab; no trap observed. |
| Native semantics/scaling audit has bounded scope | VERIFICATION LIMITATION | Useful UIA roles/names and native 125% Slint scaling checked; custom scope/dialog reports can lag; empty pages may retain region focus until Tab. Screen-reader behavior not directly verified; no OS monitor-DPI change or real downloaded-poster smoke claimed. |
| Prior populated close-request timeout | RESOLVED IN BOUNDED NATIVE SCOPE | Not reproduced on targetable desktop. Six final installed closes, three per refresh mode; code 0, 298–481 ms, byte-identical DB, DB/lock handles released. Prior evidence remains environment/input-delivery inconclusive; no live credentialed-refresh shutdown claim. |
| Deterministic R17 migration, Backup V1, active-write crash and full product journey | VERIFIED LOCAL SCOPE | Final regressions and reused matching-source release evidence pass; details in 2 October R17 report |
| Startup/CPU, private-memory soak, cache/handles/threads and page/write/query plans | MEASURED LOCAL SCOPE | Native responsive-window/idle measurements plus three bounded headless product soaks and timing processes; no live smoothness or indefinite-leak guarantee |
| Final security/privacy/failure matrix | VERIFIED LOCAL SCOPE | Bounded source/fake-server sanity review and final suites pass; native permission prompts and penetration testing not claimed |
| Windows VC++ x64 runtime must be installed separately | ACCEPTED FOR RC | Documented prerequisite; no clean-PC proof is claimed |
| Linux needs desktop runtime libraries and a Secret Service for token storage | ACCEPTED FOR RC | Documented prerequisites; offline startup survives unavailable secure storage without plaintext fallback |
| macOS R16 executable verification is structural | ACCEPTED FOR RC | Meets the configured R16 gate; native process and interactive claims are absent |
| Historical R4 interactive evidence is incomplete | ACCEPTED FOR RC | Preserve it as historical; final R17 uses its own identified readiness proxy |
| Native OS notifications are absent | ACCEPTED FOR RC | In-app Updates is the shipped scope |
| Release events have no database expiry | ACCEPTED FOR RC | Updates display is bounded; do not add speculative retention work |
| Live TMDB/service smoke beyond deterministic fake-server coverage | POST-RC | Does not waive the required final deterministic R17 journey or failure tests |
| Application license, Slint license basis and intended distribution | EXTERNAL DECISION | Owner decision; no commercial/non-commercial intent inferred |
| TMDB intended-use/licensing agreement | EXTERNAL DECISION | Owner decision and applicable agreement |
| Inno Setup commercial-license decision, if applicable | EXTERNAL DECISION | Owner decision; no purchase or applicability assumed |
| macOS signing/notarization | EXTERNAL DECISION | Public distribution policy; unsigned artifacts are permitted for technical RC testing |
| Windows signing and distribution policy | EXTERNAL DECISION | Owner decision; unsigned artifacts are permitted for technical RC testing |

The former macOS path-alias and missing Linux X11 library failures are fixed
and verified in native CI. They are not current blockers. The sidebar's
missing keyboard focus and native episode/Updates gaps are fixed locally
with regression/native proof. Episode targets now load the requested title
before selecting season/episode. Final suites pass 221 ordinary tests,
16 ignored (one new manual native fixture separately passed). Native follow-up supersedes blanket native-unavailable and
unresolved-close claims, preserving historical evidence. Exact corrected
CI/artifacts alone still block the technical gate.
No accepted or post-RC item was promoted to
an observed defect without evidence.
