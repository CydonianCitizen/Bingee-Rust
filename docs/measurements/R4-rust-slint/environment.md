# R4 environment — 11 September 2026

Source: `raw/20260911T192701561Z/environment.json`; measured on this machine,
not copied from R2/R3. The baseline executable and product sources are unchanged.

| Field | Recorded value |
| --- | --- |
| Branch / source | `spike/rust-slint`, `95feb454864907a29bcf84f629278dc861e10336` (Milestone 3) |
| Windows | Microsoft Windows 11 Home, 10.0.26200, build 26200, 64-bit |
| CPU | Intel Core i5-10500H @ 2.50 GHz, 6 cores / 12 logical CPUs |
| Physical RAM | 25,582,931,968 bytes (23.826 GiB) |
| Display GPU | Intel UHD Graphics; driver 31.0.101.2141 |
| Other GPU | NVIDIA GeForce GTX 1650 with Max-Q Design; driver 32.0.16.1692 |
| Display | 1920×1080, 60 Hz |
| Display scaling / DPI | **Not verified.** Registry has no LogPixels value; per-monitor DpiValue values are relative configuration, not an observed effective DPI. Do not assume 100%. |
| Window | Unchanged preferred 1280×800 logical client pixels. Actual client bounds were not observed; verify before accepting interaction comparisons. |
| Rust | rustc 1.98.1 (48a229cea 2026-09-01) |
| Cargo | cargo 1.98.1 (797e8a9bc 2026-08-05) |
| Target | x86_64-pc-windows-msvc |
| Slint / slint-build | 1.17.1 / 1.17.1 |
| rusqlite | 0.40.2, bundled SQLite |
| SQLite | 3.53.2, verified in resolved libsqlite3-sys sqlite3.h |
| Build | `cargo build --release`, unchanged Cargo defaults; no custom profile, LTO, strip, renderer or feature tuning |
| Renderer | **Measured diagnostic:** `FemtoVG renderer with OpenGL backend` |
| Window backend | winit expected from default Windows backend configuration; not independently named by the captured diagnostic |
| GPU execution | NVIDIA OpenGL module `nvoglv64.dll` is loaded. This suggests NVIDIA involvement, but no GPU engine attribution was collected. Intel drives the display; that does not prove Intel renders the app. |
| Debugger | Direct Start-Process launch; none attached by the suite. External debugger attachment not independently detected. |
| Defender | AntivirusEnabled=true; RealTimeProtectionEnabled=true at environment capture; not changed during the run |
| Power | AC online, charging, battery 58%; active scheme Balanced. Separate Windows power-mode slider not queried. No power settings changed. |
| Overrides | SLINT_BACKEND, SLINT_DEBUG_PERFORMANCE, SLINT_SCALE_FACTOR, RUSTFLAGS, CARGO_ENCODED_RUSTFLAGS absent in core run |
| Profiler | None in core run. Separate `refresh_lazy,console` diagnostic run retained separately. |
| Optional tools | WPR at `C:\WINDOWS\system32\wpr.exe`; VMMap/WPA not found on PATH or checked Downloads/Desktop/toolkit paths. A broad Program Files scan was stopped; this is not proof of absence everywhere. No tools installed. |

Executable: 16,927,232 bytes; SHA-256
`AEAF7F294E5D829CF2F693E768F79BC8AF6A1BB0416954296965F78218B9F35E`.
Database and asset hashes are in the [shared workload contract](../../../benchmark/R4-workload.md).
The core suite rechecked executable and database hashes after measurements.

The desktop automation helper failed on initial call, retry, and reset/retry:
`Computer Use native pipe is unavailable: failed to connect native pipe: The system cannot find the file specified. (os error 2)`.
Process APIs can launch and sample the app, but this does not supply a visual
observation or reliable input automation session. No screenshot or motion claim
is made. DPI and actual client geometry remain comparison prerequisites.

Initial sandbox WMI/Defender queries returned `Access denied`; the environment
record above was obtained through approved read access. No security setting changed.

## Dependency inventory

- Runtime: `slint` 1.17.1; `rusqlite` 0.40.2 with `bundled`.
- Build: `slint-build` 1.17.1.
- Dev: `image` 0.25.10, default features disabled, `jpeg` enabled, used by the
  deterministic asset generator. No dependency added in R4.
- Windows target graph: 330 unique package identities including the application
  (329 dependencies), counting runtime/build edges. Adding dev edges yields the
  same unique count because the resolved image package is already present.
  This is a resolved package count, not a count of DLLs or bytes linked into the exe.
- Major enabled Slint features: default, accessibility, backend-default,
  compat-1-2, renderer-femtovg, renderer-software, std, system-tray.
  Raw cargo-tree output is preserved in the run directory.

## R4 continuation: separate latency environment

`raw/latency-20260911T201032397Z/environment.json` records the new latency run
on the same OS/CPU/toolchain, AC online, Balanced plan and Defender enabled.
The feature `r4-measurement` affects only this application's opt-in harness;
Slint features, dependency versions, renderer selection and workload inputs
remain unchanged. The latency executable SHA-256 is
`DB1CFDEB8A47CCD4AB28B89A4ACC59DF15DC63FE1300E751F71CDB39D6DE67E3`.
It is distinct from the original binary, whose valid core results remain intact.

All three GUI callback runs report a 1280×800 physical client size and Slint
scale factor 1, hence 1280×800 logical pixels. See `ui-1.geometry.txt` through
`ui-3.geometry.txt` in that run directory. This verifies application-reported
client geometry and effective scaling for these latency runs only; monitor DPI
was not independently queried. Future frozen keyboard runs must verify their
own geometry and focus.

Computer Use still returned the missing-native-pipe error after initial call,
retry and reset/retry. No continuous visual observation or keyboard memory/soak
run occurred. Programmatic callbacks do not resolve that limitation.
No profiler, debugger, VMMap installation or system-setting change was made.
