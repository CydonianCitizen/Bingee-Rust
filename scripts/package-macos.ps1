$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsMacOS) { throw 'Run on macOS.' }
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $root
try {
    $arch = (& uname -m).Trim()
    $target = if ($arch -eq 'arm64') { 'aarch64-apple-darwin' } elseif ($arch -eq 'x86_64') { 'x86_64-apple-darwin' } else { throw "Unsupported architecture: $arch" }
    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Release build failed.' }
    $version = (cargo metadata --no-deps --format-version 1 --locked | ConvertFrom-Json).packages |
        Where-Object name -eq 'bingee-desktop' | Select-Object -ExpandProperty version
    if ($version -notmatch '^(\d+)\.(\d+)\.(\d+)(?:-rc\.(\d+))?$') { throw "Unsupported Cargo version: $version" }
    $shortVersion = "$($Matches[1]).$($Matches[2]).$($Matches[3])"
    $hasRc = $Matches.ContainsKey(4) -and $Matches[4]
    $rc = if ($hasRc) { [int]$Matches[4] } else { 99 }
    if ($hasRc -and $rc -gt 98) { throw 'RC sequence exceeds the macOS bundle version policy.' }
    $bundleVersion = 1 + [int]$Matches[1] * 1000000 + [int]$Matches[2] * 10000 + [int]$Matches[3] * 100 + $rc
    $dist = [IO.Path]::GetFullPath((Join-Path $root 'dist'))
    New-Item -ItemType Directory -Force $dist | Out-Null
    $bundle = Join-Path $dist 'Bingee Desktop.app'
    if ([IO.Path]::GetFullPath($bundle) -ne (Join-Path $dist 'Bingee Desktop.app')) { throw 'Unexpected bundle path.' }
    if (Test-Path -LiteralPath $bundle) { Remove-Item -LiteralPath $bundle -Recurse -Force }
    $macos = Join-Path $bundle 'Contents/MacOS'
    $resources = Join-Path $bundle 'Contents/Resources'
    New-Item -ItemType Directory -Force $macos,$resources | Out-Null
    Copy-Item -LiteralPath 'target/release/bingee-desktop' -Destination $macos
    & chmod +x (Join-Path $macos 'bingee-desktop')
    Copy-Item -LiteralPath 'THIRD_PARTY_NOTICES.txt' -Destination $resources
    Copy-Item -LiteralPath 'ui/assets/bingee-icon.svg' -Destination $resources
    & (Join-Path $PSScriptRoot 'package-licenses.ps1') -Package $resources -Target $target
    if ($LASTEXITCODE -ne 0) { throw 'License collection failed.' }
    @"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>Bingee Desktop</string>
<key>CFBundleDisplayName</key><string>Bingee Desktop</string>
<key>CFBundleIdentifier</key><string>io.github.cydoniancitizen.bingee-desktop</string>
<key>CFBundleVersion</key><string>$bundleVersion</string>
<key>CFBundleShortVersionString</key><string>$shortVersion</string>
<key>CFBundleExecutable</key><string>bingee-desktop</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
"@ | Set-Content (Join-Path $bundle 'Contents/Info.plist') -Encoding utf8NoBOM
    & plutil -lint (Join-Path $bundle 'Contents/Info.plist')
    if ($LASTEXITCODE -ne 0) { throw 'Invalid Info.plist.' }
    $archive = Join-Path $dist "bingee-desktop-macos-$arch.tar.gz"
    if (Test-Path -LiteralPath $archive) { Remove-Item -LiteralPath $archive -Force }
    & tar -C $dist -czf $archive 'Bingee Desktop.app'
    if ($LASTEXITCODE -ne 0) { throw 'Archive failed.' }
    Write-Output "Bundle: $bundle"
    Write-Output "Archive: $archive, $((Get-Item $archive).Length) bytes, SHA-256 $((Get-FileHash $archive).Hash)"
    Write-Output 'Unsigned. Gatekeeper may block launch until user explicitly allows it.'
} finally { Pop-Location }
