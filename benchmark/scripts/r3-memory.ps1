<#
.SYNOPSIS
  Informal R3 poster-memory observation for the Rust + Slint spike (Windows).

.DESCRIPTION
  Launches the release build, drives it with the keyboard and records process
  memory at four points: A startup/idle, B initial library, C after a long
  scroll that churns the poster cache, D back at the top after settling.
  At each point it presses F12 (list focused), which makes the app print its
  poster cache counters to stderr, captured here through a redirect.

  Uses only built-in cmdlets and the WScript.Shell COM object. Keys go to the
  foreground window, so do not use the machine while it runs (about 1-2 min).

  With -Fps it sets SLINT_DEBUG_PERFORMANCE=refresh_full_speed,console, so the
  app renders continuously and prints frames per second each second.
  Memory numbers from such a run are not comparable to a normal run.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File benchmark/scripts/r3-memory.ps1
#>
param(
    [string]$Exe = "target/release/bingee-desktop.exe",
    [int]$SettleSeconds = 5,
    [int]$ScrollPasses = 2,
    [int]$KeyDelayMs = 40,
    [switch]$Fps
)
$ErrorActionPreference = 'Stop'

$log = Join-Path $env:TEMP "bingee-r3-stderr-$PID.log"
if ($Fps) { $env:SLINT_DEBUG_PERFORMANCE = 'refresh_full_speed,console' }
$app = Start-Process -FilePath $Exe -PassThru -RedirectStandardError $log
Remove-Item Env:SLINT_DEBUG_PERFORMANCE -ErrorAction SilentlyContinue
$launched = Get-Date
while ($app.MainWindowHandle -eq 0) {
    if ($app.HasExited) { throw "app exited early; see $log" }
    Start-Sleep -Milliseconds 50
    $app.Refresh()
}
$shell = New-Object -ComObject WScript.Shell
Start-Sleep -Seconds 1   # let the window finish showing before sending keys

function Send-Keys([string]$keys, [int]$count = 1) {
    for ($i = 0; $i -lt $count; $i++) {
        [void]$shell.AppActivate($app.Id)
        $shell.SendKeys($keys)
        Start-Sleep -Milliseconds $KeyDelayMs
    }
}

function Measure-Point([string]$point) {
    Start-Sleep -Seconds $SettleSeconds
    Send-Keys '{F12}'
    Start-Sleep -Milliseconds 500
    $app.Refresh()
    $cache = Get-Content $log | Where-Object { $_ -like 'posters:*' } | Select-Object -Last 1
    if (-not $cache) { Write-Warning "$point`: no F12 output; keys did not reach the app, run is invalid" }
    [pscustomobject]@{
        Point         = $point
        WorkingSetMiB = [math]::Round($app.WorkingSet64 / 1MB, 1)
        PrivateMiB    = [math]::Round($app.PrivateMemorySize64 / 1MB, 1)
        PeakWSMiB     = [math]::Round($app.PeakWorkingSet64 / 1MB, 1)
        Cache         = $cache
    }
}

$results = @()
# Tab focuses the search field, Down moves focus to the list (the selection
# stays on the first title), so F12 reaches the list's key handler.
Send-Keys '{TAB}'
Send-Keys '{DOWN}'
$results += Measure-Point 'A startup/idle'

Send-Keys '{DOWN}' 3
Send-Keys '{UP}' 3
$results += Measure-Point 'B initial library'

# Each pass pages through all 1,000 rows down and back up (~8 rows per page),
# with Home/End jumps: ~2,000 rows, ~20x the 100-poster pool per pass.
# Sampling after every pass shows whether memory plateaus or keeps growing.
$scrollStart = Get-Date
for ($pass = 1; $pass -le $ScrollPasses; $pass++) {
    Send-Keys '{PGDN}' 130
    Send-Keys '{HOME}'
    Send-Keys '{END}'
    Send-Keys '{PGUP}' 130
    Send-Keys '{END}'
    $results += Measure-Point "C long scroll, after pass $pass (at end)"
}
$scrollEnd = Get-Date

Send-Keys '{HOME}'
$SettleSeconds *= 2
$results += Measure-Point 'D back at top, settled'

$results | Format-List
"Process: $($app.Id), launched $launched, scroll phase $([int]($scrollEnd - $scrollStart).TotalSeconds) s"
if ($Fps) {
    'FPS lines, one per second for the whole run (scroll phase is the middle):'
    Get-Content $log | Where-Object { $_ -like '*frames per second*' }
}
[void]$app.CloseMainWindow()
if (-not $app.WaitForExit(5000)) { $app.Kill() }
"stderr log: $log"
