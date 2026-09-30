# ADR-0028: Profile ownership and explicit maintenance

- Status: Accepted
- Date: 2026-09-29

## Context

Two application processes could write one profile. Provider metadata and cached posters also accumulated after Library removal. Backup export depended on hard-link support.

## Decision

The normal application acquires an OS file lock on `data/bingee.lock` before opening SQLite. The handle stays open through the event loop and is released on exit. A second instance shows the startup error page and never opens the database. The file itself is not deleted: a crash releases its OS lock, so a stale file does not block restart. The benchmark fixture uses its separate database and does not acquire the production profile lock.

Settings offers explicit **Check storage** and **Clean unused data** actions. Check storage runs `PRAGMA quick_check` and `foreign_key_check`, and reports poster count, disk bytes, and the 12 MiB decoded RAM budget. Cleanup first checks integrity. It deletes metadata only when no Library membership, media tracking, episode tracking, watch event, or release event references it. It removes only old, unreferenced Bingee poster files and poster temporary files older than one hour. Referenced posters survive regardless of age. Unreferenced posters survive until thirty days after their last disk read. Unknown files and symlinks are never touched. Cache failures are nonfatal. Release events have no database expiry; the Updates query limits display only.

Backup export still prefers a same-directory hard link for atomic publication. If hard links are unavailable, it opens the destination with `create_new`, copies the already synced JSON, then syncs the destination. It never replaces an existing destination. A crash during this fallback can leave a truncated final-name file; V1 JSON parsing and validation reject that file on restore. A normal copy failure removes it. Temporary export files use an identifiable `.bingee-<pid>-<nanoseconds>.tmp` suffix and are removed after normal success or error. Because exports may target arbitrary user directories, Bingee does not scan them and delete old files automatically.

SQLite uses its default DELETE rollback journal, explicit `synchronous=FULL`, `foreign_keys=ON`, and a 5,000 ms busy timeout. No WAL sidecars are expected in normal operation. Migrations and restore remain single durable transactions. Backup reads rows through the open SQLite connection, so an inconsistent raw file copy is never used. Full integrity checks run during restore; quick checks run only on explicit request. The local log rotates at 1 MiB, retaining one previous file. Rotation failure disables file logging for that session and does not stop the app.

## Validation

Tests cover lock contention and stale lock files, orphan preservation rules, poster preservation and cleanup, both backup finalize paths and collisions, interrupted fallback copies, integrity diagnostics, and SQLite pragmas. The existing restore tests validate before mutation and force import failures; data remains unchanged. The 1,000-title, 17,550-episode, 10,000-event headless UI soak runs 100 cycles with resource sampling recorded in the R15–R17 run report.
