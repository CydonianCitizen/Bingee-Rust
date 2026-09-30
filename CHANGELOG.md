# Changelog

## 0.1.0-rc.1 — release candidate preparation (29 September 2026)

- Prevent concurrent Bingee processes from opening the same profile database.
- Add explicit integrity check and conservative metadata/poster cleanup in Settings.
- Make backup export work where hard links are unavailable without overwriting an existing destination.
- Set SQLite FULL synchronous mode and a 5-second busy timeout; bound runtime logs by rotation.
- Add a Windows per-user installer and portable ZIP, macOS bundle and Linux tarball packaging scripts, third-party license material and a manually triggered cross-platform artifact workflow.

This version string is reserved for the first release candidate. Cross-platform package verification, final performance and accessibility checks, and distribution license decisions remain open. No public release is approved yet.
