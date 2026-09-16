# ADR-0015: Episode coverage is not metadata freshness

- Status: Proposed
- Date: 2026-09-16

## Context

Bingee Android coupled "the series was refreshed recently" with "we have its
episodes", so a fresh series could hide seasons whose episodes had never been
downloaded, and new episodes did not appear. R9 fetches series details and
season episodes separately (ADR-0014), so the two facts diverge by design.

For a cached season, three things must be distinguishable:

1. the season summary is known;
2. its episode list was fetched;
3. that fetch covered the episodes the provider counts.

## Decision

- **Summary known**: a `seasons` row exists. It is written by the series
  detail refresh.
- **Freshness of the series**: `media.details_fetched_at`. It says nothing
  about episodes.
- **Episodes fetched, and their freshness**: `seasons.episodes_fetched_at`,
  written only by that season's episode fetch.
- **Coverage of the last fetch**: `seasons.episodes_known` (episodes that
  fetch supplied) compared with `seasons.episode_count` (what the latest
  series detail says the season holds).

`metadata::Season::coverage()` derives:

| `episodes_fetched_at` | `episodes_known` vs `episode_count` | Coverage |
| --- | --- | --- |
| `NULL` | — | `NotFetched` |
| set | known < count | `Partial { known, expected }` |
| set | known ≥ count, or no count | `Complete(known)` |

Rules:

- A series detail refresh updates `episode_count` but **never** the coverage
  columns. When TMDB reports more episodes, the season becomes `Partial`
  without losing the stored episodes, and the next episode fetch (stale, or
  Refresh) completes it.
- An episode fetch updates coverage for its season only.
- Freshness decides *whether to ask*; coverage decides *what the user is
  told* ("Not downloaded", "8 of 10 saved"). Neither is inferred from the
  other or from `media.metadata_updated_at`.

## Alternatives considered

- **Count episode rows instead of storing `episodes_known`**: rows can be
  kept by the no-empty-deletion rule (ADR-0014), so the count would not
  describe the last fetch.
- **One timestamp per series**: the coupling this ADR exists to avoid.

## Consequences

### Positive

- New episodes show up as partial coverage the moment a series refresh sees
  them.
- The UI can say which seasons are available offline.

### Negative / risks

- Coverage is only as good as TMDB's `episode_count`; a missing count reads
  as complete.

## Validation

`metadata::tests::a_series_detail_refresh_keeps_episode_coverage`,
`an_episode_list_that_changed_is_reconciled_not_rebuilt`,
`nullable_provider_fields_stay_empty`, and the season list coverage labels in
`detail::tests`.

## Revisit trigger

R10 completion rules (whether specials count); calendar/new-episode features
that need per-episode air-date freshness.
