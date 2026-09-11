# R2 — informal SQLite observations

**Informal.** This is not an R4 benchmark and is not comparable to the
Avalonia spike yet. It exists only to decide whether the SQLite query is big
enough to justify a worker thread in R2 (brief §8, §13).

## Environment

| | |
| --- | --- |
| OS | Windows 11 Home 10.0.26200, 64-bit |
| CPU | Intel Core i5-10500H @ 2.50 GHz, 6 cores / 12 threads |
| RAM | 23.8 GB |
| Disk | SSD (temp dir on the system SSD) |
| Build | `release` profile (Cargo defaults: no LTO, no strip) |
| Toolchain | rustc 1.98.1, Slint 1.17.1, rusqlite 0.40.2, bundled SQLite 3.53.2 |
| Commit | uncommitted working tree on top of `c29907a` |
| Debugger/profiler | none attached; normal desktop session with other apps open |

## Query timings

**Method:** `cargo test --release informal_query_timings -- --ignored --nocapture`
(`src/db.rs`). The test:

1. creates a fresh database file in the OS temp dir;
2. reopens it (as the app does on every launch);
3. calls `db::search` 50 times for each query on that one connection, timing
   each call with `std::time::Instant`.

Each timed call covers everything the app pays for per keystroke on the Rust
side: lowercasing the query, running the statement, and mapping every row into
an owned `MediaItem`. The Slint model reset and row rendering are **not**
included.

**Cache state:** warm. The OS file cache is warm (the file was just written).
The SQLite page cache is warm because `open` runs `count(*)` over the table.
`first` is the first call on the connection and includes statement
preparation. This matches the app, which also runs `open` and then the first
query at startup.

**Samples:** 5 separate processes × 50 calls per query. All times are in µs.

| Run | Query | Rows | first | min | median | p95 | max | R1 in-memory median |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | `""` (all) | 1000 | 3140 | 1655 | 2020 | 2496 | 3140 | 12 |
| 1 | `harbor` | 25 | 297 | 246 | 365 | 548 | 574 | 22 |
| 1 | `zzzz` (none) | 0 | 257 | 231 | 304 | 356 | 402 | 62 |
| 2 | `""` (all) | 1000 | 2615 | 1803 | 2226 | 3115 | 3452 | 20 |
| 2 | `harbor` | 25 | 521 | 464 | 485 | 679 | 703 | 31 |
| 2 | `zzzz` (none) | 0 | 432 | 255 | 308 | 363 | 432 | 62 |
| 3 | `""` (all) | 1000 | 4583 | 2138 | 2516 | 5008 | 5387 | 28 |
| 3 | `harbor` | 25 | 814 | 386 | 405 | 675 | 814 | 42 |
| 3 | `zzzz` (none) | 0 | 530 | 278 | 328 | 530 | 559 | 72 |
| 4 | `""` (all) | 1000 | 3817 | 2370 | 3212 | 4537 | 4798 | 19 |
| 4 | `harbor` | 25 | 562 | 329 | 432 | 512 | 562 | 24 |
| 4 | `zzzz` (none) | 0 | 380 | 338 | 661 | 898 | 1780 | 105 |
| 5 | `""` (all) | 1000 | 3026 | 1825 | 2766 | 4237 | 4369 | 20 |
| 5 | `harbor` | 25 | 522 | 425 | 438 | 512 | 680 | 29 |
| 5 | `zzzz` (none) | 0 | 373 | 356 | 381 | 637 | 692 | 151 |

Startup-related, one sample per run (µs):

| Run | open + create + seed (new file) | open existing (no reseed) |
| --- | ---: | ---: |
| 1 | 30984 | 2445 |
| 2 | 33228 | 2332 |
| 3 | 35378 | 2206 |
| 4 | 26778 | 3605 |
| 5 | 96981 | 1762 |

The R1 in-memory column is `library::search` on the same data. It returns
indices only and builds no records, so it is **not** a like-for-like
comparison. It is included only for scale.

## Reading

- The slowest case, returning all 1,000 records, has a median of 2.0–3.2 ms
  and a worst sample of 5.4 ms. A 60 Hz frame is 16.7 ms.
- A no-match query (a full scan with `instr` and no rows to map) takes about
  0.3–0.7 ms. Most of the fetch-all cost is therefore building 1,000 owned
  `MediaItem`s (strings plus the recomputed search key), not SQLite.
- Seeding a new file takes 27–97 ms, once per database file. Reopening an
  existing file takes about 2–4 ms and happens on every launch.

**Decision for R2:** keep the query synchronous on the UI thread. Nothing here
justifies a worker thread, an async runtime, or a pool. Revisit if the formal
R4 search-latency measurement, or a larger dataset, pushes a keystroke
toward a frame budget.

## Binary footprint

`target/release/bingee-desktop.exe`, same toolchain and profile, measured with
`ls -l`:

| State | Bytes |
| --- | ---: |
| R1 (before `rusqlite`) | 15,114,240 |
| R2 (with `rusqlite` + bundled SQLite) | 16,833,536 |
| Difference | +1,719,296 (≈ +1.64 MiB, +11.4 %) |

This is the executable only. It is not a package or installed footprint.
