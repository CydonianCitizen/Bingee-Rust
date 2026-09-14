# ADR-0004: Opt-in R4 latency measurements

- Status: Accepted for measurement only
- Date: 2026-09-11

## Context

The frozen R4 executable has no search/selection timers. Historical ignored
tests cannot be relabelled as formal GUI measurements. Desktop Computer Use
remains unavailable. Independent latency evidence can still be collected.

## Decision

Add an application-only `r4-measurement` Cargo feature, disabled by default,
and an explicit `--r4-latency` entry point. It reuses the real application
functions and connected Slint callbacks. No Slint feature, renderer, cache,
asset, schema, dependency, or product behavior changes. Preserve the original
release executable and hash before building the instrumented variant.

Use `Instant` around operations, with CSV writes and correctness assertions
outside measured intervals. Record timer overhead. Keep SQLite row mapping
inside SQL timings. GUI timings end at callback return/property update;
they do not include paint, input delivery, or deferred row realization.

## Alternatives

External input timers cannot resolve callback completion. Copying the product
implementation into a benchmark would risk workload drift. Always-on timers
would affect the baseline. None are used.

## Validation and limits

Run the requested gates, plus feature-enabled release build/tests. Verify
query records against the deterministic generator and selection/detail/poster
identity after every timed GUI callback. Keep source hashes and a patch with
the executable identity. Default release identity is checked separately.

Programmatic callback timing does not replace the frozen keyboard memory/soak
procedure or continuous visual observation. Those require a working desktop
connection. No R5 work or optimization is authorized by this decision.
