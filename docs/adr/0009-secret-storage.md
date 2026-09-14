# ADR-0009: Secret storage: the OS credential store via `keyring`

- Status: Proposed
- Date: 2026-09-14

## Context

The TMDB API Read Access Token is a secret: it identifies the user's TMDB
account and its rate limit. It must never be stored in SQLite, a settings
file, logs, diagnostics, backups or Git, and must survive restarts. Bingee
runs on Windows, macOS and Linux desktops.

## Decision

- **Store**: the OS credential store through `keyring` 4 with its default
  `v1` feature: Windows Credential Manager, macOS Keychain, and the Secret
  Service (GNOME Keyring, KWallet) on Linux through the pure-Rust `zbus`
  client, so no system library is needed at build time.
- **Entry**: service `bingee-desktop`, account `tmdb-api-read-access-token`.
  One secret, no other data in it.
- **Abstraction** (`src/secrets.rs`): a `SecretStore` trait with `load`,
  `save`, `delete`. `KeyringStore` is the production implementation;
  tests use an in-memory store. Store calls run on the network worker pool
  (ADR-0008), never on the UI thread.
- **In memory**: the token is a `Token` newtype whose `Debug` prints
  `Token([redacted])` and which has no `Display`. The UI shows only its last
  four characters. The entry field is a password field and is cleared after
  a save.
- **No environment-variable override**: a development token can be saved
  through the normal UI. This removes one way to leak or mix up credentials.
- **Lifecycle**: a candidate token is validated remotely first and saved
  only if TMDB accepts it. Network or service failures during validation
  are reported as such and save nothing. Removing the token deletes the
  entry and disables only remote features; the database is untouched.

## Alternatives considered

- **Encrypted file or SQLite column**: the key must live somewhere; without
  the OS store this is obfuscation, not protection.
- **`keyring-core` plus individual store crates**: what `keyring` `v1` does
  internally, with more code to maintain here. Revisit if a store must be
  swapped (for example `linux-keyutils`).
- **Environment variable in development builds**: allowed by the brief, but
  not needed; skipped.

## Consequences

### Positive

- The secret is protected by the OS account and never touches Bingee's
  files. Removing it is one action.
- Tests never touch a real credential store.

### Negative / risks

- Linux needs a running Secret Service. Without one (minimal window
  managers, headless sessions), saving fails and Settings reports "secure
  storage unavailable". There is no plaintext fallback, by design.
- Linux may show an unlock prompt; that is why store calls are off the UI
  thread.
- macOS and Linux behavior is build-checked by CI only; only Windows was run
  locally, with an in-memory store in tests.
- The token is kept in ordinary process memory (no zeroing) while the app
  runs.

## Validation

Unit tests with the in-memory store for save, replace (only after
validation), remove, and the rule that invalid or unverifiable tokens are
never saved. Redaction tests for `Token`, client and error `Debug` and
`Display`.

## Revisit trigger

Reports of Linux sessions without a Secret Service, a second secret, or a
requirement to share credentials with Bingee Android.
