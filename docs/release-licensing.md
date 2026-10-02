# Release licensing decision matrix

Engineering review: 1 October 2026. Native package evidence is updated for `6e24b2a`; upstream conditions remain the previously recorded review, not a new legal determination. This records implementation evidence. It does not classify Bingee Desktop's intended use or decide a license for its own code. Public distribution remains blocked until the responsible owner makes those decisions.

| Item | Technical path and evidence | Status before public distribution |
| --- | --- | --- |
| Bingee Desktop's own license | No project license has been selected. About and package README say so. | **EXTERNAL DECISION REQUIRED; BLOCKER** |
| Slint 1.17.1 | Working engineering path: Royalty-free Desktop, Mobile, and Web Applications License 2.0. The About page is reachable from the main sidebar and contains `AboutSlint`; package material includes Slint `LICENSES/`. This engineering implementation matches the [section 2(a) attribution condition](https://slint.dev/terms-and-conditions). | **ENGINEERING COMPLETE; EXTERNAL DECISION REQUIRED** for the final license basis |
| TMDB developer API, non-commercial path | About contains an approved, unmodified TMDB logo, smaller than the Bingee name, and the required non-endorsement notice. This engineering implementation matches the [TMDB FAQ attribution requirements](https://developer.themoviedb.org/docs/faq). | **ENGINEERING COMPLETE; EXTERNAL DECISION REQUIRED** whether this path applies |
| TMDB commercial path | [TMDB says](https://developer.themoviedb.org/docs/faq) commercial API/data/image use requires contacting it for a license. No agreement is claimed. | **LICENSE/PURCHASE REQUIRED IF COMMERCIAL; BLOCKER** until applicable terms are obtained |
| Inno Setup 6.7.3 | Windows installer uses the official compiler. [Publisher's download page](https://jrsoftware.org/isdl.php) requests a purchased license for commercial use. No purchase is claimed. | **LICENSE/PURCHASE REQUIRED IF COMMERCIAL; EXTERNAL DECISION REQUIRED** |
| Rust crates | Locked Cargo metadata drives per-platform `licenses/INDEX.txt`, individual crate text directories, and generated linked-crate notices. All three native packages built and their downloaded license/notices resources passed inspection for `6e24b2a`. | **PACKAGE ENGINEERING VERIFIED; FINAL DISTRIBUTION REVIEW REQUIRED** |

## Available Slint paths

The [Royalty-free license](https://slint.dev/terms-and-conditions) permits its `AboutSlint` widget in a top-level About screen or an attribution badge on a public webpage. Bingee implements the widget path. Upstream also offers a [paid Software License and GPL-3.0](https://slint.dev/terms-and-conditions). Their applicability depends on the owner's distribution choice; this record selects neither alternative.

## Available TMDB paths

The [TMDB FAQ](https://developer.themoviedb.org/docs/faq) says its API is free for non-commercial purposes with attribution. It requires an approved logo in About or Credits and the prominent notice: “This product uses the TMDB API but is not endorsed or certified by TMDB.” For commercial use, TMDB says to contact its sales team for a license. Bingee has not been classified as commercial or non-commercial.

## Dependency material and review limit

`scripts/package-licenses.ps1` uses `cargo metadata --locked --filter-platform` and copies crate-supplied license and notice files. For a crate with no bundled license file, it copies a matching standard text from `licenses/common/`; if neither exists, packaging fails. `licenses/INDEX.txt` maps each crate and version to its declared license expression. The generated `THIRD_PARTY_NOTICES.txt` lists normal, non-proc-macro dependencies linked for the target. Build-only crates are included in the broader license tree. This is a reproducible engineering inventory, not a legal interpretation of each expression. Native macOS and Linux packaging and downloaded-resource verification passed for `6e24b2a`; this grants no public distribution authorization.

## Separate gates

Technical package readiness requires built and checked Windows, macOS and Linux packages plus normal cross-platform tests. Signing and notarization can remain pending. Authorization to distribute publicly requires the owner's application-license and intended-use decisions, any applicable TMDB agreement and Inno Setup purchase, plus review of the final license material. Technical R17 validation may proceed once the technical gate passes, even while these external decisions remain open.
