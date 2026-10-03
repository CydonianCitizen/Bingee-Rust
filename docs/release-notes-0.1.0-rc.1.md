# Bingee Desktop 0.1.0-rc.1 — draft release notes

This is a publication draft, not a published release or distribution authorization.

## What's included

Bingee Desktop keeps movies and TV series together in a personal Library.
Search TMDB, open title details, browse seasons and episodes, mark what you
watched and add personal ratings. History records watches; Statistics summarizes
them. Home provides Continue Watching, Up Next, recent watches and Coming Soon.
Calendar shows locally known episode dates. Updates lists changes found during
metadata refresh, which can run automatically while the app is open.

For a fresh profile, configure your own TMDB API Read Access Token in Settings,
search in Discover, then choose Add to Library. Saved Discover results and
History entries can open their local details directly.

## Offline/local-first behavior

Your Library, tracking, ratings and history stay on this computer. Previously
fetched metadata and cached posters remain available offline. Searching TMDB
and fetching new information require a connection and your own token. Tokens
use system credential storage. There is no cloud sync or telemetry.

## Backup

Settings can export a portable JSON backup. Restore replaces saved Bingee data
after first saving a safety backup; it stops if the safety backup cannot be
created. TMDB credentials, cached images and logs are excluded. Restart after
restore to reload every page. This is Bingee Desktop Backup V1; Android backup
compatibility is not promised.

## Supported desktop platforms

Packages target Windows x64, macOS (native arm64 or x86_64 build) and Linux
(native x86_64 or aarch64 build). The validated R17 CI matrix used Windows x64,
macOS arm64 and Linux x64. Other architecture branches in packaging scripts
do not establish equivalent validation. See [installation](installation.md).

Artifacts are unsigned. macOS artifacts are not notarized. Technical package
validation does not establish platform signing or interactive GUI certification.

## Known limitations

The [current limitations list](known-limitations.md) is canonical. Refresh runs
only while the app is open; Updates are in-app, without OS notifications.
Calendar and series progress depend on the metadata already fetched. History
shows the most recent 200 entries. Live-provider and accessibility validation
retain the bounded scopes recorded in the R17 and R18 reports.

## Distribution/licensing status

Public distribution is blocked by external decisions about the application
license, Slint license basis, intended TMDB use, any applicable Inno Setup
purchase, signing/notarization and the distribution channel. No commercial or
non-commercial classification is assumed. Attribution and dependency notices
are implemented; this is not a conclusion about legal compliance.
See [licensing](release-licensing.md) and [signing readiness](release-signing.md).
