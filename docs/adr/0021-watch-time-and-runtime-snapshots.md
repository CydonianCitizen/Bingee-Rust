# ADR-0021: Watch time from runtime snapshots on watch events (schema v4)

- Status: Proposed
- Date: 2026-09-16

## Context

R11 reports watch time. In schema v3 a watch event names its movie or episode
but not its length; runtimes live only in provider tables (`media`,
`episodes`), which every metadata refresh overwrites. Computing watch time
from them would let a TMDB edit silently rewrite how long the user watched in
the past. Some runtimes are unknown (a title never opened, an episode TMDB has
no runtime for).

## Decision

**Schema v4 adds `watch_events.runtime_minutes`: the runtime known when the
event was recorded.**

- New events copy it inside the same insert: a movie's `media.runtime_minutes`,
  an episode's own `episodes.runtime_minutes`. A series' typical episode length
  is **not** used for an episode: it is an estimate, not that episode's length.
- A known snapshot is never rewritten. Provider refreshes do not touch
  `watch_events` (ADR-0016 firewall), so a later runtime change does not change
  past watch time.
- **Migration v3 → v4** (one transaction with the others): `ALTER TABLE … ADD
  COLUMN runtime_minutes INTEGER CHECK (runtime_minutes > 0)`, then backfill
  each existing event from the runtime stored at upgrade time. Unknown stays
  `NULL`; nothing is invented. Backfilled values are not flagged: before v4 the
  stored runtime was the only runtime the app ever knew for that event.
- **Watch time** = sum over events of the snapshot; a rewatch adds its runtime
  again (never deduplicated).
- **Unknown runtime**: an event whose snapshot is `NULL` uses the runtime
  stored **now**, if any (for example details downloaded after watching
  offline). If there is still none, the event adds nothing to watch time and
  is counted as "watch with no runtime", shown next to the total. Known values
  are never replaced by this fallback.

v1–v3 steps are unchanged. No index is added (ADR-0020 has the measurement).

## Alternatives considered

- **No snapshot, always join current runtimes**: no migration, but history
  changes whenever TMDB edits a runtime. Rejected.
- **Strict snapshot without fallback**: an offline watch before details arrive
  would stay "unknown" forever though the runtime is known later. Rejected.
- **Series runtime as the episode fallback**: fills more minutes with an
  estimate the user cannot see. Rejected.
- **A `runtime_estimated` flag for backfilled rows**: the backfill uses the
  same source a v3 statistic would have used; a flag with no user-visible
  meaning. Rejected.

## Consequences

### Positive

- Past watch time is stable across refreshes and restarts.
- Missing runtime is visible, not zero.

### Negative / risks

- Events recorded before v4 carry the runtime known at upgrade time, not at
  the original watch.
- An event with a `NULL` snapshot keeps following the stored runtime, including
  later changes to it; only known snapshots are frozen.

## Validation

`database::tests::a_real_v3_file_upgrades_to_v4_with_runtimes_from_stored_metadata`
(real v3 file: every provider and personal row unchanged, backfill 45 / NULL /
136 / 136 / NULL for a timed episode, an untimed one, a movie and its rewatch,
an episode no longer listed; later runtime change does not reach events; CHECK
rejects 0; reopen writes nothing),
`a_failed_v4_migration_leaves_a_v3_file_untouched`, and
`statistics::tests::runtime_is_kept_per_event_and_unknown_runtime_is_counted_apart`.
