# R16 native revalidation — 1 October 2026

**R16 technical gate: PASS.** This supersedes the earlier unpushed-fix
snapshot in `2026-10-01-r16-revalidation.md`; historical records remain intact.

## Source identity

`main`, HEAD, `origin/main`, and public GitHub main agree on
`6e24b2a109c8fa041075adcbcf1001bc4646cd82`. The working tree was clean at
entry. This commit contains the canonical-directory assertion, Linux X11
runtime prerequisite, package README and associated documentation/reports.
No Git write operation was performed by this validation continuation.

Both actual workflows completed successfully for that exact revision:

- [Normal CI 36785367532](https://github.com/CydonianCitizen/Bingee-Rust/actions/runs/36785367532)
- [Native packaging 36785367485](https://github.com/CydonianCitizen/Bingee-Rust/actions/runs/36785367485)

Each job and step was inspected independently. Each platform's normal tests
passed **214 tests, 13 ignored**. Native package jobs passed default and
all-feature tests with the same counts. All native release builds passed.

| Platform | Package verification | Process/runtime verification | R16 gate |
| --- | --- | --- | --- |
| Windows x64 | Installer, portable ZIP, PE GUI x64, resources/notices, all 696 portable checksums, version and manifest PASS | Exact downloaded CI payload: fresh/reopen, live lock contention, idle hard kill/restart, corrupt-file preservation and default fixture refusal PASS. Installer install/launch/reinstall/uninstall/data retention PASS | PASS |
| macOS arm64 | Native `.app`, Mach-O arm64, executable permission, Info.plist lint, bundle ID, numeric versions, icon/notices/licenses, linked-library inspection, archive and manifest PASS | Structural executable verification PASS; native launch and interactive GUI NOT PERFORMED (not configured by this workflow) | PASS |
| Linux x86_64 | Native ELF, executable permission, desktop entry validation, icon/notices/licenses/install scripts, archive and manifest PASS | Xvfb software-renderer launch survived the required 20-second timeout and created the profile database; fail-fast step PASS | PASS |

Downloaded outer artifact digests and every manifest's package size/SHA-256,
source commit, application version, architecture and toolchain were checked.
macOS reports `CFBundleShortVersionString=0.1.0`, `CFBundleVersion=10002` and
bundle ID `io.github.cydoniancitizen.bingee-desktop`. All package manifests
report application version `0.1.0-rc.1`, rustc 1.98.1 and the exact revision.
Windows installer ProductVersion is `0.1.0-rc.1`.

Linux logs confirm installation of `libxkbcommon-x11-0` 1.6.0-1build1 and
its libxcb-xkb prerequisite. The original dynamic-loader panic is resolved.
The headless runner lacks a Secret Service keyring: secure-token loading
reports unavailable, without preventing offline startup. The dependency's
v1 backend initializes its platform store automatically; source inspection
confirms this is the documented runtime-service prerequisite, not an omitted
application initialization call. No plaintext fallback was added.

Interactive GUI validation is **NOT PERFORMED** for every platform. Process,
file and offscreen structural checks are not evidence of interactive smoothness.
Windows smoke uses this host, not a clean PC. Reinstall smoke uses an empty
v5 profile; realistic nonempty upgrade is still an R17 requirement.

## Footprint

| Item | Bytes |
| --- | ---: |
| Windows executable | 28,853,248 |
| Windows portable ZIP | 13,736,161 |
| Windows installer | 10,022,466 |
| macOS tarball | 11,672,521 |
| macOS executable | 31,040,784 |
| Linux tarball | 16,894,633 |
| Linux executable | 49,864,656 |

These are disk sizes, not memory measurements. Full package hashes and outer
digests are in `r16-native-revalidation-20261001/artifact-inspection.json` and
`artifacts.json`. The same directory retains workflow/job results, relevant
native log excerpts, nine local correctness gates and Windows portable smoke.
Installer smoke exited 0 on this host in the isolated target directory.

The nine local Rust gates all passed again; each test configuration reports
214 passed, 13 ignored. The first scratch runner invocation stopped on a
PowerShell 5 native-stderr handling issue; correcting its error-handling scope
allowed all actual Cargo commands to complete successfully. No application
code, schema, lockfile or dependencies changed.

## Next gate and independent status

R17 can now begin technically. Its prior NOT STARTED record is historical,
not a completed RC verdict. Final technical RC readiness remains unproven
until those checks execute. R18 stabilization must close that inherited gap
before product polish. The later R18 request authorizes that milestone;
no release publication or R19 is authorized.

Licensing, intended-use, signing, notarization and public distribution remain
independent unresolved external decisions. R16 technical PASS grants no
public distribution authorization.
