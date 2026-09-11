# ADR-0002: SQLite via `rusqlite` with bundled SQLite

- Status: Proposed
- Date: 2026-09-11

## Context

Milestone R2 replaces the in-memory R1 library with a small SQLite-backed
query. The spike needs to find out how cleanly SQLite fits the Rust
architecture and how much complexity it adds. It does not need a production
data layer. These constraints apply:

- one user and one process on a local desktop: a single writer, and the UI
  thread is the only reader;
- Windows, macOS, and Linux, with no system packages beyond those Slint already
  needs;
- a small dependency graph, and no async runtime, ORM, migration framework, or
  connection pool (R2 brief §12);
- search must stay equivalent to the R1 in-memory oracle, which uses
  Unicode-aware lowercasing (`str::to_lowercase`).

## Decision

Use **`rusqlite`** (0.40) with the **`bundled`** feature, which compiles the
SQLite amalgamation into the binary. One `rusqlite::Connection` is opened in
`main` and owned by the library view. Queries are plain parameterized SQL over
one small `media` table.

## Alternatives considered

- **`sqlx` (SQLite driver)**: async-first, so it needs an async runtime (R2
  forbids Tokio without evidence). Its compile-time query checks need a database
  or an offline cache. That is a lot of weight for one table.
- **`diesel` / `sea-orm`**: full ORM or query builder, plus a migration story.
  They add a schema DSL and macros to learn and a much larger graph, and give
  nothing for a single table with a single query.
- **`sqlite` crate / raw `libsqlite3-sys`**: the first is a thinner wrapper
  than `rusqlite` and less widely used. The second needs `unsafe` FFI, which
  CLAUDE.md does not allow without an ADR and which has no benefit here.
- **System SQLite (no `bundled`)**: smaller binary, but the SQLite version then
  depends on the OS. Windows has no system `sqlite3.lib` to link, and macOS and
  Linux versions differ. We would give up reproducible query behavior for the
  benchmark.

## Consequences

### Positive

- The SQLite version is the same on every OS and every build, so query
  behavior can be benchmarked the same way everywhere.
- Synchronous API: one call returns rows. No runtime, no futures, and no second
  thread are needed to find out whether threading is justified at all.
- `rusqlite` is safe Rust over FFI and widely used. With `bundled` it pulls in
  only `libsqlite3-sys` (and `cc`/`pkg-config`/`vcpkg` at build time), plus
  `hashlink` for its default statement cache.

### Negative / risks

- Building needs a C compiler on every platform: MSVC on Windows (already
  required for Rust MSVC), Xcode CLT on macOS, gcc/clang on Linux. The first
  build compiles the amalgamation, which adds a one-off compile cost.
- The binary grows by the size of SQLite. The size is **measured in R2**, not
  assumed: see `docs/measurements/R2-sqlite-informal.md`.
- SQLite's built-in `lower()`/`LIKE` fold ASCII only. To match the R1 oracle,
  the lowercased search key is computed in Rust and stored in a `search_text`
  column. Every writer must keep that column up to date. R2 has only one writer,
  the seed.
- No migration framework: a schema change during the spike means deleting the
  spike database file.

## Validation

- The SQLite results equal the R1 in-memory reference for a fixed query set (a
  test in `src/db.rs`).
- Informal release-build query timings (R2 brief §13) show whether the
  synchronous UI-thread query is a problem. The formal SQLite latency number is
  R4 (`BENCHMARK_SPEC.md` §7).
- Release binary size before and after the dependency is recorded.

## Revisit trigger

- Queries measurably block the UI (for example with a much larger library, or
  FTS), which would call for a worker thread.
- The product schema needs real migrations, at which point a migration tool
  (or `PRAGMA user_version` handling) must be decided.
- Binary size becomes a deciding factor in the Avalonia comparison, which would
  bring system SQLite back into consideration.
