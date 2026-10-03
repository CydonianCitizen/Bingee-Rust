# Signing and publication readiness

No signing credentials are configured or added by R18. No release is published.
Unsigned packages are permitted for technical RC checks; public distribution
policy is an independent owner decision.

| Platform | Current artifact state | Future pipeline insertion point | Evidence required after a decision |
| --- | --- | --- | --- |
| Windows | Application and Inno Setup installer unsigned | In `package-windows.ps1`, sign and verify the copied application before portable checksums, ZIP creation and Inno compilation. Sign and verify the resulting installer before the workflow manifest/upload. | Selected signing service/certificate, timestamp policy, signature verification and new package/installer smoke; regenerate every affected hash. |
| macOS | `.app` unsigned, archive not notarized | In `package-macos.ps1`, sign nested code and the completed bundle after resources/plist creation and before archiving. Submit the archive for notarization, wait for success, staple/verify the app, then rebuild the final archive before manifest/upload. | Owner's Developer ID/notarization setup, entitlement review, signature/notarization verification and native launch assessment of the final artifact. |
| Linux | Unsigned tarball with manifest SHA-256 | After final archive generation and before upload, add detached package/manifest signatures only if the distribution policy calls for them. | Selected distribution channel and signature/key handling policy; final signature/hash verification. |

Certificates and service credentials belong in the selected CI secret store,
not this repository. There are no fake or self-signed release credentials.
Signing changes bytes, so pre-signing checksums cannot describe final artifacts.

## Artifact generation and future RC publication

`release-artifacts.yml` still builds on pushes to `main` and manual dispatch.
It also accepts `v*` tags and rejects tags that do not equal `v` plus the
Cargo package version. For the current version this is `v0.1.0-rc.1`.
This document does not authorize creating or pushing that tag.

The workflow has `contents: read`, builds three native packages, records
manifests/hashes and uploads workflow artifacts. It has no GitHub Release
creation or publication step. The locked dependency graph, native build steps
and manifests make provenance reproducible; stable toolchains/runners and
archive timestamps are not pinned for byte-for-byte reproducibility.

After owner decisions, validate the exact final source in both workflows,
inspect each platform job and artifact, apply the agreed signing process,
verify the final bytes, and complete the public-distribution checklist before
any separately authorized publication.
