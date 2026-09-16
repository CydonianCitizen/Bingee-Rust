# ADR-0014: TV season and episode storage, transactions and reconciliation

- Status: Proposed
- Date: 2026-09-16

## Context

R9 stores series detail, season summaries and episode metadata so that R10
can add watch tracking without redesigning provider synchronization. Episodes
are provider metadata only in R9. Seasons must include TMDB's season 0
(specials). A series may have dozens of seasons; opening it must not cause
dozens of requests. Repeated refreshes must never duplicate rows or reach
personal data. Schema v1 (ADR-0006) is shipped and must not be rewritten.

## Decision

### Schema v2 (`database::SCHEMA_V2`, appended migration)

- `media` gains `status`, `tagline`, `last_air_date`, `season_count`,
  `episode_count` and `details_fetched_at`. `runtime_minutes` (v1) holds a
  movie's runtime or a series' typical episode length.
- `genres (source, external_id, name)`, primary key `(source, external_id)`.
- `media_genres (local_media_id, source, external_id)`, primary key of all
  three, foreign keys to `media` (cascade) and `genres`.
- `seasons`, primary key `(local_media_id, season_number)`;
  `season_number >= 0`; `media_type` fixed to `'tv'` with the composite
  foreign key to `media (local_media_id, media_type)`, so a movie can never
  get seasons. Summary columns plus the coverage columns of ADR-0015.
- `episodes`, primary key `(local_media_id, season_number, episode_number)`,
  foreign key to its season (cascade); `external_id` (TMDB episode id) with a
  partial unique index `(local_media_id, season_number, external_id)`.
- No `watched`, `watched_at`, rating, progress or completion columns anywhere.

Identity: an episode is addressed by the explicit domain fields local series
id + season number + episode number (the primary key, so a repeated refresh
upserts instead of adding rows); the TMDB episode id is stored as its stable
provider identity and cannot appear twice in a season.

### Loading strategy

- Series detail (`/3/tv/{id}`) brings **season summaries only**.
- A season's episodes come from `/3/tv/{id}/season/{n}` — one request per
  season, never one per episode; it carries every stored episode field
  (`id`, `episode_number`, `name`, `overview`, `air_date`, `runtime`,
  `still_path`).
- Episodes are fetched only for the season **on screen**, and only if never
  fetched, stale (ADR-0013) or on Refresh/Try again. The pane shows the first
  regular season by default (specials if that is all there is), so opening a
  series costs at most two requests. Nothing is fetched at startup, for other
  seasons, or for other titles.

### Season 0

Stored, listed first (season order) and labelled "Specials" unless TMDB names
it. It is selectable and fetched like any season. Whether specials count
toward completion is an R10 policy decision; R9 does not decide it.

### Transaction boundaries

1. **Detail refresh** (`metadata::save`, one `BEGIN IMMEDIATE`): update the
   `media` row's provider columns (only where `local_media_id` and
   `media_type` match), replace the title's `media_genres` (genres upserted,
   never deleted, since other titles share them), upsert season summaries,
   remove seasons the answer no longer lists. All or nothing.
2. **Season episodes** (`metadata::save_episodes`, one transaction per
   season): ensure the season row, remove episodes the answer no longer
   lists, upsert every listed episode, record coverage. A failure affects that
   season only.

Library membership (`library_entries`), `added_at`, provider identity
(`external_refs`) and the other seasons are never written by either.

### Reconciliation

- **Upsert, never rebuild.** No refresh deletes and recreates a title. A
  season summary update leaves the season's coverage columns alone.
- **Grown season**: existing episodes still correspond and are updated in
  place; new ones are inserted.
- **Shrunk or renumbered season**: episodes absent from the new answer are
  removed first, which also frees a renumbered episode's old row; then the
  answer is upserted (`INSERT OR REPLACE`, so a provider id moved to another
  number replaces one row rather than failing the season).
- **Dropped season**: removed with its episodes.
- **Empty answers are not deletions.** An empty season list or episode list
  removes nothing (a truncated answer cannot wipe the cache); an empty
  episode list still records its coverage.
- **Malformed answers**: entries without a usable number are skipped;
  duplicate episode numbers and duplicate provider ids are reduced to their
  first occurrence before storage; duplicate genres give one join row.

### Personal-state firewall (for R10)

Refresh code writes only the provider-owned tables and columns above. Personal
tables added later must:

- live in their own tables, never as columns of `media`, `seasons` or
  `episodes`;
- **not** be `ON DELETE CASCADE` children of `seasons` or `episodes`, because
  reconciliation may remove and re-insert those rows; reference episodes by
  (local series id, season number, episode number) and/or the provider
  episode id instead.

## Alternatives considered

- **Fetch every season with the series**: complete offline data after one
  open, but tens of requests for long series and rate-limit risk.
- **Keep episodes TMDB no longer lists**: never loses a row, but shows
  ghosts and wrong counts forever. Personal data will not live in these
  rows, so removing them is safe.
- **Genres as a comma-separated column**: simpler, but not queryable and not
  shareable between titles.
- **Surrogate episode ids**: unnecessary; the natural key is stable and
  explicit.

## Consequences

### Positive

- Bounded requests; offline access to every season ever opened.
- Repeated refreshes are idempotent; a failure never touches other seasons.
- R10 can reference episodes by stable keys without schema redesign.

### Negative / risks

- Seasons never opened have no episodes offline.
- A provider that transiently returns fewer episodes loses those rows until
  the next fetch (metadata only).

## Validation

`database::tests` (fresh → v2, real v1 file → v2 keeps membership, refs and
poster paths, rollback, reopen), `metadata::tests` (season 0, nullable
fields, idempotent fetches, grown/shrunk/renumbered seasons, duplicate ids,
wrong media type refused, coverage kept by detail refresh, restart),
`detail::tests` (only the shown season is fetched).

## Revisit trigger

R10 tracking design; a need for complete offline series (e.g. "download all
seasons" as an explicit action); provider identity changes for episodes.
