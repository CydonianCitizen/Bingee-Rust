# Installing Bingee Desktop release candidates

Current candidate: `0.1.0-rc.1`. No public release is authorized or published.
Use only artifacts supplied for authorized RC testing. Keep the accompanying
`PACKAGE-MANIFEST.json`; it identifies platform, architecture, Cargo version,
source commit, size and SHA-256. Compare its SHA-256 with the downloaded file.
Hashes detect mismatches; they are not code signatures.

## Windows installer

Run `bingee-desktop-0.1.0-rc.1-windows-x64-setup.exe`. It installs for the current
user without requiring an administrator. Install the Microsoft Visual C++
2015–2022 x64 runtime first if it is absent. The installer is unsigned;
technical smoke checks do not remove Windows reputation warnings. Follow your
organization's policy for evaluating unsigned candidates.

Uninstall removes application files and leaves your Library, cache and logs.

## Windows portable ZIP

Extract `bingee-desktop-0.1.0-rc.1-windows-x64.zip`, preserving its directory
structure, and run `bingee-desktop.exe`. Keep the notices and `licenses/` folder.
The same runtime prerequisite applies. Portable describes the application
files: by default, saved data still goes to `%LOCALAPPDATA%\Bingee Desktop`.
The ZIP includes `SHA256SUMS.txt` for its individual files.

## macOS app archive

Extract `bingee-desktop-0.1.0-rc.1-macos-arm64.tar.gz` on Apple Silicon, then
copy `Bingee Desktop.app` to Applications or another folder you can write to.
An x86_64 archive requires a native x86_64 build and separate validation.
The app is unsigned and not notarized; Gatekeeper may prevent opening it.
Do not treat the structural CI checks as signing, notarization or native GUI
validation. Keep the bundle intact, including its notices and licenses.

## Linux package

Extract `bingee-desktop-0.1.0-rc.1-linux-x86_64.tar.gz`, enter its directory,
and run `./install.sh` for a user-level executable, icon and desktop entry.
`./uninstall.sh` removes application files while retaining personal data.
Alternatively, run `bin/bingee-desktop` from the extracted package.
An aarch64 build requires separate validation.

Required runtime facilities include fontconfig, OpenGL/EGL or a working
software renderer, and `libxkbcommon-x11` for X11 sessions (Debian/Ubuntu:
`libxkbcommon-x11-0`). Secure TMDB token storage needs a Secret Service.
The tarball is unsigned; it is not a distribution-managed DEB/RPM package.

## First run and data

Open Settings and add your own TMDB API Read Access Token. Search in Discover
and choose Add to Library. Cached details, tracking and history work offline.
Settings shows the actual data/cache/log paths and offers Backup / Restore.
Export a backup before moving or replacing an installation. Restore replaces
saved data and first creates a safety backup; tokens and image cache are excluded.

The original validated R17 artifacts have unversioned filenames. Their exact
identities remain in the R17 report. R18 filenames are versioned; do not rename
an older binary to imply it contains the R18 changes.
