# ADR-0016: Personal tracking schema (v3) and the provider/personal firewall

- Status: Proposed
- Date: 2026-09-16

## Context

R10 adds the first user-owned data: watched state for movies and episodes, a
personal rating, and watch history. Until now every table except
`library_entries` mirrored a provider (ADR-0014). Provider synchronization
reconciles aggressively: `metadata::save_episodes` deletes episodes TMDB no
longer lists and uses `INSERT OR REPLACE`; `metadata::save` deletes seasons
TMDB dropped, cascading to their episodes. Library removal deletes the
membership row only (ADR-0010). Schema v1 and v2 have shipped and must not be
edited. Personal data must survive metadata refresh, provider deletions,
library removal and re-add, restarts and offline use, and must be exportable
later without the TMDB token, caches or logs.

## Decision

**Personal state lives in its own tables, which no provider code writes and
no provider row can cascade into.**

### Ownership domains

| Provider-owned (TMDB may update/delete) | Personal (only user actions write) |
| --- | --- |
| `media` columns, `external_refs`, `genres`, `media_genres`, `seasons`, `episodes` | `library_entries` (R8), `media_tracking`, `episode_tracking`, `watch_events` |

- `metadata.rs`, `tmdb.rs` and `detail.rs`' refresh paths never name a personal
  table. Personal writes live in `tracking.rs` only and never call the network.
- Provider DTOs (`MediaDetails`, `Season`, `Episode`) carry no personal field.

### Schema v3 (`database::SCHEMA_V3`, appended)

```sql
media_tracking  (local_media_id PK, media_type, watched_at NULL, rating NULL 1..10)
episode_tracking(local_media_id, media_type='tv', season_number, episode_number,
                 watched_at NOT NULL, PK (local_media_id, season_number, episode_number))
watch_events    (event_id AUTOINCREMENT, local_media_id, media_type,
                 season_number NULL, episode_number NULL, watched_at NOT NULL)
```

- Every personal table references **`media` only**, through the composite key
  `(local_media_id, media_type)`, with `ON DELETE RESTRICT`: deleting a title's
  metadata can never silently remove personal data.
- **No foreign key to `seasons` or `episodes`.** Episode tracking and episode
  events address an episode by its stable local key (series id, season number,
  episode number, ADR-0014). Reconciliation may delete and re-insert episode
  rows freely; personal rows are untouched.
- Type rules in the schema: movie watched state only on movies
  (`CHECK (watched_at IS NULL OR media_type = 'movie')`), episode tracking only
  on series, a movie event has no episode, a series event always has one.
- `watch_events.event_id` is `AUTOINCREMENT`, so an id is never reused after a
  deletion and is a deterministic tie-breaker.
- Indexes: `watch_events (watched_at)` for newest-first history,
  `watch_events (local_media_id, media_type)` for the foreign key.
- Timestamps: Unix seconds, UTC, `INTEGER`, as in v1/v2. No formatted strings
  are stored; local time is produced only for display (SQLite `localtime`).

### Provider-removed episodes

The smallest robust rule: **personal rows are simply not deleted.** When TMDB
drops an episode, its `episodes` row goes (ADR-0014) but its
`episode_tracking` row and its events stay. Progress queries join tracking to
the current `episodes` rows, so the orphan counts nowhere and shows nowhere
except in history (as `S01E05`). If the provider lists that episode number
again, it is watched again immediately.

### Privacy and backup boundary

- Watched state, ratings and history never leave the database: no telemetry,
  sync or upload; TMDB requests carry only provider ids and the token. Normal
  tracking actions are not logged; only write failures are.
- A future export of "personal Bingee data" is exactly: `library_entries`,
  `media_tracking`, `episode_tracking`, `watch_events`, plus `external_refs`
  (to re-identify titles) and optionally `media`/`seasons`/`episodes` for
  names. Never the token (OS credential store), poster cache, or logs.

## Alternatives considered

- **Columns on `media` / `episodes`** (`watched`, `rating`): the shortest
  schema, but `INSERT OR REPLACE` and season reconciliation would erase them,
  and every provider update would have to name columns it must not touch.
- **Personal tables as cascade children of `episodes`**: consistent orphans
  are impossible, but a transiently shorter TMDB answer would destroy history.
- **Surrogate episode id** in tracking: survives renumbering, but `episodes`
  rows are replaced on refresh, so the id itself is not stable without
  rewriting reconciliation.
- **A separate `watched` boolean next to `watched_at`**: allows "watched on an
  unknown date" (useful for imports) but also contradictory rows; a later
  migration can add it when an import needs it.

## Consequences

### Positive

- No refresh, 404, removal or re-add path can reach personal data; tests prove
  each (see Validation).
- Export boundary is four tables plus identities.

### Negative / risks

- A provider that **renumbers** an episode moves its metadata but not its
  watched state, which stays on the old number (and reappears if that number
  is reused). Rare for TMDB; revisit with import/backup work.
- Orphaned personal rows are kept indefinitely (small).
- "Watched, date unknown" is not representable yet.

## Validation

`database::tests` (fresh → v3, a real v2 file with library, identities,
details, genres, seasons, episodes and coverage → v3, rollback, reopen, type
checks), `tracking::tests` (refresh, removed-episode, 404 and library
remove/re-add firewalls), `detail::tests` (UI journeys with restart and
offline).

## Revisit trigger

Backup/restore or Android import (unknown watch dates, identity mapping),
episode renumbering reports, or a need to delete personal data on purpose.
