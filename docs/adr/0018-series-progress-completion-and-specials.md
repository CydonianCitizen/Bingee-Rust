# ADR-0018: Coverage-aware series progress, completion, specials and next episode

- Status: Proposed
- Date: 2026-09-16

## Context

Bingee stores episodes only for seasons that were opened (ADR-0014), and
separates coverage from freshness (ADR-0015): a season can list 10 episodes
while 6 are downloaded. A naive `watched / episode_count` or
`watched == episodes_known` would show "complete" for a series whose later
episodes were never downloaded, or never become incomplete when TMDB adds an
episode. Season 0 (specials) was left open by R9.

## Decision

### Specials

**Season 0 is trackable episode by episode but excluded from the main
progress, completion and next-episode logic.** Its own count is reported
separately ("3 / 8 specials watched"). Specials are often optional or
out-of-order; counting them would leave many series permanently incomplete.

### Progress

Computed in SQL aggregates (never a query per episode) from the local
`seasons`, `episodes` and `episode_tracking` rows:

- `known` = normal (season ≥ 1) episode rows stored locally;
- `watched` = those with a tracking row (orphaned tracking does not count);
- per season, `covered` = `metadata::coverage(...)` is `Complete` (ADR-0015).

`SeriesProgress::coverage_complete()` holds only if the series detail was
fetched, at least one normal season exists, and **every** normal season is
covered. The percentage is `watched / known`; the UI shows it as definitive
only with complete coverage, otherwise labelled over downloaded episodes
("12 / 18 downloaded episodes watched · episode list incomplete").

### Completion

A series is **watched (complete)** only if coverage is complete, `known > 0`
and `watched == known`. `watched == episodes_known` is never enough. When a
series refresh raises a season's `episode_count`, that season becomes
`Partial` and the series stops being complete at once; when the episode fetch
adds episode 11, it is simply an unwatched known episode.

### Next episode

`tracking::next_episode` returns one of:

| Result | Meaning |
| --- | --- |
| `Episode { season, number, name }` | first unwatched known normal episode, by season then episode number (gaps allowed) |
| `Complete` | series complete as defined above |
| `CaughtUp` | every known normal episode watched, but coverage is incomplete — more may exist |
| `NothingKnown` | no normal episode downloaded yet |

Nothing is ever marked automatically.

### Continue watching

`tracking::continue_watching` (domain only, no UI in R10): library series with
at least one watched and at least one known unwatched normal episode, most
recently watched first. Home (a later milestone) will present it.

## Alternatives considered

- **Denominator = TMDB `episode_count`**: shows 12 / 24 even when only 18 rows
  exist, and cannot say which episodes are missing; misleading for next-episode.
- **Count specials**: simplest, but permanent incompleteness for most long
  series.
- **Store progress/completion columns**: cached derived data that a refresh
  would have to keep consistent, breaking the firewall (ADR-0016).

## Consequences

### Positive

- "Complete" can never be shown over incomplete metadata; new episodes reopen
  a series naturally.

### Negative / risks

- A series with a never-opened season is "incomplete" until that season is
  downloaded; the label says so.
- Coverage trusts TMDB's `episode_count`; a missing count reads as covered
  (ADR-0015).

## Validation

`tracking::tests`: no episodes, partial, complete, all known watched but
partial, all watched complete, specials only, specials plus seasons, episode
11 after completion, next-episode ordering with gaps and specials.

## Revisit trigger

Home/Continue Watching presentation, per-series "count specials" preference,
air-date-aware progress (unaired episodes) for Calendar.
