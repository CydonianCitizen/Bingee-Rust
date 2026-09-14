# ADR-0008: Network execution: blocking HTTP on an app-owned worker pool

- Status: Proposed
- Date: 2026-09-14

## Context

R7 is the first milestone with network I/O. It must never block the Slint
event loop, must not flood TMDB while the user types, and a slow, stale
response must never overwrite a newer one. Load is tiny: a debounced search
issues two requests (movie, TV) per page; validation is one request. Slint
properties and models may only be touched on the UI thread.

## Decision

- **HTTP/TLS**: `ureq` 3 (blocking), default features: rustls with the
  `ring` provider, Mozilla roots from `webpki-roots`, gzip. One `Agent` for
  the whole app, so connections are pooled. **No async runtime**: nothing in
  R7 needs thousands of concurrent requests or cancellation of in-flight
  sockets.
- **Timeouts**: connect 10 s, whole request (including body) 15 s. Both
  finite; a hung server becomes `Timeout`.
- **Execution** (`src/network.rs`): `Network` owns a fixed pool of 4 worker
  threads fed by one channel, created once at startup. A job runs the
  blocking work and hands its result back with
  `slint::Weak::upgrade_in_event_loop`, so every state change and every
  property update happens on the UI thread. Dropping `Network` closes the
  channel and the workers exit after their current job. At process exit,
  in-flight requests are abandoned (they have no side effects).
- **Debounce**: 300 ms, measured by a restartable single-shot `slint::Timer`
  on the UI thread. The decision logic is a pure state machine
  (`SearchController`, `src/search.rs`) that takes explicit `Instant`s, so it
  is tested with synthetic time.
- **Stale responses**: every query gets a new generation number. A response
  carries the generation and page it was requested for and is applied only
  if both still match; everything else is dropped. Clearing the query,
  editing it, or removing the token starts a new generation. In-flight HTTP
  requests are not cancelled; they finish and are ignored.
- **Credential and secret-store calls** run on the same pool: Secret Service
  on Linux may block on an unlock prompt.

## Alternatives considered

- **`reqwest` + `tokio`**: the usual async stack and true cancellation, but
  about twice the dependency graph (tokio, hyper, h2, tower) for two
  requests per search. Cancellation is not needed for correctness because
  generations already discard stale results.
- **A thread per request**: simple, but unbounded under fast typing and
  retries. The fixed pool bounds threads and connections.
- **`slint::spawn_local` futures + an async client**: still needs an async
  HTTP stack and a runtime or executor bridge.
- **Debounce inside the worker (sleep, then check)**: blocks a worker per
  keystroke and needs cross-thread time. A UI timer is free.

## Consequences

### Positive

- Small graph, no runtime to start or shut down, and one mental model:
  worker thread → `upgrade_in_event_loop` → UI thread.
- Deterministic tests: the controller takes synthetic time and explicit
  responses; the client runs against a local fake server.

### Negative / risks

- A superseded request still uses a worker until it finishes (at most
  15 s). With 4 workers and a 300 ms debounce this does not starve new
  searches in practice.
- `webpki-roots` ignores certificates installed in the OS store, so
  TLS-inspecting corporate proxies fail with `Offline`. Switch to
  `rustls-platform-verifier` if that is reported.
- R8 poster downloads may want more concurrency; revisit then.

## Validation

Controller tests for debounce, whitespace, out-of-order responses, stale
pages and credential removal; a headless UI test that drives the real Slint
timer and worker pool against a fake server and checks the UI stays usable
while a request is pending.

## Revisit trigger

Bulk network work (poster downloads, library metadata refresh) that needs
many concurrent requests or real cancellation; proxy/TLS reports.
