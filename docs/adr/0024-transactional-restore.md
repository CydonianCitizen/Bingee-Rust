# ADR-0024: Transactional replace restore

- Status: Accepted
- Date: 2026-09-29

## Context

A restore must replace Bingee durable state without leaving a partial import.
Windows does not permit reliable replacement of an open SQLite file. The app
uses one open connection shared by its pages.

## Decision

Parse and validate the complete V1 backup before opening a write transaction.
Delete and import all durable rows inside one SQLite `IMMEDIATE` transaction.
Before a confirmed restore, export current portable state beside the live
database as `bingee-pre-restore-<time>-<unique>.json`. If that export fails,
do not replace anything. The safety file remains until the user removes it.
Run foreign-key, integrity, and row-count checks before commit. Any failure
rolls back; SQLite's atomic commit protects the live file from a partial
restore, including interruption. Reuse the open connection and reload pages
after success. Restore means replace, never merge.

## Alternatives considered

- Temporary database then file swap is awkward with an open connection on
  Windows and needs a multi-file recovery protocol.
- Row-by-row transactions can leave a partially restored database.

## Consequences

Large imports hold the write lock for one transaction. The UI shows a clear
second confirmation before restore and reports the safety file location.
No SQLite file is exposed as backup.

## Validation

Tests force malformed input and import errors and compare the original state
after failure. `PRAGMA foreign_key_check` and `integrity_check` run before
commit.

## Revisit trigger

Measured import duration or concurrent access requires an isolated build and
swap protocol.
