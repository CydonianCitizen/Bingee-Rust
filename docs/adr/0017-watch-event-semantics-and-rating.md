# ADR-0017: Watched state, watch events, rewatches and rating

- Status: Proposed
- Date: 2026-09-16

## Context

Schema v3 (ADR-0016) stores current state (`media_tracking`,
`episode_tracking`) separately from history (`watch_events`). The rules that
connect them decide whether history stays trustworthy: repeated clicks must not
invent rewatches, unmarking must not erase the past, and a correction to
history must not silently change what is shown as watched.

## Decision

**Current state and history are different facts. Actions change state; only a
real watch adds an event; nothing but an explicit history deletion removes
one.**

| Action | Current state | History |
| --- | --- | --- |
| Mark watched (currently unwatched) | `watched_at = now` | one event at `now` |
| Mark watched (already watched) | unchanged | nothing (idempotent) |
| Watch again (movie) | `watched_at = now` | one new event |
| Mark unwatched | `watched_at = NULL` / tracking row deleted | kept |
| Mark season watched | every known episode of the season not yet watched: `watched_at = now` | one event per newly watched episode, all at `now`, inserted in episode order |
| Mark season unwatched | tracking of the season's known episodes deleted | kept |
| Delete history event | unchanged | that event only |
| Set / change / clear rating | `rating` 1–10 or `NULL` | nothing |

- **Idempotency**: "mark watched" is a set operation, never an append; only
  "Watch again" appends to an already watched title. It is offered in the UI
  for movies; episodes support it in the data model
  (`tracking::watch_episode_again`) without a UI yet.
- **`watched_at`** is the time of the latest watch, so a rewatch moves it.
- **Episodes**: a row in `episode_tracking` means watched; unmarking deletes
  the row. Only episodes currently in `episodes` can be marked (the action
  needs a known episode).
- **Bulk operations** are one `BEGIN IMMEDIATE` transaction with set-based SQL
  (one `INSERT … SELECT` for events, one for state), never a statement or
  transaction per episode. Unaired known episodes are included: the user asked
  for the whole season.
- **Rating**: integer 1–10, nullable, one per title (movie or series). It does
  not imply watched and watching does not change it. It is personal only; TMDB
  vote averages are never stored as or shown as the user's rating.
- **Timestamps**: Unix seconds UTC from the injectable `metadata::Clock`.
- **History order**: `watched_at DESC, event_id DESC` — explicit, never
  insertion order.
- **Atomicity**: every compound write (state + event) is one transaction. On
  failure nothing is written; the UI re-reads SQLite and shows a notice, so it
  never displays uncommitted state.
- **No network**: no tracking action reads the token or starts a request.

## Alternatives considered

- **Unmarking deletes the latest event**: "undo" feels natural, but it treats
  history as a mirror of state and loses real past watches of rewatched titles.
- **Every "Mark watched" appends an event**: simplest, but a double click
  becomes a fake rewatch and statistics lie.
- **Deleting the last event unmarks the title**: couples the two facts again
  and makes a history cleanup change the library silently.
- **Rating scale 1–5 or half stars**: 1–10 integers map to TMDB/IMDb habits
  and store without rounding rules.

## Consequences

### Positive

- History is trustworthy for later statistics; state is always what the user
  last set.

### Negative / risks

- A title can be watched with no events (after deleting them) or unwatched
  with events. This is intended and documented; the UI shows each fact where
  it belongs.
- Bulk-marking a season gives all its episodes the same timestamp.

## Validation

`tracking::tests`: idempotent mark, rewatch, unmark keeps events, event
deletion keeps state, rating set/change/clear independent of watched, bulk
transaction rollback on failure, deterministic newest-first order.

## Revisit trigger

A rewatch UI for episodes, "watched on date X" entry, or statistics needing
per-watch metadata (device, partial watches).
