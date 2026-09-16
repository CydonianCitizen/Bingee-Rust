# ADR-0019: Personal tracking writes stay synchronous on the UI thread

- Status: Proposed
- Date: 2026-09-16

## Context

R9 measured SQLite commits at 4–9 ms on the UI thread. R10 adds frequent,
user-generated writes: episode toggles, whole-season marks, movie watched and
ratings. Each one also re-reads tracking for the pane and the Library's
personal status. A background database worker would need a second connection
or a channel, ordering rules for rapid clicks, and "pending" UI states, and it
would make "the UI shows only committed state" harder to guarantee.

## Decision

**Keep personal writes synchronous on the UI thread**: one small transaction,
then a re-read and render of the committed state, in the same callback. No
database worker is introduced.

Evidence (informal, Windows 11, release build, file-backed database, not a
`BENCHMARK_SPEC.md` run; `detail::tests::informal_tracking_ui_timings` and
`tracking::tests::informal_tracking_timings`):

| Action, end to end on the UI thread (1,000-title library) | Median | Max |
| --- | --- | --- |
| Episode toggle, 100 rapid clicks | 9.96 ms | 12.2 ms |
| Mark a 250-episode season watched | 21.4 ms | 24.4 ms |
| Movie watched toggle | 9.56 ms | 23.7 ms |
| Rating change | 8.25 ms | 11.1 ms |

| SQL alone | Median |
| --- | --- |
| One episode commit | 4.9 ms |
| 250-episode season watched / unwatched | 7.2 / 5.8 ms |
| 2,000-episode season watched / unwatched | 11.7 / 7.8 ms |
| Series progress, 61 seasons / 17,000 episodes | 4.3 ms |
| Library status, 1,000 titles | 5.3 ms |
| Recent history, 200 of 33,099 events | 0.55 ms |

A click costs at most about one frame; a whole-season mark about one to two
frames, once per deliberate action. Rapid clicking does not accumulate (max
stays within 25 % of the median). Commit time is dominated by the disk sync.

## Alternatives considered

- **Background database worker**: removes the ~10 ms, but adds concurrency,
  ordering and pending-state UI for an action that is not continuous.
- **Optimistic UI with background commit**: violates "UI reflects only
  committed state" and needs rollback UI.
- **`synchronous = NORMAL` in WAL mode**: would cut commit time, but changes
  durability of all data; not justified by a one-frame cost.

## Consequences

### Positive

- Simple, ordered, and the pane can never show uncommitted state.

### Negative / risks

- Slow disks (network home folders, HDDs) could make a season mark visibly
  hitch.
- The Library status reload after each action is O(library size).

## Validation

The timings above; UI tests that a failed write leaves committed state shown.

## Revisit trigger

Any single action measured above ~50 ms on target hardware, a batch feature
(import, "mark series watched" for hundreds of seasons), or user reports of
stutter.
