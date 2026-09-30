# R16 source and CI revalidation — 1 October 2026

**R16 = PARTIAL. R17 = NOT STARTED.** The required fixes have not been
committed or pushed. This continuation therefore stops at the R16 gate,
as instructed. It does not claim a new candidate or R17 measurements.

## Source identity and prepared fixes

Local branch `main`, HEAD, local `origin/main`, and the independently queried
GitHub `main` all identify `ceb10794eccc356268b402ff9a6cadf1844848b5`.
Its latest commit is **R15-R16: harden runtime and add cross-platform release
packaging**, authored on 30 September 2026. Read-only Git inspection and the
public GitHub API agree; this finding does not rely on a stale tracking ref.

The committed safety-copy test still compares directory path spelling. The
committed package workflow still omits `libxkbcommon-x11-0`. The local diff
contains the canonical-directory assertion and Linux prerequisite fix,
including the package README update. Related planning, README, milestone,
checklist and report changes also remain uncommitted.

At entry, seven tracked files were modified: `.github/workflows/release-artifacts.yml`,
`IMPLEMENTATION_PLAN.md`, `README.md`, `docs/milestones/R16.md`,
`docs/release-checklist.md`, `scripts/package-linux.ps1`, and `src/backup.rs`.
The two 30 September reports and their raw evidence directory were untracked.
This continuation preserves that work and adds only documentation/evidence.

## Actual CI, independently reviewed

The API returned no newer run for either workflow. Both latest runs remain
completed failures for the exact SHA above. Their platform jobs and steps
were retrieved again, rather than relying on an overall badge:

- [cross-platform.yml run 36678282320](https://github.com/CydonianCitizen/Bingee-Rust/actions/runs/36678282320)
- [release-artifacts.yml run 36678282283](https://github.com/CydonianCitizen/Bingee-Rust/actions/runs/36678282283)

| Platform | Build and tests | Package and manifest | Runtime/process smoke | R16 technical verdict |
| --- | --- | --- | --- | --- |
| Windows | PASS: normal CI and package job | PASS: portable ZIP, installer, structure and manifest; exact artifact checks retained from 30 September | PASS: downloaded-artifact process/file smoke retained from 30 September | PASS |
| macOS | Check passes; tests fail; release build skipped | Bundle, structure, resources, version and manifest not produced/validated | Not performed; no bundle | BLOCKED |
| Linux | PASS: normal CI and package build/tests | Tarball creation passes; structure checks reached launch; final manifest/upload skipped | FAIL: missing dynamically loaded `libxkbcommon-x11`; unpushed fix cannot change this run | BLOCKED |

The macOS failure is the `/var` versus `/private/var` directory alias
assertion. The local fix compares canonical directories while preserving
the safety-copy state assertion. Linux's runtime prerequisite is loaded
dynamically and cannot be certified by `ldd` alone. No error suppression or
Windows packaging redesign was introduced. Full original logs, artifact
hashes and test counts remain in `2026-09-30-r16-ci-closure.md` and its
evidence directory. Newly retrieved job/step results are saved in
`r16-revalidation-20261001/github-evidence.json`.

Interactive GUI validation was not performed for any platform. Existing
process/file smoke proves only its recorded behavior. Windows is not rebuilt
or re-engineered in this continuation because its source and successful
artifact evidence are unchanged.

## Verification and scope

The 30 September local post-fix suite remains historical evidence: all nine
Rust gates passed, with 214 passed and 13 ignored in each test configuration.
It was not rerun or relabelled as R17 evidence here. This continuation checked
source identity, committed versus local fix contents, current workflow runs,
each native platform job, documentation status, and `git diff --check`.
A transient job-fetch disconnect was resolved by a read-only retry.
No dependencies, application behavior, schema, lockfile or version changed.

Commit, push and `gh` restrictions in `.claude/settings.json` were respected.
No Git write operation, workflow dispatch, release publication or R18 work
was attempted. The fixes remain ready for a user commit/push; actual native
CI revalidation is pending. R17 starts only after the pushed revision passes
all three technical package gates and required smoke checks.

## Independent verdicts

**TECHNICAL RC STATUS: NOT TECHNICALLY READY FOR RC.** R16 remains partial;
R17 has not started.

**PUBLIC DISTRIBUTION STATUS: PUBLIC DISTRIBUTION NOT READY.** Technical
validation is incomplete. Licensing, application intended-use, signing,
notarization and distribution decisions remain separate unresolved external
decisions. No commercial/non-commercial intent or public distribution
authorization is inferred.
