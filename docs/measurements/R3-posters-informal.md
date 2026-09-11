# R3 — informal poster pipeline observations

**Informal.** These are R3 observations, not the R4 benchmark, and they are
not comparable to the Avalonia spike yet. They exist to decide whether
synchronous poster decoding is good enough and whether the poster cache keeps
memory bounded (R3 brief §7, §12, §13, §14).

## Environment

| | |
| --- | --- |
| OS | Windows 11 Home 10.0.26200, 64-bit |
| CPU | Intel Core i5-10500H @ 2.50 GHz, 6 cores / 12 threads |
| RAM | 23.8 GB |
| GPU / display | Intel UHD Graphics (integrated, drives the display) + GeForce GTX 1650 Max-Q; 1920×1080 @ 60 Hz |
| Renderer | Slint default on this machine: winit + femtovg (OpenGL) |
| Build | `release` profile (Cargo defaults: no LTO, no strip), except where debug is stated |
| Toolchain | rustc 1.98.1, Slint 1.17.1 (`image` 0.25.10, `zune-jpeg` 0.5.15 inside Slint), rusqlite 0.40.2 |
| Commit | uncommitted R3 working tree on top of `956cbb5` (R2) |
| Debugger/profiler | none attached; normal desktop session with other apps open |

## Assets

100 synthetic baseline JPEGs, 240×360, RGB, quality 85, 20.5–23.5 KB each.
See `benchmark/assets/posters/README.md`. Each decodes to an RGB8 buffer of
259,200 bytes. The cache budget counts 345,600 (×4 B/px, a deliberate
over-estimate).

## Decode / load latency

**Method:** `cargo test --release informal_poster_timings -- --ignored --nocapture`
(`src/main.rs`), timing each call with `std::time::Instant`, in 3 separate
processes. The OS file cache is warm, because the files were read recently.

- *cold*: the first `slint::Image::load_from_path` of each of the 100 files
  in the process, which reads and fully decodes the file.
- *repeated*: 500 more loads cycling through the 100 files. Their ~25 MB of
  RGB8 overflows Slint's internal 5 MiB cache, so every load decodes again.
  This is a forced re-decode.
- *Slint cache hit*: the same path loaded again straight away. Slint serves it
  from its internal cache after a `stat` of the file.
- *PosterCache hit*: our cache, full with 36 entries, cycling through all of
  them. Worst-case linear scan, no file access.

All times in µs.

| Run | Case | n | min | median | p95 | max |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 1 | cold decode | 100 | 1115.5 | 1349.4 | 1930.5 | 2246.9 |
| 1 | repeated decode | 500 | 897.2 | 1228.6 | 1641.1 | 2188.5 |
| 1 | Slint cache hit | 500 | 44.1 | 46.9 | 71.3 | 937.7 |
| 1 | PosterCache hit | 500 | 0.1 | 0.1 | 0.2 | 0.9 |
| 2 | cold decode | 100 | 1067.8 | 1266.2 | 1889.9 | 2176.4 |
| 2 | repeated decode | 500 | 890.9 | 1046.4 | 1632.0 | 2307.8 |
| 2 | Slint cache hit | 500 | 45.5 | 47.7 | 66.1 | 931.5 |
| 2 | PosterCache hit | 500 | 0.1 | 0.1 | 0.2 | 1.1 |
| 3 | cold decode | 100 | 1031.5 | 1227.7 | 1772.5 | 1883.2 |
| 3 | repeated decode | 500 | 926.0 | 1101.8 | 1631.9 | 2110.3 |
| 3 | Slint cache hit | 500 | 45.7 | 48.5 | 71.8 | 977.7 |
| 3 | PosterCache hit | 500 | 0.1 | 0.1 | 0.2 | 1.0 |

The app's own counters (F12 dump, below) agree: in the running release app, a
miss averaged 0.87–1.15 ms and the worst miss was 2.45–3.01 ms across the
scripted runs.

**Debug build:** in the headless test (`cargo test`, unoptimized
dependencies), a load averaged about 36 ms and peaked at 43–58 ms. That is 30×
slower than release. A PageDown in a debug build loads about 8 posters in one
go (about 0.3 s). This was not observed interactively.

## Scripted memory observation

**Script:** `benchmark/scripts/r3-memory.ps1 -ScrollPasses 4` (PowerShell,
built-in cmdlets plus WScript.Shell `SendKeys`). It launches the release exe
and focuses the list (Tab, Down), then records:

- **A** startup/idle, 5 s after launch;
- **B** initial library, after Down×3/Up×3;
- **C1–C4** after each scroll pass, sampled at the end of the list. One pass is
  130×PageDown, Home, End, 130×PageUp, End: about 2,000 rows, 20× the poster
  pool;
- **D** after Home and a 10 s settle.

**Metrics:** `WorkingSet64` (working set) and `PrivateMemorySize64` (private
bytes) from `Get-Process`, 5 s after the last key. Cache counters come from
pressing F12, which makes the app print its cache state to the redirected
stderr. Keys are sent 40 ms apart.

### R3 (with posters): 3 valid runs

WS / private in MiB.

| Point | Run 2 | Run 3 | Run 4 |
| --- | --- | --- | --- |
| A startup/idle | 126.5 / 166.1 | 126.6 / 165.8 | 126.2 / 169.1 |
| B initial library | 124.2 / 166.7 | 124.3 / 166.5 | 126.4 / 169.2 |
| C1 after pass 1 | 148.2 / 194.5 | 148.8 / 194.3 | 147.6 / 194.1 |
| C2 after pass 2 | 147.7 / 194.4 | 148.9 / 194.3 | 147.9 / 194.2 |
| C3 after pass 3 | 147.9 / 194.3 | 148.8 / 194.2 | 148.1 / 194.3 |
| C4 after pass 4 | 147.9 / 194.3 | 148.3 / 194.1 | 147.8 / 194.7 |
| D back at top | 147.9 / 194.3 | 148.3 / 194.1 | 147.8 / 194.7 |
| Peak WS | 150.0 | 151.0 | 149.9 |

The cache counters were identical in all three runs, because the key sequence
is deterministic:

| Point | Entries | Est. bytes (MiB) | Hits | Misses | Evictions | Failures |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| A | 9 | 3,110,400 (2.97) | 1 | 9 | 0 | 0 |
| B | 9 | 3,110,400 (2.97) | 7 | 9 | 0 | 0 |
| C1 | 36 | 12,441,600 (11.87) | 297 | 1,992 | 1,956 | 0 |
| C2 | 36 | 12,441,600 (11.87) | 578 | 2,992 | 2,956 | 0 |
| C3 | 36 | 12,441,600 (11.87) | 859 | 3,992 | 3,956 | 0 |
| C4 | 36 | 12,441,600 (11.87) | 1,140 | 4,992 | 4,956 | 0 |
| D | 36 | 12,441,600 (11.87) | 1,150 | 4,992 | 4,956 | 0 |

The budget is 12,582,912 bytes (12.00 MiB). At D the top rows are hits
because the last pass had just passed them. The test
`only_visible_posters_load_and_the_cache_stays_bounded` covers returning
to rows whose posters were evicted, which reload.

Two more runs are not in the tables above:

- **Invalid run:** no keystrokes reached the window, so there was no F12
  output and no memory change. It is discarded. The script now waits 1 s
  before sending keys and warns when F12 output is missing.
- **Exploratory run:** an earlier version of the script, with 2 passes and
  only one C sample. Its values were consistent with the runs above: A
  126.3/166.1, B 125.9/168.3, C 147.0/193.1, D 147.8/193.8.

### R2 baseline (no posters), same script, 2 runs

R2 at commit `956cbb5` was built release in a temporary worktree and driven by
the same script. There is no poster cache, so F12 does nothing.

| Point | Run 1 | Run 2 |
| --- | --- | --- |
| A startup/idle | 128.8 / 153.1 | 116.5 / 151.8 |
| B initial library | 130.2 / 154.6 | 118.0 / 153.3 |
| C1 | 138.0 / 163.3 | 125.7 / 162.1 |
| C2 | 138.0 / 163.3 | 125.7 / 162.1 |
| C3 | 137.9 / 163.2 | 125.6 / 162.1 |
| C4 | 138.6 / 164.0 | 126.3 / 162.8 |
| D | 138.6 / 164.0 | 126.3 / 162.8 |

### Reading

- **The explicit cache stays within its bound.** It holds at most 36 posters
  and 12,441,600 estimated bytes (11.87 of 12.00 MiB), through 4,992 loads and
  4,956 evictions. The actual RGB8 bytes it can hold are 36 × 259,200 =
  8.9 MiB. The unit tests and the headless GUI test assert the bound after
  every step.
- **The process plateaus.** Private bytes reach about 194 MiB after the first
  pass. Three more passes (3,000 loads) move them by at most 0.6 MiB. Working
  set behaves the same way. No growth without bound was seen.
- **Posters cost memory beyond the cache estimate.** Compared with R2 on the
  same script, private bytes are about 13–17 MiB higher at startup, with 9
  posters (2.2 MiB of actual pixels), and about 30–32 MiB higher after
  scrolling. Other known holders:
  - Slint's internal 5 MiB decoded-image cache;
  - GPU textures for visible posters; the display runs on an integrated GPU,
    whose driver memory can appear in the process;
  - the decoder's working buffers;
  - allocator retention.

  This split is **not measured**, since no profiler was used. R4 should
  attribute it before comparing with Avalonia.
- **Memory does not drop at D.** That is expected. The cache is still full at
  36 posters, and the OS working set does not shrink when the allocator keeps
  freed pages. The invariant that matters is the explicit bound above.

## Scrolling / frame-rate observation

**FPS run:** the same script with `-ScrollPasses 2 -Fps`, which sets
`SLINT_DEBUG_PERFORMANCE=refresh_full_speed,console`. The window then renders
continuously, and Slint prints the average frames per second once per second.
The run lasted about 60 s, including a 37 s scroll phase with 2,992 poster
loads. The per-second values were:

```text
40 60 60 60 60 59 60 60 61 60 60 60 58 60 60 60 60 60 60 60 60 60 60 60 60 60
61 60 61 61 60 60 60 60 60 60 60 60 60 61 60 61 60 60 61 60 60 60 60 60 60 60
60 60 60 60 60 60 60 60
```

The 40 is the first second, during startup. After that the rate holds at
58–61 (vsync at 60 Hz). A one-second average can hide a single late frame, so
this rules out sustained stalls, not every hitch.

**Direct human observation**, of the release build on this machine, with two
posters deliberately broken for the session: the user scrolled with the mouse
wheel (slow and fast, long stretches), held PageDown and PageUp, pressed
End/Home repeatedly, clicked rows, and searched and cleared. The user reported
**smooth, no hitching**. Poster numbers matched the rows; the missing and the
corrupt poster showed the placeholder; and the detail poster followed the
selection. Each broken poster was logged exactly once for the whole session.

A debug build was not observed interactively.

## Decision

Keep **synchronous decoding on the UI thread**.

- A release decode has a median of 1.0–1.35 ms and a worst sample of 2.3 ms.
  The worst burst, a jump that brings about 10 new rows into view, costs about
  11–13 ms, inside a 16.7 ms frame.
- Wheel scrolling brings in only 1–3 rows per frame.
- The FPS run shows no sustained drop, and direct observation found no hitch.

No worker thread, channel, or async runtime was added. Revisit this if posters
get larger, a grid view shows many more at once, or R4 finds late frames (see
ADR-0003).

## Binary footprint

`target/release/bingee-desktop.exe`, same path, toolchain and profile,
measured with `ls -l`:

| State | Bytes |
| --- | ---: |
| R2 | 16,833,536 |
| R3 | 16,927,232 |
| Difference | +93,696 (≈ +91.5 KiB, +0.56 %) |

The small growth suggests that most of the decoding code was already linked in
through Slint's `image` dependency. This was not checked at the symbol level.
The direct `image` entry is a dev-dependency, used only by the asset
generator, and it is not part of the release binary's normal dependency graph.
