# ADR-0026: Bounded in-app metadata refresh

- Status: Accepted
- Date: 2026-09-29

## Context

R14 needs automatic Library refresh without blocking local startup or
saturating the four shared network workers. R9 already defines seven-day
freshness. Unopened seasons must not trigger full downloads.

## Decision

One application-owned coordinator runs only while Bingee is open. Automatic
refresh defaults off until enabled in Settings. On startup it waits for local
UI and token initialization. While enabled, it checks every six hours. A
cycle selects at most 50 stale Library titles and 50 stale or partial
previously downloaded seasons. It keeps at most one automatic request in
flight. Thus at least three of four workers remain available for Discover,
manual Refresh, and posters. It uses R9 `freshness` and never refreshes
removed titles or unopened season lists. A changed count on a downloaded
season can enqueue that season in the current cycle.

After 429 it pauses automatic work for 30 minutes; after offline, timeout,
or 5xx it pauses 15 minutes. An invalid token stops automatic work until
credential state changes and triggers the existing Settings credential check.
No retry loop runs on errors. Cached content stays. An item removed from the
Library while a request is in flight is skipped on arrival.

## Alternatives considered

- A new priority worker pool duplicates infrastructure and threads. One
  maintenance request at a time leaves interactive capacity in the existing
  pool.
- Refreshing every Library title or season each cycle ignores freshness and
  can cause large request bursts.
- An OS daemon would run while Bingee is closed and adds platform services
  outside R14 scope.

## Consequences

Large Libraries may need multiple cycles. The app stays local-first and
maintenance work remains bounded. Manual Refresh remains available.

## Validation

Fake TMDB tests cover fresh/stale selection, token absence, error pauses,
one-in-flight behavior, and new episode processing. A 1,000-title synthetic
selection check confirms queue bounds. No live TMDB is used in tests.

## Revisit trigger

Measured user request delay with one maintenance job, or Libraries that need
unacceptable time to refresh within the configured cadence.
