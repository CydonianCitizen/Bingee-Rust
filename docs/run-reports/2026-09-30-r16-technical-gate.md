# R16 technical distribution gate — 30 September 2026

## 1. Starting state and source identity

`main` and `origin/main` both point to `0c84fe2c33b5bd4f45d558a3e79e11caad6b19e0` (`R12 to R14`). R15/R16 source, scripts and documentation remain uncommitted in the working tree. No user changes were discarded. `git branch -vv` and the last 20 commits show no later local or remote commit. `.claude/settings.json` denies `git commit`, `git push` and `gh`; none was run. The current work therefore cannot have run in GitHub CI. A public Actions/API page fetch also failed in this environment, so older CI status was not established.

Starting milestone state: R15 local exit PASS, R16 PARTIAL, R17 NOT STARTED. Cargo version is `0.1.0-rc.1`; schema is v5. `Cargo.lock` SHA-256 is `677DE18AACBAAACB0A11DCC60054CCE2338E964D84030943808DF2ED5459CC79`. Windows host toolchain: rustc and Cargo 1.98.1, x86-64 MSVC. Frozen R4 SQLite SHA-256 `61918DB3B71A4B0F77154ABB498268F8D7F327469355B569AAD993B58C69FEC3` and poster manifest SHA-256 `4BE13BC484B971585C04CA4322629CECC837DB3BB823715FE72F69C1AFD0978C` still match the benchmark contract.

## 2. Windows package result

`scripts/package-windows.ps1 -RequireInstaller` rebuilt the release executable, portable ZIP and Inno Setup installer from the current working tree. The binary hash matched the previous report. ZIP and installer hashes changed although sizes did not; package timestamps make those archives non-reproducible byte for byte. Fresh artifact hashes below are authoritative for this run.

`scripts/smoke-windows-package.ps1` passed fresh launch, reopen without migration, live second-instance refusal, idle hard-kill restart, corrupt-DB preservation, fixture flag refusal and package immutability. `scripts/smoke-windows-installer.ps1` passed isolated install, launch, reinstall, uninstall and schema-v5 profile retention. Its first sandboxed attempt exited 4 because per-user registry writes returned `RegCreateKeyEx failed; code 5`; the permitted registry-capable rerun passed. These are process/file smoke checks, not visual inspection. No nonempty upgrade profile was used in this rerun.

## 3–6. macOS, Linux, CI and artifact manifests

The macOS `.app` and Linux tarball have **not** been built on their native runners. No architecture, plist, executable, runtime or package hash is claimed for them. The release workflow now runs on push to `main` or manual dispatch. Each OS job runs normal format/check/tests, builds its package, validates structure and creates `dist/PACKAGE-MANIFEST.json` with platform, filename, type, architecture, version, bytes, SHA-256, commit and toolchain. macOS checks bundle files, `Info.plist`, identifier, numeric version, Mach-O architecture, linked libraries and archive structure. Linux checks ELF, desktop entry, icon, license material, dynamic dependencies, archive and virtual-display startup with an isolated profile. Windows checks installer, ZIP and package files. Package artifacts are uploaded; no GitHub Release is created. This workflow remains **unrun** for the working tree.

| Platform | Artifact filename | Type | Architecture | Version | Bytes | SHA-256 | Build commit and toolchain |
| --- | --- | --- | --- | ---: | ---: | --- | --- |
| Windows | `bingee-desktop-windows-x64-setup.exe` | Inno Setup installer | x86-64 | 0.1.0-rc.1 | 10,019,794 | `CBD3B12331A27F0552B85F4B59B39E8298D1EB07CE270AE6AE5AD579DE6B3015` | `0c84fe2` plus uncommitted work; rustc 1.98.1 |
| Windows | `bingee-desktop-windows-x64.zip` | portable ZIP | x86-64 | 0.1.0-rc.1 | 13,728,171 | `F5070E8CA281CD86CD61EDA2143A53709A7CB45849A496F92BA94BED2D28EC4E` | same |
| Windows | `bingee-desktop.exe` | main executable | x86-64 | 0.1.0-rc.1 | 28,831,232 | `8914A8801C3D79C010F4C7D452AC85FF059ED620F73AED76777B993B8A6012C3` | same |
| macOS | `bingee-desktop-macos-<arch>.tar.gz` containing `Bingee Desktop.app` | bundle archive | pending native runner | 0.1.0-rc.1 planned | pending | pending | pending CI |
| Linux | `bingee-desktop-linux-<arch>.tar.gz` | executable tarball | pending native runner | 0.1.0-rc.1 planned | pending | pending | pending CI |

Windows package folder totals 32,823,593 bytes in 697 files, including 387 crate license directories and notices for 325 linked crates. Windows needs the Microsoft Visual C++ 2015–2022 Redistributable x64, which is not bundled. The macOS artifact is unsigned and unnotarized; Gatekeeper/public distribution limitations remain documented. Normal GUI launch on a hosted macOS runner is not yet proven. Linux runtime assumptions are in its package README; the configured virtual-display smoke has not run.

## 7–10. Engineering licensing review

The [Slint Royalty-free Desktop, Mobile, and Web Applications License 2.0](https://slint.dev/terms-and-conditions) allows the `AboutSlint` attribution widget in an About screen accessible from top-level navigation. Bingee's sidebar About page contains that widget and package material includes Slint license files. This engineering implementation matches that stated attribution condition. Paid and GPL alternatives remain owner choices.

The [TMDB FAQ](https://developer.themoviedb.org/docs/faq) describes free non-commercial developer use with attribution. Bingee's About page contains the approved logo and required non-endorsement notice. TMDB directs commercial users to obtain separate licensing. Bingee's intended-use classification is unresolved.

The [Inno Setup download page](https://jrsoftware.org/isdl.php) requests a purchased license for commercial use. The repository uses version 6.7.3. No purchase or classification is inferred.

The Windows dependency-license inventory was generated from locked Cargo metadata: 387 resolved crate/version entries, each with license material, and 325 normal linked crates in notices. It includes transitive and build-only crates, possibly more than the executable ships. `licenses/INDEX.txt` maps crate/version to declared license expression, while each crate directory contains source-supplied or standard text. Native inventories need package verification. See `docs/release-licensing.md` for the decision matrix. No engineering conclusion of legal compliance is made.

## 11. R16 technical gate

**PARTIAL.** Windows package build and smoke PASS. macOS package build, Linux package build and current-tree cross-platform normal build/tests are unverified. Required package structures have not all been validated. Signing/notarization and external licensing are separate public-distribution concerns. The technical gate cannot close; R17 does not start.

## 12–30. R17 technical validation

R17 remains **NOT STARTED** under the explicit technical gate. The R15 headless debug soak and Windows package smoke do not substitute for release-candidate validation. No R17 candidate revision is frozen; no R17 feature freeze or benchmark identity is claimed.

| Report item | Current result |
| --- | --- |
| 12 Candidate identity | NOT RUN: no gated R17 candidate. Base commit and lock hash above identify current R16 work only. |
| 13 Full correctness gate | NOT RUN as R17. This turn: `cargo fmt --check`, `cargo check --locked`, `cargo test --locked --quiet` PASS: 214 passed, 13 ignored; Windows release build PASS. Prior R15 report records all feature/clippy gates, not a final RC run. |
| 14 Migration chain | NOT RUN as R17; existing migration tests remain in the 214-test suite. |
| 15 Early/current Backup V1 | NOT RUN as R17; existing backup tests remain in the suite. |
| 16 Comprehensive fake-TMDB journey | NOT RUN. |
| 17 Formal startup | NOT RUN. Windows smoke records individual window-ready times, but not a defensible release distribution or R4-equivalent proxy. |
| 18 Idle CPU | NOT RUN. |
| 19 Memory baseline | NOT RUN. |
| 20 Soak and cache occupancy | NOT RUN in release profile. R15's 100-cycle debug soak remains historical evidence only. |
| 21 Thread/resource stability | NOT RUN as R17. |
| 22 Local-write latency | NOT RUN. R10 synchronous-write decision remains unreassessed. |
| 23 Page performance | NOT RUN. |
| 24 Refresh stress and query plans | NOT RUN. |
| 25 Backup performance and failure paths | NOT RUN as R17. R15 fallback tests remain historical evidence. |
| 26 Crash and multi-instance regression | Windows idle-kill and live second-instance smoke PASS; active refresh/export kill and other final-platform cases NOT RUN. |
| 27 Rendering and high-DPI artwork | NOT RUN; no visual inspection claimed. |
| 28 Keyboard and accessibility | NOT RUN as final audit. |
| 29 Failure matrix | NOT RUN as final matrix; Windows corrupt-DB and second-instance smoke PASS. |
| 30 Security and privacy | NOT RUN as final audit; no penetration test or universal privacy claim. |

## 31–35. Footprints, limits and final classification

Windows footprint is in the manifest table; macOS and Linux sizes remain unknown. The current known-limitations classification is in `docs/release-checklist.md`. Exact technical blocker: unpushed R15/R16 working tree prevents native CI execution, so macOS and Linux packages and cross-platform tests have no current-tree result. Exact public-distribution blockers: application license and intended-use choice, any applicable TMDB commercial agreement and Inno Setup purchase, plus unfinished technical and R17 validation.

**Technical RC status: NOT TECHNICALLY READY FOR RC.**

**Public distribution status: PUBLIC DISTRIBUTION NOT READY.** External licensing decisions remain pending separately. No release was published.

## Verification and Git plan

This turn: Windows release package PASS; portable smoke PASS; installer smoke PASS after sandbox registry denial; `cargo fmt --check` PASS; `cargo check --locked` PASS; `cargo test --locked --quiet` PASS (214/13); `git diff --check` PASS with line-ending warnings; R4 hashes PASS. Python YAML parsing attempt did not run because `yaml` is not installed; workflow was reviewed as text and remains unexecuted.

The logical history unit is one coherent R15/R16 commit containing current source, Cargo files, scripts, workflow, license material, UI, notices and documentation. An authorized human can review the full diff, stage it, run `git diff --cached --check`, commit, then push `main`. The push now starts both normal cross-platform CI and the three-package release-artifacts workflow automatically. Inspect every job and its uploaded manifest before marking R16 technical PASS. If all five technical conditions pass, proceed to R17 without waiting for external licensing decisions. Do not publish a release.
