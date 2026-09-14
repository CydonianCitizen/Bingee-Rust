Bingee Desktop - portable Windows x64 spike package
====================================================

Internal technology-evaluation build of the Rust + Slint implementation.
Not a product release. Do not distribute outside the team (see
THIRD_PARTY_NOTICES.txt, Slint attribution).

Run
  Start bingee-desktop.exe from this folder: double-click it, or run it from
  any shell and any working directory. No installation and no console window.

Layout
  bingee-desktop.exe      the application
  assets\posters\         100 synthetic benchmark posters (read-only)
  data\                   the library database, bingee-spike.db, created and
                          seeded with 1,000 records on first launch
  THIRD_PARTY_NOTICES.txt licenses and attribution
  SHA256SUMS.txt          checksums of the files as packaged

Data policy
  Everything stays inside this folder, so the folder must be writable.
  Delete data\bingee-spike.db to reseed. This is a portable-spike policy,
  not the production data-directory policy.

Requirements
  Windows 10 or 11, x64.
  Microsoft Visual C++ 2015-2022 Redistributable (x64), for VCRUNTIME140.dll.
  Most machines already have it.
  OpenGL 2.0 or newer (default FemtoVG renderer). If the window stays blank,
  for example in a VM or over Remote Desktop, set SLINT_BACKEND=winit-software
  to try Slint's software renderer (not tested for this spike).

Build
  Source commit 95feb454864907a29bcf84f629278dc861e10336 plus uncommitted changes
  rustc 1.98.1 (48a229cea 2026-09-01), cargo build --release --locked, target x86_64-pc-windows-msvc
