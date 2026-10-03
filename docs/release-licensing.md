# Release licensing decision matrix

Engineering review: 2 October 2026. This records shipped mechanisms and
outstanding owner decisions; it does not select a project license or classify
Bingee Desktop as commercial or non-commercial. Public distribution remains
blocked. No legal-compliance conclusion is made.

| Item | Technical requirement implemented | External owner decision | Potential purchase/license requirement | Remaining evidence |
| --- | --- | --- | --- | --- |
| Bingee Desktop | About and package README disclose that the application license is undecided. | Select the application license and intended distribution. | Depends on the selected model; none assumed. | Recorded decision and approved distribution material. |
| Slint 1.17.1 | Sidebar About contains `AboutSlint`; package license trees include Slint texts. Existing engineering assumption remains Royalty-free Desktop, Mobile, and Web Applications License 2.0. | Confirm the final license basis; no model change in R18. | A paid path exists; no purchase is assumed or represented as necessary for every path. | Owner's selected basis and review of final attribution/license material against [upstream terms](https://slint.dev/terms-and-conditions). |
| TMDB | About ships the unmodified logo and non-endorsement notice; token storage and user-provided API access remain implemented. | Determine intended use and applicable TMDB terms/agreement. | The [TMDB FAQ](https://developer.themoviedb.org/docs/faq) distinguishes attributed non-commercial API use from commercial licensing through TMDB. Applicability is not decided here. | Owner decision and any applicable agreement; final asset/notice review. |
| Inno Setup 6.7.3 | Official compiler acquisition, per-user installer and package smoke tooling are implemented. | Determine whether the publisher's commercial-use purchase condition applies. | The [publisher's page](https://jrsoftware.org/isdl.php) requests purchasing a license for commercial use. No applicability or purchase is assumed. | Owner decision and purchase record if applicable. |
| Rust dependencies | Locked Cargo metadata drives target-specific `licenses/INDEX.txt`, copied license texts and linked-crate notices. R17 verified all three native package inventories for `3ab08c32a2fef16cf8555e9db467f681c1dbd1a2`. | Review final dependency expressions and notices for the selected distribution. | Package inventory alone does not decide obligations or any purchase requirement. | Review of the exact final artifact inventories; Cargo.lock remains unchanged in R18. |

## Attribution and package evidence

About remains reachable from the sidebar and Settings. It shows the Cargo
version, `AboutSlint`, the TMDB logo, the TMDB non-endorsement notice and the
location of third-party notices. Required attribution mechanisms remain in
place; the application license is still expressly undecided.

`scripts/package-licenses.ps1` collects crate-supplied license/notice files.
Where a crate lacks bundled text, it uses matching standard text from
`licenses/common/`; absent both, packaging fails. `licenses/INDEX.txt` maps
crate versions to declared license expressions. Generated linked-crate notices
exclude proc-macro-only dependencies; the broader license tree also includes
build-time material. These are engineering inventories, not legal analysis.

Upstream pages above were inspected on 2 October 2026. They are reference
material for the responsible owner; this review grants no distribution rights.
R17 exact-source artifact evidence remains in its native-blocker follow-up
report and `run-reports/r17-ci-closure-20261002/`. R18 local package evidence
has its own source scope in `run-reports/2026-10-03-r18-rc-polish.md`.

## Separate gates

Technical RC validation covers build, tests, package structure, manifests and
configured smoke checks. Public distribution additionally requires owner
licensing, intended-use and signing/distribution decisions. See
[the release checklist](release-checklist.md) and [signing readiness](release-signing.md).
