param([Parameter(Mandatory)][string]$Exe, [Parameter(Mandatory)][string]$Seed)
$ErrorActionPreference = 'Stop'
$scratch = [IO.Path]::GetFullPath('target/r17-validation-20261001/startup')
New-Item -ItemType Directory -Force $scratch | Out-Null
$samples = @()
foreach ($mode in @('empty','populated-off','populated-on')) {
    foreach ($run in 1..10) {
        $profile = Join-Path $scratch "$mode-$run"
        New-Item -ItemType Directory -Force (Join-Path $profile 'data') | Out-Null
        if ($mode -ne 'empty') {
            $database = Join-Path $profile 'data/bingee.db'
            Copy-Item -LiteralPath $Seed -Destination $database
            $enabled = if ($mode -eq 'populated-on') { 1 } else { 0 }
            & python -c 'import sqlite3,sys; c=sqlite3.connect(sys.argv[1]); c.execute("UPDATE app_settings SET automatic_refresh_enabled=?",(int(sys.argv[2]),)); c.commit(); c.close()' $database $enabled
            if ($LASTEXITCODE -ne 0) { throw 'Fixture preference setup failed' }
        }
        $savedProfile = $env:BINGEE_HOME
        $env:BINGEE_HOME = $profile
        $timer = [Diagnostics.Stopwatch]::StartNew()
        try { $process = Start-Process -FilePath $Exe -WindowStyle Hidden -PassThru -WorkingDirectory $scratch } finally { $env:BINGEE_HOME = $savedProfile }
        try {
            do {
                Start-Sleep -Milliseconds 20
                $process.Refresh()
                if ($process.HasExited -or $timer.Elapsed.TotalSeconds -gt 30) { throw 'Startup failed' }
            } until ($process.MainWindowHandle -ne 0 -and $process.MainWindowTitle -eq 'Bingee Desktop' -and $process.Responding)
            $ready = $timer.Elapsed.TotalMilliseconds
            $entry = [ordered]@{mode=$mode;run=$run;responsive_window_ms=$ready;pid=$process.Id}
            if ($run -eq 10) {
                Start-Sleep -Seconds 5
                $process.Refresh()
                $cpuBefore = $process.TotalProcessorTime.TotalMilliseconds
                $idleTimer = [Diagnostics.Stopwatch]::StartNew()
                $resources = @()
                foreach ($second in 1..30) {
                    Start-Sleep -Seconds 1
                    $process.Refresh()
                    $resources += @{second=$second;private_bytes=$process.PrivateMemorySize64;working_set=$process.WorkingSet64;threads=$process.Threads.Count;handles=$process.HandleCount}
                }
                $entry.idle_cpu_ms = $process.TotalProcessorTime.TotalMilliseconds - $cpuBefore
                $entry.idle_wall_ms = $idleTimer.Elapsed.TotalMilliseconds
                $entry.idle_one_core_percent = 100 * $entry.idle_cpu_ms / $entry.idle_wall_ms
                $entry.resources = $resources
            }
            $samples += $entry
        } finally {
            [void]$process.CloseMainWindow()
            if (-not $process.WaitForExit(10000)) { $process.Kill(); $process.WaitForExit(); throw 'Graceful close timed out' }
        }
        if ($process.ExitCode -ne 0) { throw "Exit $($process.ExitCode)" }
        $samples | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $PSScriptRoot 'startup-idle.json')
    }
    Write-Output "${mode}: 10 independent starts; 30-second idle sample"
}
