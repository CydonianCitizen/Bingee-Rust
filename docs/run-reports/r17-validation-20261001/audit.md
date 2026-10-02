# R17 correctness, failure and privacy evidence

Source: `candidate-identity.json`. Production code is the previously prepared
sidebar fix above `6e24b2a109c8fa041075adcbcf1001bc4646cd82`; this continuation
adds tests only. The default/all-feature/fixture gate logs name every executed
test. Ignored tests are not counted as executed by those gates.

## Data and failure matrix

| Case | Evidence and boundary |
| --- | --- |
| Fresh database and v1, v2, v3, v4 upgrades to v5 | `database::tests` exercises actual file databases, provider/personal row preservation, runtime backfill and unchanged reopen. All pass. |
| Migration failure/rollback | Separate v2, v3, v4 and v5 failing-step tests preserve the prior file and permit subsequent upgrade. General failed migration, corrupt/foreign/newer file preservation, foreign-key enforcement and integrity diagnostic tests pass. |
| Current Backup V1 | Empty/populated, movie/TV, partial coverage, removed metadata, identities, runtime snapshots, settings/events, history and statistics round trips pass. |
| Earlier and additive Backup V1 | New test removes R14 optional settings/events, adds unknown root/media fields, validates, restores a real file and reopens it. Existing fields retain their semantics. Defaults are off/empty for early V1. |
| Invalid backup / failed restore | Malformed JSON, future version, invalid media type, duplicate identity/event, invalid rating and dangling references are rejected; injected import failure rolls back all original rows. |
| Backup filesystem failure | Existing destination survives hard-link and fallback publication; interrupted fallback copy removes the partial destination; pre-restore safety copy matches prior state. |
| Credential failures | Missing, invalid and unavailable stores, failed save/remove and redacted errors have existing passing tests. No live credential was supplied to the deterministic tests. |
| HTTP failure | 401, 404, 429, server errors, malformed JSON, timeout/offline and stale answers have passing fake-server tests. Cached local data survives; refresh backoff and bounded selection pass. |
| Local write failure | Personal writes retain the committed UI state and show a notice; tracking bulk rollback and provider/personal firewall tests pass. |
| Startup / profile ownership | Corrupt-file retry and refusal tests pass. Existing same-production-source package evidence covers live contention and idle kill. Active-write process evidence is recorded separately in `active-crash.txt`. |
| Keyboard | Expanded sidebar tests reach all nine pages with Tab plus Enter and Space and return Home. Existing Library/detail/season/episode and Discover keyboard tests pass. These latter tests use an initial pointer action to focus their lists; they do not establish an entirely keyboard-only end-to-end journey. |

## Security and privacy sanity review

The review covers the current application boundary, not a penetration test or
a new vulnerability-database scan. No licensing decision is inferred.

- `secrets.rs` stores the real token only through the OS keyring adapter.
  Formatting redacts it, input is bounded to 1,024 bytes and rejects control
  characters/whitespace, and store failures do not fall back to plaintext.
- `tmdb.rs` fixes the production API root to HTTPS, uses the standard ureq TLS
  validation path, finite connection/global deadlines, a 2 MiB JSON-body limit
  and a 4 MiB image-body limit. Authorization is a header on API requests, not
  a query parameter; image requests receive no token. Provider configuration
  accepts an HTTP image base, so this review does not claim all image traffic
  is enforced HTTPS. No certificate validation bypass was found in app code.
- Personal tracking, ratings, watch history, restore and exports have no
  upload path. Remote search intentionally sends the entered query and
  provider requests send provider identifiers. Test fakes use loopback only.
- Poster keys restrict characters before filesystem joining. Production
  profile paths are absolute and platform policy tests pass. Cache cleanup
  preserves unknown/recent/referenced files and skips symlinks in its scoped
  cleanup traversal.
- Backup import uses typed JSON plus semantic validation and parameterized
  inserts. A 256 MiB metadata size check precedes reading; this is a bound on
  the checked file, not a guarantee against concurrent replacement/growth or
  a promise of constant-memory parsing. No backup-supplied filesystem path is
  executed or extracted. Poster paths remain provider metadata.
- Export uses exclusive temporary/final creation, sync, and no-overwrite
  publication. Restore uses a pre-restore safety copy and one transaction
  with integrity, foreign-key and row-count checks. Crash scope is stated in
  the run report; forced process termination is not a simulated power loss.
- Logs rotate locally at 1 MiB with one retained file. No new telemetry,
  credential logging, dependency, endpoint or production feature was added.

No new technical security defect was established by this bounded review.
Native keyring/desktop permission prompts and screen-reader behavior remain
outside the deterministic tests.

## Rendering and accessibility scope

Both existing offscreen page tests pass at 1280×800 and 1700×1100. The
1280×800 Settings/Calendar and 1700×1100 Home images were inspected. Home and
Calendar labels and controls are legible in those samples. Settings extends
below the viewport and uses its existing ScrollView; the image alone does
not prove that its lower controls are reachable with native keyboard input.
Test renders intentionally leave uninitialized storage/version text empty.
This is image inspection, not a live GUI session, high-DPI check or smoothness
claim. Source review confirms sidebar button roles/names and list selection
semantics. Native focus indication and screen-reader announcement are still
unverified.
