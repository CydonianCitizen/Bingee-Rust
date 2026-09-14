# R5 packaging and cross-platform report

Date: 13 September 2026 (unattended session). Machine and toolchain: the R4
Windows machine (see `../R4-rust-slint/environment.md`), rustc and cargo 1.98.1,
`x86_64-pc-windows-msvc`, Slint 1.17.1, rusqlite 0.40.2 with bundled SQLite
3.53.2. Source: commit `95feb454864907a29bcf84f629278dc861e10336` plus the
uncommitted R4/R5 working tree (commits were denied in this session).

This is a deployment report, not a benchmark. Sizes are disk bytes. No
startup, memory or latency number here is a `BENCHMARK_SPEC.md` measurement.

## Verification level per OS

| OS | Level | Evidence |
| --- | --- | --- |
| Windows 11 x64 | **Verified locally** (build, tests, Clippy, release build, package, process-level launch) | `raw/gate-20260913/`, `raw/package-20260913/`, `raw/smoke-20260913T221112Z/` |
| Linux (Ubuntu) | **Source-audited only** | CI job defined, never run. The local WSL Ubuntu failed to start (`Wsl/Service/CreateInstance/MountDisk/HCS/ERROR_FILE_NOT_FOUND`, its virtual disk is missing) |
| macOS (arm64) | **Source-audited only** | CI job defined, never run |

No OS is "build-verified in CI": the workflow never ran, because this session
could neither commit nor push. There was no visual verification on any OS. The
Windows launch check uses process APIs (window handle, title, responsiveness,
exit code) and stderr.

## Package

`scripts/package-windows.ps1` produces `dist/bingee-desktop-windows-x64/`
(git-ignored):

```text
bingee-desktop-windows-x64/
├── bingee-desktop.exe          16,930,816 bytes
├── assets/posters/             100 JPEGs, 2,188,318 bytes, + SHA256SUMS (8,100)
├── data/                       empty; bingee-spike.db (196,608) on first launch
├── THIRD_PARTY_NOTICES.txt     15,727 bytes (curated header + 301 linked crates)
├── README.txt                  1,587 bytes
└── SHA256SUMS.txt              10,048 bytes (every other file)
```

| Item | Value |
| --- | --- |
| Package as built | 19,154,596 bytes in 105 files |
| After first launch | 19,351,204 bytes (adds the seeded database) |
| Executable SHA-256 | `D65E7E5E381D5E5A8253A5E50B6DCEE2B3E46A79C28BD75420F35B4A3A7C8801` |
| Executable vs frozen R4 binary | +3,584 bytes (16,927,232 before): the R5 path code and the app-ID call. A fact about disk size, not a footprint conclusion |
| Repeatability | Two assemblies from the same build produced identical `SHA256SUMS.txt`. A clean rebuild changes the executable's bytes (PDB GUID and timestamps, as R4 documented), so the package is repeatable, not bit-reproducible |

Not packaged: source, `target/` intermediates, the PDB, raw measurements, Git
metadata, and any secrets (none exist). No zip, installer or signature.

## Resource lookup and data policy

ADR-0005. The root is `std::env::current_exe()`'s directory. Posters:
`<root>/assets/posters/` if present, else the checkout's
`benchmark/assets/posters/` (build-tree binaries only). Database:
`<root>/data/bingee-spike.db`, `data/` created on launch. Paths are built with
`Path::join`, with no separators hard-coded per OS. The working directory is
never consulted. **This is a portable-spike packaging policy, not necessarily
the final production data-directory policy.**

The unit test `package_paths_derive_from_the_executable_directory` checks the
root, the fallback, the priority of packaged assets, and `data/` creation and
opening.

## Windows smoke test

`scripts/smoke-windows-package.ps1 -WorkDir <scratch dir outside the repo>`,
raw result `raw/smoke-20260913T221112Z/smoke.json`:

| Check | Result |
| --- | --- |
| Packaged exe started with an unrelated working directory | Window `Bingee Desktop` appeared and was responsive; graceful close, exit code 0 |
| Database created inside the package | `data\bingee-spike.db`, 196,608 bytes |
| Nothing written to the working directory | Confirmed |
| stderr | Empty (no missing-poster or database errors) |
| Posters come from the package | A copy without `assets\posters\poster-001.jpg` logged exactly `Poster could not be loaded, showing placeholder: <copy>\assets\posters\poster-001.jpg`, then exited 0 |
| Fresh seed vs frozen R4 database | Identical SHA-256 `61918DB3B71A4B0F77154ABB498268F8D7F327469355B569AAD993B58C69FEC3` |

The last row means one launch of this build regenerates the exact R4 contract
database on this toolchain and platform. It was not checked on other SQLite
versions or OSes.

The smoke test also recorded time-to-window (1,304 and 1,319 ms) and process
memory. These are not startup or memory measurements: a different endpoint,
50 ms polling, a first launch of a new executable file (plausibly including an
antivirus scan, not verified), and in the first run seeding a new database.
Do not compare them with R4.

## Windows console and runtime dependencies

PE inspection (`raw/package-20260913/pe-inspection.txt`): release
`bingee-desktop.exe` uses subsystem `WINDOWS_GUI` (no console window); the
debug build uses `WINDOWS_CUI`, so `cargo run` keeps stderr visible. This comes
from the pre-existing
`#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]`, which was
not duplicated.

Imports: Windows system DLLs, the Universal CRT (`api-ms-win-crt-*`, part of
Windows 10/11), `OPENGL32.dll`, and **`VCRUNTIME140.dll`**. The last one comes
from the Microsoft Visual C++ 2015–2022 Redistributable, not from Windows. The
package README states it. A static CRT or app-local runtime was not adopted:
either would change the release build relative to the R4 baseline, and
app-local deployment needs its own license review.

## Cross-platform source audit

| Area | Finding |
| --- | --- |
| Paths | `std::env::current_exe` + `Path::join`; no drive letters, no `\` literals, no working-directory use. The build-tree fallback uses `CARGO_MANIFEST_DIR` (a compile-time path), only when the package folder is absent |
| Windows-only APIs | None in `src/`, `ui/` or `build.rs`. The only Windows item is the `windows_subsystem` attribute, which other targets ignore |
| Shell / processes / environment | None at run time. Tests use `std::env::temp_dir` |
| Filesystem | Creates `<root>/data/`; needs a writable package folder. SQLite handles file locking per OS |
| Keyboard | Slint `Key.*` values only. On macOS, F12 (debug dump) may be taken by the system; laptop keyboards reach Home/End/PageUp/PageDown through Fn |
| Fonts | System fonts via Slint/fontique; nothing embedded for desktop targets |
| Linux build | Only fontconfig is linked at build time (`yeslogic-fontconfig-sys` through pkg-config). `wayland-sys` (feature `dlopen`), `x11-dl`, `xkbcommon-dl` and EGL/GLX are loaded at run time; `x11-dl`'s pkg-config lookups are optional |
| macOS build | No extra system packages (`objc-sys`, `core-foundation-sys`, the Xcode CLT compiler for SQLite) |
| Benchmark scripts | `benchmark/scripts/*.ps1` and `scripts/*.ps1` are Windows PowerShell by design and not part of the application |

## Slint backend and renderer

The R4 Windows baseline measured `FemtoVG renderer with OpenGL backend` on the
winit backend. Enabled Slint features are unchanged: default, accessibility,
backend-default, compat-1-2, renderer-femtovg, renderer-software, std,
system-tray. On Linux (Wayland/X11 through EGL/GLX) and macOS, Slint's default
selection is expected to be the same winit + FemtoVG/OpenGL pair, but this was
**not verified**, and R5 does not force any backend. Environments without
OpenGL 2.0 (some VMs, Remote Desktop) were not tested. The package README
mentions `SLINT_BACKEND=winit-software` as an untested fallback.

## Application identity

Window title `Bingee Desktop` (unchanged); Cargo package and executable
`bingee-desktop`; package folder `bingee-desktop-windows-x64`; XDG app ID
`bingee-desktop` (Wayland app_id and X11 WM_CLASS). A reverse-DNS ID can
replace it once the product has a domain. No icon; branding is unchanged.

## CI

`.github/workflows/cross-platform.yml`: triggered on pushes to `main`, pull
requests and manual runs; read-only permissions; `fail-fast: false`; matrix
`windows-latest`, `ubuntu-latest`, `macos-latest`. The steps are rustup
stable with rustfmt and clippy; on Ubuntu, `pkg-config` and
`libfontconfig-dev`; then `cargo fmt --check`, `cargo check --locked`,
`cargo test --locked`,
`cargo clippy --locked --all-targets --all-features -- -D warnings` and
`cargo build --release --locked`. There is no `continue-on-error` and no cache.

Validation was structural only: it was reviewed by hand and contains no tabs.
No YAML parser was available offline (no PyYAML, Node YAML module, Ruby or Perl
YAML). **It has never run.** `origin` is
`https://github.com/CydonianCitizen/Bingee-Rust.git` with only `main`
(`95feb45`), and this session's settings deny `git commit`, `git push` and
`gh`.

## Licensing and attribution

`THIRD_PARTY_NOTICES.txt` (repository root; the packaged copy also lists all
301 linked crates for `x86_64-pc-windows-msvc` with their license
expressions):

- Slint 1.17.1: GPL-3.0-only OR Slint Royalty-free 2.0 OR Slint Software 3.0.
  The spike assumes Royalty-free 2.0 for evaluation only, and no commercial
  path was taken. Its attribution condition (AboutSlint in an About screen, or
  the badge on a public download page) is **not** met by this build, which
  has neither. The package must therefore not leave the team.
- SQLite: public domain. rusqlite and libsqlite3-sys: MIT.
- All other linked crates are permissive: MIT, Apache-2.0, BSD, Zlib, ISC,
  BSL-1.0, Unicode-3.0, CC0, Unlicense, 0BSD. Their full license texts are not
  bundled yet (for example with cargo-about); that is required before external
  distribution.
- Posters: generated by this project; no third-party material.

## Commands and outcomes

| Command | Outcome |
| --- | --- |
| `rustc --version` | `rustc 1.98.1 (48a229cea 2026-09-01)` |
| `cargo --version` | `cargo 1.98.1 (797e8a9bc 2026-08-05)` |
| `cargo fmt --check` | exit 0 |
| `cargo check` | exit 0 |
| `cargo test` | exit 0: 31 passed, 0 failed, 2 ignored (the informal R2/R3 timing tests) |
| `cargo clippy --all-targets --all-features -- -D warnings` | exit 0, no warnings |
| `cargo build --release` | exit 0 |
| `pwsh -NoProfile -File scripts/package-windows.ps1` (twice) | exit 0 both times; identical `SHA256SUMS.txt` |
| `pwsh -NoProfile -File scripts/smoke-windows-package.ps1 -WorkDir <scratch>` | exit 0; all checks above passed |

Logs: `raw/gate-20260913/gate-0.txt` … `gate-6.txt` and `gate.txt`. The
missing/corrupt-poster errors in the test log are expected output of the
fallback test.

## Unresolved platform limitations

- The Linux and macOS jobs have never run: the next push must confirm them.
- `VCRUNTIME140.dll` is required on Windows (Visual C++ Redistributable).
- The package folder must be writable (not `C:\Program Files`).
- Unsigned executable: SmartScreen on Windows, Gatekeeper on macOS for
  downloaded builds. No macOS `.app` bundle or Linux `.desktop` file.
- The OpenGL requirement and the software-renderer fallback are untested.
- Full third-party license texts are not bundled, and the Slint attribution
  condition is unmet. Both must be fixed before any external distribution.
- The R4 interactive items remain environment-blocked; see
  `../R4-rust-slint/STATUS.md`.
