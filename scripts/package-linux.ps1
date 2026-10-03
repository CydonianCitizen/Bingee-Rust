$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsLinux) { throw 'Run on Linux.' }
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $root
try {
    $arch = (& uname -m).Trim()
    $target = if ($arch -eq 'x86_64') { 'x86_64-unknown-linux-gnu' } elseif ($arch -eq 'aarch64') { 'aarch64-unknown-linux-gnu' } else { throw "Unsupported architecture: $arch" }
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    $version = (cargo metadata --no-deps --format-version 1 --locked | ConvertFrom-Json).packages |
        Where-Object name -eq 'bingee-desktop' | Select-Object -ExpandProperty version
    $dist = [IO.Path]::GetFullPath((Join-Path $root 'dist'))
    New-Item -ItemType Directory -Force $dist | Out-Null
    $package = Join-Path $dist "bingee-desktop-linux-$arch"
    if ([IO.Path]::GetFullPath($package) -ne (Join-Path $dist "bingee-desktop-linux-$arch")) { throw 'Unexpected package path.' }
    if (Test-Path -LiteralPath $package) { Remove-Item -LiteralPath $package -Recurse -Force }
    $bin = Join-Path $package 'bin'
    $doc = Join-Path $package 'share/doc/bingee-desktop'
    $icon = Join-Path $package 'share/icons/hicolor/scalable/apps'
    $applications = Join-Path $package 'share/applications'
    New-Item -ItemType Directory -Force $bin,$doc,$icon,$applications | Out-Null
    Copy-Item -LiteralPath 'target/release/bingee-desktop' -Destination $bin
    Copy-Item -LiteralPath 'THIRD_PARTY_NOTICES.txt' -Destination $doc
    Copy-Item -LiteralPath 'ui/assets/bingee-icon.svg' -Destination (Join-Path $icon 'bingee-desktop.svg')
    Copy-Item -LiteralPath 'scripts/bingee-desktop.desktop' -Destination $applications
    Copy-Item -LiteralPath 'scripts/linux-install.sh' -Destination (Join-Path $package 'install.sh')
    Copy-Item -LiteralPath 'scripts/linux-uninstall.sh' -Destination (Join-Path $package 'uninstall.sh')
    & chmod +x (Join-Path $bin 'bingee-desktop') (Join-Path $package 'install.sh') (Join-Path $package 'uninstall.sh')
    & (Join-Path $PSScriptRoot 'package-licenses.ps1') -Package $doc -Target $target
    if ($LASTEXITCODE -ne 0) { throw 'License collection failed.' }
    @"
Bingee Desktop $version

Run ./install.sh to install into ~/.local/bin and your XDG data directory.
Run ./uninstall.sh to remove application files only. Both scripts leave your
Library, cache and logs untouched. The executable also runs from this folder.
Linux requires fontconfig, OpenGL/EGL or a working software renderer, and
libxkbcommon-x11 for X11 sessions (Debian/Ubuntu: libxkbcommon-x11-0). This
keyboard library is loaded at runtime, so ldd does not list it. A Secret
Service implementation is required for secure TMDB token storage. No terminal
is required after desktop installation. This tarball is unsigned.
"@ | Set-Content (Join-Path $doc 'README.txt') -Encoding utf8NoBOM
    $archive = Join-Path $dist "bingee-desktop-$version-linux-$arch.tar.gz"
    if (Test-Path -LiteralPath $archive) { Remove-Item -LiteralPath $archive -Force }
    & tar -C $dist -czf $archive (Split-Path $package -Leaf)
    if ($LASTEXITCODE -ne 0) { throw 'Archive failed.' }
    Write-Output "Package: $package"
    Write-Output "Archive: $archive, $((Get-Item $archive).Length) bytes, SHA-256 $((Get-FileHash $archive).Hash)"
} finally { Pop-Location }
