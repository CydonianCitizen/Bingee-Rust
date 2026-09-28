# ADR-0020: Statistics — viewing history versus current state

- Status: Proposed
- Date: 2026-09-16

## Context

R11 adds a Statistics page computed from the local database. R10 stores two
different kinds of personal fact (ADR-0017): **history** (`watch_events`, one
row per real watch, kept when a title is unmarked, removed only by an explicit
History deletion) and **current state** (`media_tracking`, `episode_tracking`,
ratings). Library membership (`library_entries`) is a third, independent fact.
A statistic that mixes them silently becomes wrong the moment a user unmarks,
rewatches, deletes a history entry or removes a title from the Library.

## Decision

**Statistics are derived on demand, never stored, and every number belongs to
exactly one of two labelled sections.**

### Viewing history (from `watch_events` only; the time range applies)

| Metric | Definition |
| --- | --- |
| Watches | Events in the range, movies and episodes. |
| Movie / episode watches | Events of that kind. A series is never "one episode". |
| Different movies / episodes | Distinct targets (movie, or series + season + episode number) with an event in the range. |
| Rewatches | Events in the range that are not the first recorded event of their target. The first event may lie before the range. Repeated "Mark watched" clicks never create events (ADR-0017), so they are never rewatches. Deleting a target's first event makes its next event the first. |
| Watch time | Sum of known runtimes of those events; unknown runtimes are counted apart and shown (ADR-0021). |
| Movies and TV | Known watch time of movie events versus episode events, with watch counts beside them; never "number of movies" versus "number of episodes". |
| Activity | Watch events (the primary chart measure) per local day (30 days), local month (12 months) or local year (all time), empty buckets included. |
| Genres | Watches and known minutes per genre. Episodes use their series' genres. A watch counts toward **every** genre its title has, so genre totals exceed the watch total; this is said on the page and no percentages are shown. Titles without stored genres are one "No genre stored" row. Genres are the ones stored **now** (no genre snapshot on events): a refresh that changes a title's genres re-categorises its past watches. |

History ignores Library membership and current watched state: a watched title
removed from the Library, or marked unwatched, still counts. Specials (season
0) are episodes like any other here.

### In your Library now (current state of titles in `library_entries`; no range)

| Metric | Definition |
| --- | --- |
| Movies watched | Library movies with `watched_at` set, "x of y". |
| Episodes watched | Watched episodes still stored for Library series, specials included; tracking of an episode the provider no longer lists counts nowhere (ADR-0018). |
| Series progress | Each Library series is exactly one of: **Complete** = `SeriesProgress::is_complete` (every regular season's list downloaded and complete, every regular episode watched); **In progress** = some regular episode watched, not complete; **Not started** = no regular episode watched (watched specials alone do not start a series). |
| Incomplete episode lists | Library series whose `coverage_complete()` is false, shown as a note: they cannot be complete. |
| Ratings | The user's own 1–10 rating of Library titles (movies and series), one per title regardless of rewatches. Count, distribution 1–10, and the average **over rated titles only**; never TMDB's score, unrated titles never count as zero. |

Series classification reuses R10's `tracking::library_status` (the same
aggregate the Library rows use), so there is one completion algorithm.

**Why Library-scoped:** history answers "what have I watched"; the current
section answers "what am I tracking now". A removed title is no longer being
tracked, but its watches happened.

### Ranges and time

- Three ranges: **30 days** = today plus the 29 previous local calendar days;
  **12 months** = this local month plus the 11 previous; **All time** = every
  event, one bar per local year since the first watch (at most 100).
- Range starts and bucket boundaries are local midnights converted to UTC by
  SQLite (`localtime` / `utc` modifiers), the same local-time source History
  uses. Persisted timestamps stay UTC. The newest bucket has no upper bound, so
  an event dated after "now" (a clock moved back) still has a bar.
- `now` comes from the injectable `metadata::Clock`.

### Not in R11

Streaks, binge-session inference, percentages of genres, genre snapshots, any
persisted counter.

### Loading and threading

The page reloads when it is opened and when a range is chosen, synchronously
on the UI thread, in a fixed number of SQL statements (6–7, asserted by a test
with SQLite statement tracing; no statement per title or event).

Evidence (informal, Windows 11, release build, file database; not a
`BENCHMARK_SPEC.md` run; `statistics::tests::informal_statistics_timings`,
`statistics_page::tests::informal_statistics_page_timings`). Synthetic
test-only history: 1,000 titles (850 movies, 150 series), 17,505 episodes,
10,000 watch events over three years, 19 genres, 313 ratings, rewatches.

| Query | Median |
| --- | --- |
| `load`, 30 days / 12 months / all time | 18.0 / 22.0 / 21.9 ms |
| Viewing totals (per-target aggregate) | 10.7 ms |
| Activity, 12 months | 0.47 ms |
| Genres, all time | 4.5 ms |
| Library now (progress + ratings) | 5.9 ms |

| End to end (query, view model, software render 1280×800) | Median / max |
| --- | --- |
| First open, cold statement cache | 46.9 ms (single) |
| Choose 30 days / 12 months / all time | 27.9 / 29.6 / 28.4 ms, max 31.7 ms |

The first version of the queries took 58 ms (window function with
`count(DISTINCT …)`, genres joined per event, `localtime` per event). The
query plans showed index lookups only, no N+1; the cost was sorting and time
zone conversion. Rewritten as a per-target aggregate, per-title genre
aggregation and per-bucket index range counts. A candidate index on
`watch_events (local_media_id, season_number, episode_number, watched_at)`
saved about 2.5 ms and was **not** added.

**Decision: stay synchronous.** About two frames once per deliberate action
(opening the page, clicking a range) on a history far larger than typical; no
continuous input drives it. A worker would need a second connection or moving
`SharedDb` off the UI thread (it is UI-thread only today) plus stale-result
handling, for no visible gain.

## Alternatives considered

- **Persisted counters** (total minutes, per-genre counts): fast, but a second
  source of truth that deletion, unmarking and metadata refresh would have to
  keep consistent. Rejected until measurements need it.
- **Library-scoped history**: would make removing a title erase what was
  watched. Rejected.
- **All-titles current state** (not Library-scoped): mixes "tracked now" with
  titles the user dropped; ambiguous with History. Rejected.
- **Normalised genre shares** (1/n per genre): sums to 100 % but hides that
  multi-genre titles count in each; raw counts with a note are clearer.
- **Rolling 30×24 h / 365-day windows**: disagree with calendar-labelled bars.

## Consequences

### Positive

- One definition per number, visible on the page, testable with a fake clock.
- No new state to migrate or repair.

### Negative / risks

- A genre refresh re-labels old watches.
- Statistics cost grows linearly with history size (≈ 2 ms per 1,000 events
  measured).
- Current-state numbers ignore titles removed from the Library, even if rated.

## Validation

`statistics::tests` (empty, rewatch/unmark/delete, delete-only-event, episodes
and specials, runtime snapshot and unknown runtime, Library removal, genres and
refresh, ratings, series progress with coverage, 30-day / 12-month / all-time
boundaries, > 200 events, constant statement count) and
`statistics_page::tests` (empty, reload on open, ranges, deletion and removal,
error state).

## Revisit trigger

Statistics load above ~50 ms on target hardware, histories far beyond
10,000 events, or a Home/insights feature needing the same numbers per frame.
