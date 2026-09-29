# ADR-0022: Local Home and Calendar snapshots

- Status: Accepted
- Date: 2026-09-29

## Context

Home and Calendar must open offline, use R10 tracking and R9 coverage, and avoid
one SQL statement per title or day. Provider air dates are dates; watch events
are UTC instants.

## Decision

Read Home and Calendar on page entry from the current SQLite connection.
Home uses one progress aggregate, one ordered Continue Watching query, one
batch next-episode query, recent history and a 15-date Coming Soon range
(today plus 14 days). Calendar reads one month of Library episode dates and
builds a Monday-first 42-cell grid with one recursive date query. Coverage
warnings use the existing `SeriesProgress` rule. No schema change or refresh
occurs on page entry.

## Alternatives considered

- Stored dashboard counters would need invalidation after every tracking and
  metadata write. The bounded queries are simpler.
- Per-series `next_episode` calls would create a query per card. The batch
  query uses the same season/episode ordering in one statement.
- Converting provider dates to UTC timestamps would shift some air dates to
  another calendar day. Dates remain `YYYY-MM-DD` strings.

## Consequences

Local metadata coverage limits Coming Soon and Calendar. The UI states this.
Home repeats a series under Continue Watching and Up Next when both apply.
Navigation into a removed title uses cached metadata where available.

## Validation

Domain and headless UI tests cover empty and populated pages, completion,
partial coverage, month alignment, leap day, year boundary, removed titles,
and date-only semantics. SQL statement count is fixed with Library size.

## Revisit trigger

Page-entry latency above 50 ms on a representative 1,000-title Library, or
Calendar requests needing more than one month at once.
