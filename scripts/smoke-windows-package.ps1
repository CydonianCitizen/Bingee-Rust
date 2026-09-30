<#
.SYNOPSIS
Non-interactive launch checks of dist/bingee-desktop-windows-x64 (run
scripts/package-windows.ps1 first).

.DESCRIPTION
Every launch runs with -WorkDir\cwd as its working directory and with
LOCALAPPDATA pointed at a fresh folder under -WorkDir, so the real per-user
library is never touched while the real path resolution is exercised.

1. fresh:   creates <profile>\Bingee Desktop\{data\bingee.db, cache, logs},
            migrates to schema version 5, logs startup, writes nothing to the
            working directory or the package, exits 0 after a graceful close.
2. second:  same profile; the database file is byte-identical afterwards and
            no migration runs again.
3. corrupt: a profile whose bingee.db is not a database. The window still
            opens (the startup error page), the file is byte-identical
            afterwards, and the log records the failure.
4. fixture: --benchmark-fixture is refused by this default build (exit 1,
            no window).

Process APIs and files only: a window handle, exit codes and the log file,
not a visual check.

.EXAMPLE
pwsh -NoProfile -File scripts/smoke-windows-package.ps1 -WorkDir $env:TEMP\bingee-smoke
#>
param([Parameter(Mandatory)][string]$WorkDir)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$package = (Resolve-Path (Join-Path $PSScriptRoot '../dist/bingee-desktop-windows-x64')).Path
$exe = Join-Path $package 'bingee-desktop.exe'
$WorkDir = [IO.Path]::GetFullPath($WorkDir)
if ($WorkDir -eq [IO.Path]::GetPathRoot($WorkDir)) { throw 'WorkDir cannot be a filesystem root.' }
if ($WorkDir.StartsWith($package, [StringComparison]::OrdinalIgnoreCase)) { throw 'WorkDir must be outside the package.' }
$WorkDir = Join-Path $WorkDir ("run-{0}-{1}" -f [DateTime]::UtcNow.ToString('yyyyMMddHHmmssfff'), $PID)
$cwd = Join-Path $WorkDir 'cwd'
New-Item -ItemType Directory -Force -Path $cwd | Out-Null
if (Get-Process bingee-desktop -ErrorAction SilentlyContinue) { throw 'Close running Bingee instances first.' }
$packageBefore = @(Get-ChildItem $package -Recurse -File | ForEach-Object { $_.FullName + '|' + (Get-FileHash $_.FullName).Hash })

function Invoke-Launch([string]$Label, [string]$LocalAppData) {
    $stderr = Join-Path $WorkDir "$Label-stderr.txt"
    $saved = $env:LOCALAPPDATA, $env:BINGEE_HOME
    $env:LOCALAPPDATA = $LocalAppData
    $env:BINGEE_HOME = $null
    try {
        $timer = [Diagnostics.Stopwatch]::StartNew()
        $process = Start-Process -FilePath $exe -WorkingDirectory $cwd -WindowStyle Hidden -PassThru -RedirectStandardError $stderr
    } finally {
        $env:LOCALAPPDATA, $env:BINGEE_HOME = $saved
    }
    try {
        do {
            Start-Sleep -Milliseconds 50
            $process.Refresh()
            if ($process.HasExited) { throw "${Label}: exited early with code $($process.ExitCode); see $stderr" }
            if ($timer.Elapsed.TotalSeconds -gt 30) { throw "${Label}: no responsive window within 30 s" }
        } until ($process.MainWindowHandle -ne 0 -and $process.MainWindowTitle -eq 'Bingee Desktop' -and $process.Responding)
        $windowMs = [math]::Round($timer.Elapsed.TotalMilliseconds)
        Start-Sleep -Seconds 2
        $process.Refresh()
        $result = [ordered]@{
            label = $Label; local_app_data = $LocalAppData; working_directory = $cwd; pid = $process.Id
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

function Get-Bytes([string]$Path) { [Convert]::ToBase64String([IO.File]::ReadAllBytes($Path)) }

# 1. Fresh start.
$userDir = Join-Path $WorkDir 'profile'
$root = Join-Path $userDir 'Bingee Desktop'
$db = Join-Path $root 'data\bingee.db'
$log = Join-Path $root 'logs\bingee-desktop.log'
New-Item -ItemType Directory -Path $userDir | Out-Null
$fresh = Invoke-Launch 'fresh' $userDir
foreach ($path in @($db, $log, (Join-Path $root 'cache'))) { if (-not (Test-Path $path)) { throw "fresh: $path was not created." } }
$text = Get-Content $log -Raw
foreach ($line in @('INFO Bingee Desktop ', 'INFO Migrating the database from schema version 0 to 5', 'INFO Migration to schema version 5 complete', 'INFO Library opened: schema version 5, 0 titles', 'INFO Bingee Desktop closed')) {
    if (-not $text.Contains($line)) { throw "fresh: log lacks '$line'." }
}
if ($text.Contains(' ERROR ')) { throw "fresh: log has an error: $text" }
if (@(Get-ChildItem $cwd -Force).Count) { throw 'fresh: files were created in the working directory.' }

# 2. Second start, same profile.
$before = Get-Bytes $db
$second = Invoke-Launch 'second' $userDir
if ((Get-Bytes $db) -ne $before) { throw 'second: the database file changed.' }
$text = Get-Content $log -Raw
if (([regex]::Matches($text, 'Migrating the database')).Count -ne 1) { throw 'second: migrated again.' }
if (([regex]::Matches($text, 'Library opened: schema version 5')).Count -ne 2) { throw 'second: library not opened.' }

# 3. Two live processes contend; an idle hard kill leaves a usable profile.
$before = Get-Bytes $db
$saved = $env:LOCALAPPDATA, $env:BINGEE_HOME
$env:LOCALAPPDATA = $userDir
$env:BINGEE_HOME = $null
try {
    $owner = Start-Process -FilePath $exe -WorkingDirectory $cwd -WindowStyle Hidden -PassThru
    $contender = $null
    try {
        $deadline = [DateTime]::UtcNow.AddSeconds(30)
        do {
            Start-Sleep -Milliseconds 50
            $owner.Refresh()
            if ($owner.HasExited) { throw 'lock owner exited early.' }
            if ([DateTime]::UtcNow -gt $deadline) { throw 'lock owner did not open.' }
        } until ($owner.MainWindowHandle -ne 0)
        $contender = Start-Process -FilePath $exe -WorkingDirectory $cwd -WindowStyle Hidden -PassThru
        $deadline = [DateTime]::UtcNow.AddSeconds(30)
        do {
            Start-Sleep -Milliseconds 50
            $contender.Refresh()
            if ($contender.HasExited) { throw 'contender exited before showing its message.' }
            if ([DateTime]::UtcNow -gt $deadline) { throw 'contender did not report the lock.' }
        } until ((Get-Content $log -Raw).Contains('already open for this profile'))
    } finally {
        if ($contender -and -not $contender.HasExited) { $contender.Kill(); $contender.WaitForExit() }
        if (-not $owner.HasExited) { $owner.Kill(); $owner.WaitForExit() }
    }
} finally {
    $env:LOCALAPPDATA, $env:BINGEE_HOME = $saved
}
$afterCrash = Invoke-Launch 'after-idle-kill' $userDir
if ((Get-Bytes $db) -ne $before) { throw 'idle kill or restart changed the database.' }

# 4. Corrupt database.
$badProfile = Join-Path $WorkDir 'profile-corrupt'
$badRoot = Join-Path $badProfile 'Bingee Desktop'
$badDb = Join-Path $badRoot 'data\bingee.db'
New-Item -ItemType Directory -Path (Split-Path $badDb) | Out-Null
$garbage = [byte[]]::new(8192); [Array]::Fill($garbage, [byte]0x5a)
[IO.File]::WriteAllBytes($badDb, $garbage)
$badBefore = Get-Bytes $badDb
$corrupt = Invoke-Launch 'corrupt' $badProfile
if ((Get-Bytes $badDb) -ne $badBefore) { throw 'corrupt: the database file changed.' }
$badFiles = @(Get-ChildItem (Split-Path $badDb) | Select-Object -ExpandProperty Name | Sort-Object)
if (Compare-Object $badFiles @('bingee.db', 'bingee.lock')) { throw "corrupt: unexpected files next to the database: $badFiles" }
$badText = Get-Content (Join-Path $badRoot 'logs\bingee-desktop.log') -Raw
if (-not $badText.Contains('ERROR Startup failed: InvalidData error: The library database is damaged')) { throw "corrupt: failure not logged: $badText" }

# 5. The fixture flag in a default build.
$fixture = Start-Process -FilePath $exe -ArgumentList '--benchmark-fixture' -WorkingDirectory $cwd -WindowStyle Hidden -PassThru -Wait
if ($fixture.ExitCode -ne 1) { throw "fixture flag: expected exit code 1, got $($fixture.ExitCode)." }

$packageAfter = @(Get-ChildItem $package -Recurse -File | ForEach-Object { $_.FullName + '|' + (Get-FileHash $_.FullName).Hash })
if (Compare-Object $packageBefore $packageAfter) { throw 'The package folder changed.' }
if (@(Get-ChildItem $cwd -Force).Count) { throw 'Files were created in the working directory.' }

$summary = [ordered]@{
    utc = [DateTime]::UtcNow.ToString('o')
    package = $package
    executable_sha256 = (Get-FileHash $exe).Hash
    database = @{ path = $db; bytes = (Get-Item $db).Length }
    runs = @($fresh, $second, $afterCrash, $corrupt)
    fixture_flag_exit_code = $fixture.ExitCode
    verdict = 'fresh, reopen, live lock contention, idle hard kill recovery and corrupt-database starts; user data preserved; package unchanged'
}
$summary | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $WorkDir 'smoke.json') -Encoding utf8NoBOM
$summary | ConvertTo-Json -Depth 5
