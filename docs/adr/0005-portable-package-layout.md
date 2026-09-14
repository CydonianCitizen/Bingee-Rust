# ADR-0005: Portable package layout: assets and data beside the executable

- Status: Proposed
- Date: 2026-09-13

## Context

Before R5 the executable read posters from the checkout it was compiled in
(`CARGO_MANIFEST_DIR`) and kept its database beside itself. A copied executable
therefore showed placeholders, and the package had no defined data location.
R5 needs a Windows bundle that runs from any folder and any working directory.
Nothing in it may assume Windows paths, so the same rules must hold on macOS
and Linux. Constraints: no new dependency, no installer framework, no
production data-directory decision, and no change to the benchmark workload.

## Decision

Treat the directory containing the executable as the package root, and derive
every runtime path from it, never from the current working directory:

- posters: `<root>/assets/posters/`. If that folder does not exist (a build-tree
  binary under `target/`), fall back to `benchmark/assets/posters/` in the
  checkout the binary was built from;
- database: `<root>/data/bingee-spike.db`, with `data/` created on launch.

The posters stay files. They are not embedded in the executable.

## Alternatives considered

- **Embed the 100 JPEGs** (`include_bytes!`): about 2.2 MB more executable,
  a different binary from the one R4 measured, and the benchmark assets could
  no longer be swapped or checksummed as files. Rejected.
- **Per-user OS data directories** (`%LOCALAPPDATA%`, `~/Library/Application
  Support`, `$XDG_DATA_HOME`): the right model for the product, but it needs
  per-OS code or a `directories`-style crate. It also makes the spike bundle
  not self-contained and harder to reset. Deferred to the product.
- **Resolve from the current working directory**: breaks double-click and
  shortcut launches, and was the failure R5 exists to rule out.
- **No build-tree fallback** (package layout only): `cargo run`, the headless
  tests and the R3/R4 scripts would lose posters unless every build copied the
  assets into `target/`. Rejected. The fallback never applies when the package
  has its `assets/` folder.

## Consequences

### Positive

- The package is self-contained: copy the folder, run it from anywhere. Deleting
  `data/` resets it.
- One rule for all three operating systems, using only `std::env::current_exe`.
- Development keeps working without copying assets.

### Negative / risks

- The package folder must be writable. Under `C:\Program Files` or another
  read-only location, the library shows its load error instead of data.
- A build-tree binary still depends on its checkout path. Only the package is
  relocatable.
- On macOS, launching through a symlink resolves the root to the symlink's
  directory. A real `.app` bundle would need `Contents/Resources`. Neither is
  handled or tested.
- Development databases move from `target/<profile>/bingee-spike.db` to
  `target/<profile>/data/`. The frozen R4 database stays at the old path, used
  only by the retained R4 executables.

## Validation

- Unit test `package_paths_derive_from_the_executable_directory`.
- `scripts/smoke-windows-package.ps1`: the package started from an unrelated
  working directory creates `data/bingee-spike.db` inside the package, and a
  copy without `poster-001.jpg` reports that exact packaged path.
- CI builds and tests on Windows, macOS and Linux (`.github/workflows/cross-platform.yml`).

## Revisit trigger

Choosing the production data policy (per-user data directory, installer,
migrations or backups), shipping a macOS `.app` or Linux AppImage/Flatpak, or
any report of a read-only install location.
