# Current release limitations

Reviewed 3 October 2026. This is the canonical current list. Historical
milestone and run reports retain their original records. R17 is technically
ready for RC at `3ab08c32a2fef16cf8555e9db467f681c1dbd1a2`; all six native CI
jobs and downloaded artifacts passed. R18 preserves the technical RC within its local regression scope; its
regression evidence is recorded separately in
[the R18 report](run-reports/2026-10-03-r18-rc-polish.md).

| Item | Classification | Scope or next action |
| --- | --- | --- |
| Windows Visual C++ x64 runtime is a separate prerequisite | ACCEPTED FOR RC | Documented installation prerequisite; no clean-PC proof claimed. |
| Linux runtime libraries and Secret Service | ACCEPTED FOR RC | Required desktop libraries are documented. Missing secure storage has no plaintext fallback. |
| macOS CI verification is structural | ACCEPTED FOR RC | Native process/interactive validation is unperformed; no signing or notarization claim. |
| Validated CI architectures are Windows x64, macOS arm64 and Linux x64 | ACCEPTED FOR RC | Other native architecture script branches need separate validation before being advertised as tested. |
| R18 has local validation only until its source is pushed | ACCEPTED FOR RC | R17 CI remains valid for its exact SHA. Before distributing new R18 artifacts, run both workflows on the new exact SHA and inspect each job/manifest. Push is denied in this workspace; no inherited R18 CI claim. |
| Keyboard/accessibility and scaling evidence is bounded | ACCEPTED FOR RC | Preserve R17 native controlled-provider/125% Slint scope. R18 adds off-screen layout and synthetic keyboard checks, not new interactive certification. Screen-reader and OS monitor-DPI behavior are not fully certified. |
| Historical R4 interactive evidence is incomplete | ACCEPTED FOR RC | Frozen evidence and hashes remain historical; no cross-stack superiority claim. |
| Refresh operates only while the app is open; Updates are in-app | ACCEPTED FOR RC | No background service or native OS notifications. |
| History displays the most recent 200 watches | ACCEPTED FOR RC | Older saved events remain in data/backups and relevant statistics. |
| Home, progress and Calendar depend on fetched metadata | ACCEPTED FOR RC | Incomplete episode coverage is shown; no prediction of unknown dates. |
| Restore reloads all pages after restart | ACCEPTED FOR RC | UI explicitly requests restart after successful persistence. |
| Release events have no database expiry | ACCEPTED FOR RC | Display is bounded; retention policy deferred. |
| Archives and stable CI runners are not byte-for-byte reproducible | ACCEPTED FOR RC | Locked dependencies and manifests identify source/toolchain/final bytes; timestamps/toolchain updates can change package hashes. |
| Live TMDB/service smoke beyond controlled-provider coverage | POST-RC | Existing deterministic journey and failure tests remain the release evidence. |
| Application license, Slint basis and intended distribution | EXTERNAL DECISION | Owner chooses; no commercial/non-commercial classification inferred. |
| TMDB intended-use terms/agreement | EXTERNAL DECISION | Owner determines applicability and obtains any required agreement. |
| Inno Setup purchase, if applicable | EXTERNAL DECISION | Owner determines applicability; no purchase assumed. |
| Windows signing and distribution policy | EXTERNAL DECISION | Unsigned artifacts allowed for technical testing only under the selected testing policy. |
| macOS signing/notarization and distribution policy | EXTERNAL DECISION | Owner decision and credentials remain absent. |
| Final dependency inventory and distribution channel review | EXTERNAL DECISION | Review final packages against the chosen distribution/license model. |

No current technical RELEASE BLOCKER is asserted. The former macOS path alias,
Linux X11 prerequisite, native populated close, sidebar focus and episode-target
issues are resolved within the scopes recorded in the R17 report. Migration,
Backup V1, product journey, failure matrix and performance/soak evidence remain
there rather than being duplicated as current limitations.

Public distribution remains **BLOCKED BY EXTERNAL DECISION**. A passed technical
RC gate does not authorize publication.
