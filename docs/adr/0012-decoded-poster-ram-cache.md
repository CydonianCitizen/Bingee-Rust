# ADR-0012: Decoded poster RAM cache and row binding

- Status: Proposed
- Date: 2026-09-14

## Context

Decoded posters are the largest memory cost in the app. R3 showed that an
explicit byte-budgeted LRU shared by the list and the detail pane keeps
memory bounded under long scrolls (ADR-0003, 12 MiB). Production loads are
now asynchronous, and Slint recycles list rows, so a late result could land
on a row that shows another title.

## Decision

- **Budget 12 MiB**, as in R3, charged at the real decoded size
  (RGB8: width × height × 3). At `w185` that is about 81 posters: a screen
  of rows needs about 10.
- **LRU**, keyed by the poster key (ADR-0011), shared by Library rows,
  Discover rows and both detail panes. A hit moves the entry to the back;
  inserting evicts from the front until the new entry fits; an entry larger
  than the whole budget is not cached. Eviction drops the cache's reference
  only. Failed loads never enter it.
- The cache stores `SharedPixelBuffer<Rgb8Pixel>`, which is `Send`, so
  workers can hand decoded pixels to the UI thread; rows wrap them in a
  `slint::Image` when built.
- **Pull binding**: a finished load only inserts pixels under its key and
  tells both lists and both detail panes "key K is ready". A list model then
  re-reads the rows whose *current* item has key K; a detail pane updates
  only if it still shows key K. No result is ever pushed into a row by
  position, so a poster can never appear on a row that now shows another
  title.
- Counters (hits, misses, loads from disk or network, failures, evictions)
  are kept and logged as one line at exit, not per event.

## Alternatives considered

- **Reuse the R3 `PosterCache` as is**: it loads synchronously through its
  own loader and is keyed by fixture numbers; changing it would change the
  benchmark fixture. A small production LRU keeps the R4 workload
  byte-for-byte reproducible.
- **Cache `slint::Image`**: not `Send`, so worker results could not be handed
  over without extra plumbing.
- **Per-row request tokens**: also works, but needs bookkeeping per row
  instance; key-addressed pulls make stale results harmless by construction.

## Consequences

### Positive

- Memory stays bounded however far the user scrolls.
- Wrong-row binding is impossible by design and covered by a test.

### Negative / risks

- A `slint::Image` is created per row build from shared pixels; the renderer
  may upload the same pixels again for a re-created row.

## Validation

Tests: LRU order, eviction within budget, oversized entries, failures not
cached, a late poster for a row that now shows another title, a headless
1,000-title library that loads only visible posters and stays within budget
during a long scroll.

## Revisit trigger

Measured memory or texture-upload cost above the R3 baseline, or larger
image variants.
