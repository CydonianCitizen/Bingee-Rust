# R17 gate status — 30 September 2026

Revalidation on 1 October 2026: **R17 remains NOT STARTED**. GitHub `main`
still points to `ceb1079`; the prerequisite fixes were not pushed. See
`2026-10-01-r16-revalidation.md` for current source and platform CI evidence.

**NOT STARTED.** This is a durable gate-status record, not an R17 execution
or completion report. The prerequisite R16 gate did not pass for
`ceb10794eccc356268b402ff9a6cadf1844848b5`. See
`2026-09-30-r16-ci-closure.md` and its raw evidence directory.

Windows normal CI, native packages and downloaded-artifact process/file
smoke pass. macOS fails a safety-copy test's directory-alias assertion before
creating a bundle. Linux normal CI and tarball structure pass, but the
packaged X11 launch panics on a missing dynamically loaded library. Both
project-owned fixes are ready locally; commit/push permissions prohibit
publishing the revised source to CI. Native revalidation is pending.

No candidate revision was frozen, no new product feature was introduced,
and no R17 headless harness, benchmark or optimization was started. The
existing regression suite was run as R16 local verification. Historical R15
debug soak and R4 timing data are not relabelled as final RC evidence.

| Required R17 work after the gate | Current status |
| --- | --- |
| Freeze candidate identity, lockfile, build profile and acceptance checks | NOT RUN |
| Fresh and v1–v5 migration compatibility, rollback and preservation | NOT RUN as final R17 gate |
| Early/current Backup V1, large fixture, safety/failed restore | NOT RUN as final R17 gate |
| Comprehensive fake-TMDB end-to-end regression and failure matrix | NOT RUN as final R17 gate |
| Repeated process startup samples and idle CPU | NOT MEASURED |
| Process-private memory, repeated-use soak and plateau analysis | NOT MEASURED |
| Cache occupancy, handle/thread stability and refresh stress | NOT MEASURED as final R17 workload |
| SQLite, page, local-write and UI-thread callback timing/query plans | NOT MEASURED as final R17 workload |
| Keyboard/focus/accessibility roles and names | NOT AUDITED as final R17 gate |
| Nonempty profile upgrade, install/uninstall and package regression | NOT RUN as final R17 gate; R16 empty-profile smoke is limited evidence |
| Final outbound-data, credential, backup, path and JSON sanity | NOT AUDITED as final R17 gate |
| Consolidated checklist and explicit limitations | Checklist updated; final release acceptance pending |

R17 remains a feature freeze under the implementation plan and established
release reports. On genuine R16 technical PASS, execute the above without
waiting for external licensing decisions. Use a deterministic headless
application exercise where native control is unavailable, with the real
controllers, local database and fake HTTP server. State exactly which
behavior and callback completion it proves; headless timing cannot prove
physical input delivery, paint latency or interactive smoothness. Follow
`BENCHMARK_SPEC.md`: environment, source identity, release profile, stable
fixture, sample count, raw evidence and spread. Measure before optimizing;
fix actual defects without speculative frameworks.

**TECHNICAL RC STATUS: NOT READY** because R16 is blocked and R17 has not
executed. **PUBLIC DISTRIBUTION STATUS: NOT READY**, retaining independent
application/Slint/TMDB/Inno licensing, signing/notarization and distribution
decisions. Neither verdict treats an unresolved external decision as
technical package proof. No release publication or R18 work occurred.

Next action: user review/commit/push of the R16 fixes, then actual native CI
and artifact revalidation. This record does not waive the prerequisite or
claim that an unchanged failed run can validate unpushed work.
