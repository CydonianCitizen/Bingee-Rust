<#
.SYNOPSIS
Non-interactive launch check of dist/bingee-desktop-windows-x64 (run
scripts/package-windows.ps1 first, so the package has no database yet).

.DESCRIPTION
1. Starts the packaged executable with -WorkDir as its working directory,
   waits for the responsive "Bingee Desktop" window, then closes it gracefully.
   Requires: data/bingee-spike.db created inside the package, nothing created
   in the working directory, no stderr output, exit code 0.
2. Repeats on a copy of the package without assets/posters/poster-001.jpg (the
   poster of the first row and the initial selection). The app must report that
   exact packaged path, which proves posters resolve from the package and not
   from the build checkout.

Process APIs only: a window handle and exit codes, not a visual check.

.EXAMPLE
pwsh -NoProfile -File scripts/smoke-windows-package.ps1 -WorkDir $env:TEMP\bingee-smoke
#>
param([Parameter(Mandatory)][string]$WorkDir)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$package = (Resolve-Path (Join-Path $PSScriptRoot '../dist/bingee-desktop-windows-x64')).Path
$WorkDir = [IO.Path]::GetFullPath($WorkDir)
if ($WorkDir.StartsWith($package, [StringComparison]::OrdinalIgnoreCase)) { throw 'WorkDir must be outside the package.' }
New-Item -ItemType Directory -Force -Path $WorkDir | Out-Null
if (Get-Process bingee-desktop -ErrorAction SilentlyContinue) { throw 'Close running Bingee instances first.' }
$db = Join-Path $package 'data\bingee-spike.db'
if (Test-Path $db) { throw 'The package already has a database; rebuild it with scripts/package-windows.ps1.' }

function Invoke-Launch([string]$Exe, [string]$Label) {
    $stderr = Join-Path $WorkDir "$Label-stderr.txt"
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $process = Start-Process -FilePath $Exe -WorkingDirectory $WorkDir -PassThru -RedirectStandardError $stderr
    try {
        do {
            Start-Sleep -Milliseconds 50
            $process.Refresh()
            if ($process.HasExited) { throw "${Label}: exited early with code $($process.ExitCode); see $stderr" }
            if ($timer.Elapsed.TotalSeconds -gt 30) { throw "${Label}: no responsive window within 30 s" }
        } until ($process.MainWindowHandle -ne 0 -and $process.MainWindowTitle -eq 'Bingee Desktop' -and $process.Responding)
        $windowMs = [math]::Round($timer.Elapsed.TotalMilliseconds)
        Start-Sleep -Seconds 3 # the first frame loads the visible posters
        $process.Refresh()
        $result = [ordered]@{
            label = $Label; exe = $Exe; working_directory = $WorkDir; pid = $process.Id
            window_ready_ms = $windowMs; working_set_bytes = $process.WorkingSet64; private_bytes = $process.PrivateMemorySize64
        }
    } finally {
        $process.Refresh()
        [void]$process.CloseMainWindow()
        if (-not $process.WaitForExit(10000)) { $process.Kill(); $process.WaitForExit(); throw "${Label}: no graceful exit" }
    }
    $result.exit_code = $process.ExitCode
    $result.stderr = @(Get-Content $stderr)
    if ($result.exit_code -ne 0) { throw "${Label}: exit code $($result.exit_code)" }
    [pscustomobject]$result
}

$clean = Invoke-Launch (Join-Path $package 'bingee-desktop.exe') 'package'
if (-not (Test-Path $db)) { throw 'data\bingee-spike.db was not created inside the package.' }
if ((Test-Path (Join-Path $WorkDir 'data')) -or (Test-Path (Join-Path $WorkDir 'bingee-spike.db'))) { throw 'Files were created in the working directory.' }
if ($clean.stderr.Count) { throw "Unexpected stderr: $($clean.stderr -join ' | ')" }

$copy = Join-Path $WorkDir 'package-copy'
if (Test-Path $copy) { Remove-Item -LiteralPath $copy -Recurse -Force }
Copy-Item -LiteralPath $package -Destination $copy -Recurse
Remove-Item -LiteralPath (Join-Path $copy 'assets\posters\poster-001.jpg')
$missing = Invoke-Launch (Join-Path $copy 'bingee-desktop.exe') 'package-copy-without-poster-001'
$expected = 'Poster could not be loaded, showing placeholder: ' + (Join-Path $copy 'assets\posters\poster-001.jpg')
if ($missing.stderr -notcontains $expected) { throw "Missing expected line: $expected; got: $($missing.stderr -join ' | ')" }

$summary = [ordered]@{
    utc = [DateTime]::UtcNow.ToString('o')
    package = $package
    executable_sha256 = (Get-FileHash (Join-Path $package 'bingee-desktop.exe')).Hash
    database = @{ path = $db; bytes = (Get-Item $db).Length; sha256 = (Get-FileHash $db).Hash }
    runs = @($clean, $missing)
    verdict = 'package launched from an unrelated working directory; database created in package data\; posters resolved from package assets\; graceful exit'
}
$summary | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $WorkDir 'smoke.json') -Encoding utf8NoBOM
$summary | ConvertTo-Json -Depth 5
