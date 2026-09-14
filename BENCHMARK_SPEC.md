# Bingee Desktop — Shared Benchmark Specification

## Purpose

Compare Rust + Slint and C# + Avalonia using the same representative workload.

The goal is not to prove a language is universally faster. The goal is to choose the better stack for Bingee Desktop.

## Non-functional priorities

In order:

1. perceived responsiveness;
2. bounded / predictable RAM usage;
3. near-zero unnecessary CPU while idle;
4. fast startup;
5. reasonable binary/package footprint;
6. maintainability and development complexity.

## Benchmark environment

Every report must include:

- OS and version;
- CPU;
- RAM;
- architecture;
- build mode/profile;
- framework/runtime versions;
- commit SHA;
- whether debugger/profiler was attached.

Do not compare a debug Rust build to a release C# build or vice versa.

## Shared dataset

Use a deterministic generated dataset of **1,000 media items**.

Each item should eventually include:

- stable local id;
- title;
- original title;
- year;
- media type (`Movie` / `TV`);
- rating/progress-like lightweight fields;
- deterministic searchable text.

When image testing begins, both implementations must use the same local poster assets and dimensions.

Do not use live TMDB responses for performance comparison.

## Shared UI workload

Both implementations should eventually expose:

- sidebar;
- library pane;
- detail pane;
- local search;
- item selection;
- scrollable/virtualized media presentation.

The layout need not be pixel-identical, but the amount and type of work must be comparable.

## Measurements

### 1. Cold startup

Measure process launch to first usable/interactive main window.

Record multiple samples after a true process exit.

Do not report a single run as definitive.

### 2. Idle RAM

Procedure:

1. launch release build;
2. wait until startup activity settles;
3. perform no interaction for a fixed settling interval;
4. record process working-set/resident-memory metric appropriate to the OS.

Record the exact metric and tool used.

### 3. Post-interaction RAM

Perform a fixed script:

1. open library;
2. scroll through a representative range;
3. select multiple items;
4. run several searches;
5. return to the initial library state;
6. allow a fixed settling interval;
7. measure RAM.

This is especially important after poster caching is implemented.

### 4. Idle CPU

After settling, measure CPU use for a fixed interval.

Target behavior: effectively no continuous work while the user does nothing.

### 5. Search latency

Use the same predetermined queries against the same 1,000-item dataset.

Measure input/update request to updated result model/UI where instrumentable.

### 6. Selection latency

Measure selecting an item to the detail model being updated/render-ready.

### 7. SQLite query latency

After R2, run the same logically equivalent query against the same seeded dataset.

Record warm/cold assumptions explicitly.

### 8. Footprint

Record separately:

- main executable/binary;
- distributable/package;
- installed footprint where meaningful.

Do not confuse disk size with RAM usage.

## Fairness rules

- Same dataset.
- Same feature slice.
- Same build class (release vs release).
- Same machine for direct comparison.
- Same poster assets.
- Same benchmark script.
- No hidden preload in one implementation.
- No network dependency.
- No manually disabled functionality solely to improve one score.

## Qualitative scorecard

Performance alone does not choose the winner. Also score:

- UI development ergonomics;
- architecture clarity;
- testability;
- cross-platform behavior;
- packaging complexity;
- debugging experience;
- accessibility support;
- maintainability;
- learning curve;
- ecosystem/library maturity.

## Reporting

Never write “X is faster/lighter” unless the benchmark data collected in this repository supports that statement.

Raw results should be kept alongside summaries so measurements can be audited later.
