<#
.SYNOPSIS
Builds the portable Windows x64 package: dist/bingee-desktop-windows-x64/.

.DESCRIPTION
PowerShell 7 on Windows x64, from any working directory. It builds the release
executable (default features, so no benchmark fixture), recreates the package
directory from scratch, copies the executable, the notices and a README,
writes SHA256SUMS.txt, and prints the sizes.

The package holds no writable data: the application keeps its library in the
per-user folders (ADR-0006), so the package folder may be read-only. It is not
an installer, and nothing is signed or zipped.

.EXAMPLE
pwsh -NoProfile -File scripts/package-windows.ps1
#>
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
Push-Location $root
try {
    foreach ($name in @('CARGO_TARGET_DIR', 'CARGO_BUILD_TARGET', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS')) {
        if ([Environment]::GetEnvironmentVariable($name)) { throw "Unset ${name}: the package uses the default release build." }
    }
    $target = 'x86_64-pc-windows-msvc'
    if (-not (rustc -vV | Select-String -SimpleMatch "host: $target")) { throw "Host toolchain must be $target." }

    cargo build --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'cargo build --release --locked failed.' }

    $package = Join-Path $root 'dist/bingee-desktop-windows-x64'
    if (Test-Path $package) { Remove-Item -LiteralPath $package -Recurse -Force }
    New-Item -ItemType Directory -Path $package | Out-Null
    Copy-Item 'target/release/bingee-desktop.exe' $package

    # Notices: the curated header plus every crate linked for this target,
    # read from Cargo.lock (normal dependencies, proc-macros excluded).
    [Console]::OutputEncoding = [Text.Encoding]::UTF8
    $meta = cargo metadata --format-version 1 --locked --filter-platform $target | ConvertFrom-Json
    if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed.' }
    $packages = @{}; foreach ($p in $meta.packages) { $packages[$p.id] = $p }
    $nodes = @{}; foreach ($n in $meta.resolve.nodes) { $nodes[$n.id] = $n }
    $linked = [Collections.Generic.HashSet[string]]::new()
    $stack = [Collections.Generic.Stack[string]]::new()
    $stack.Push($meta.resolve.root)
    while ($stack.Count) {
        foreach ($dep in $nodes[$stack.Pop()].deps) {
            $normal = @($dep.dep_kinds | Where-Object { $null -eq $_.kind }).Count -gt 0
            if ($normal -and $linked.Add($dep.pkg)) { $stack.Push($dep.pkg) }
        }
    }
    $crates = @($linked | ForEach-Object { $packages[$_] } |
        Where-Object { $_.targets.kind -notcontains 'proc-macro' } | Sort-Object name, version)
    $notices = @(Get-Content 'THIRD_PARTY_NOTICES.txt')
    $notices += '', "Crates linked into this build ($target, $($crates.Count) crates)", ('-' * 60)
    $notices += $crates | ForEach-Object { '  {0} {1}: {2}' -f $_.name, $_.version, $_.license }
    $notices | Set-Content (Join-Path $package 'THIRD_PARTY_NOTICES.txt') -Encoding utf8NoBOM

    $commit = (git rev-parse HEAD).Trim()
    $dirty = if (git status --porcelain) { ' plus uncommitted changes' } else { '' }
    $rust = (rustc --version).Trim()
    $version = ($meta.packages | Where-Object { $_.id -eq $meta.resolve.root }).version
    @"
Bingee Desktop $version - portable Windows x64 package
=====================================================

Bingee Desktop is a local-first desktop tracker for movies and TV series,
built with Rust and Slint. This is a development build, not a public release.
The license of Bingee Desktop itself is not decided yet; third-party licenses
are in THIRD_PARTY_NOTICES.txt and in the app's About page.

Run
  Start bingee-desktop.exe: double-click it, or run it from any shell and any
  working directory. No installation and no console window. The first start
  shows an empty library.

Layout
  bingee-desktop.exe      the application
  THIRD_PARTY_NOTICES.txt licenses and attribution
  SHA256SUMS.txt          checksums of the files as packaged

Your data
  Nothing is written to this folder. The library is
    %LOCALAPPDATA%\Bingee Desktop\data\bingee.db
  and the log is
    %LOCALAPPDATA%\Bingee Desktop\logs\bingee-desktop.log
  Settings and About show the exact paths. Set BINGEE_HOME to an absolute
  folder to keep data, cache and logs there instead (for testing).
  If the database cannot be opened, the app says so and leaves the file
  untouched. It never deletes or recreates it.

Requirements
  Windows 10 or 11, x64.
  Microsoft Visual C++ 2015-2022 Redistributable (x64), for VCRUNTIME140.dll.
  Most machines already have it.
  OpenGL 2.0 or newer (default FemtoVG renderer). If the window stays blank,
  for example in a VM or over Remote Desktop, set SLINT_BACKEND=winit-software
  to try Slint's software renderer (not tested).

Build
  Source commit $commit$dirty
  $rust, cargo build --release --locked, target $target
"@ | Set-Content (Join-Path $package 'README.txt') -Encoding utf8NoBOM

    $files = @(Get-ChildItem $package -Recurse -File | Sort-Object FullName)
    $files | ForEach-Object {
        '{0}  {1}' -f (Get-FileHash $_.FullName).Hash.ToLower(), [IO.Path]::GetRelativePath($package, $_.FullName).Replace('\', '/')
    } | Set-Content (Join-Path $package 'SHA256SUMS.txt') -Encoding utf8NoBOM

    $exe = Get-Item (Join-Path $package 'bingee-desktop.exe')
    $total = (Get-ChildItem $package -Recurse -File | Measure-Object Length -Sum).Sum
    Write-Output "Package:     $package"
    Write-Output "Source:      $commit$dirty"
    Write-Output "Executable:  $($exe.Length) bytes, SHA-256 $((Get-FileHash $exe.FullName).Hash)"
    Write-Output "Crates:      $($crates.Count) listed in THIRD_PARTY_NOTICES.txt"
    Write-Output "Total:       $total bytes in $(@(Get-ChildItem $package -Recurse -File).Count) files"
} finally {
    Pop-Location
}
