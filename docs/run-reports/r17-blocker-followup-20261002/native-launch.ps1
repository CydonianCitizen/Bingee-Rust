param([string]$Name, [ValidateSet('empty','off','on')][string]$Mode, [string]$Scale = '1', [string]$Executable)
$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$profile = Join-Path $root "target/r17-blocker-followup-20261002/$Name"
if (-not $Executable) { $Executable = Join-Path $root 'target/release/bingee-desktop.exe' }
New-Item -ItemType Directory -Force (Join-Path $profile 'data') | Out-Null
$db = Join-Path $profile 'data/bingee.db'
if ($Mode -ne 'empty' -and -not (Test-Path $db)) {
    Copy-Item (Join-Path $root "target/r17-validation-20261001/startup/populated-$Mode-1/data/bingee.db") $db
}
$before = if (Test-Path $db) { (Get-FileHash $db).Hash } else { $null }
$request = Join-Path $PSScriptRoot "$Name-close-request.json"
$savedProfile, $savedScale = $env:BINGEE_HOME, $env:SLINT_SCALE_FACTOR
$started = [DateTime]::UtcNow
try {
    $env:BINGEE_HOME, $env:SLINT_SCALE_FACTOR = $profile, $Scale
    $process = Start-Process $Executable -WorkingDirectory $profile -WindowStyle Normal -PassThru
} finally {
    $env:BINGEE_HOME, $env:SLINT_SCALE_FACTOR = $savedProfile, $savedScale
}
do {
    Start-Sleep -Milliseconds 50
    $process.Refresh()
    if ($process.HasExited) { throw 'Native app exited before usable-window verification.' }
    if (([DateTime]::UtcNow - $started).TotalSeconds -gt 30) { throw 'No responsive window within 30 seconds.' }
} until ($process.MainWindowHandle -ne 0 -and $process.Responding)
$record = [ordered]@{
    name=$Name; mode=$Mode; scale=$Scale; pid=$process.Id; profile=$profile
    binary_sha256=(Get-FileHash $Executable).Hash
    launched_utc=$started.ToString('o'); responsive_window_ms=([DateTime]::UtcNow-$started).TotalMilliseconds
    handle=$process.MainWindowHandle.ToInt64(); threads=$process.Threads.Count; handles=$process.HandleCount
    database_before=$before
}
$record | ConvertTo-Json | Set-Content (Join-Path $PSScriptRoot "$Name-launch.json")
Write-Output ($record | ConvertTo-Json -Compress)
if (-not $process.WaitForExit(600000)) { throw 'Native close was not completed within this bounded validation session.' }
$ended = [DateTime]::UtcNow
$process.WaitForExit()
$record['exit_code'] = $process.ExitCode
$record['process_exited'] = $true
$record['exited_utc'] = $ended.ToString('o')
if (Test-Path $request) {
    $requested = Get-Content $request -Raw | ConvertFrom-Json
    $requestUtc = if ($requested.utc -is [DateTime]) { $requested.utc.ToUniversalTime() } else { [DateTimeOffset]::Parse($requested.utc).UtcDateTime }
    $record['close_method'] = $requested.method
    $record['close_request_to_exit_ms'] = ($ended - $requestUtc).TotalMilliseconds
}
$record['database_after'] = (Get-FileHash $db).Hash
foreach ($file in @($db, (Join-Path $profile 'data/bingee.lock'))) {
    $stream = [IO.File]::Open($file, 'Open', 'ReadWrite', 'None')
    $stream.Dispose()
}
$record['exclusive_database_and_lock_open'] = $true
$record['log_tail'] = @(Get-Content (Join-Path $profile 'logs/bingee-desktop.log') -Tail 5)
$record | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $PSScriptRoot "$Name-close.json")
Write-Output ($record | ConvertTo-Json -Depth 5 -Compress)
