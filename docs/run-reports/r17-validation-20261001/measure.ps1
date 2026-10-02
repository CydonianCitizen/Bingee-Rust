param([Parameter(Mandatory)][string]$TestExe)
$ErrorActionPreference = 'Stop'
$out = $PSScriptRoot
$scratch = [IO.Path]::GetFullPath('target/r17-validation-20261001')
$results = @()
foreach ($run in 1..3) {
    $stdout = Join-Path $out "soak-$run.txt"
    $stderr = Join-Path $out "soak-$run-stderr.txt"
    $env:BINGEE_R17_SEED_COPY = Join-Path $scratch 'seed.db'
    $process = Start-Process -FilePath $TestExe -ArgumentList @('--exact','r17_validation::r17_product_soak','--ignored','--nocapture') -WindowStyle Hidden -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
    $env:BINGEE_R17_SEED_COPY = $null
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $samples = @()
    while (-not $process.HasExited) {
        $process.Refresh()
        if ($process.HasExited) { break }
        $samples += [ordered]@{ms=$timer.ElapsedMilliseconds;private_bytes=$process.PrivateMemorySize64;working_set=$process.WorkingSet64;handles=$process.HandleCount;threads=$process.Threads.Count;cpu_ms=$process.TotalProcessorTime.TotalMilliseconds}
        Start-Sleep -Milliseconds 200
    }
    $process.WaitForExit()
    $samples | ConvertTo-Json | Set-Content (Join-Path $out "soak-$run-resources.json")
    $results += [ordered]@{run=$run;exit_code=$process.ExitCode;elapsed_ms=$timer.ElapsedMilliseconds;samples=$samples.Count}
    $results | ConvertTo-Json | Set-Content (Join-Path $out 'soak-results.json')
    Write-Output "Soak $run exit=$($process.ExitCode) samples=$($samples.Count)"
    if ($process.ExitCode -ne 0) { exit 1 }
}
$names = @('detail::tests::informal_tracking_ui_timings','home::tests::informal_large_home_and_calendar_timings','backup::tests::informal_large_backup_timings','statistics_page::tests::informal_statistics_page_timings','library::tests::informal_library_timings','metadata::tests::informal_detail_timings')
$timings = @()
foreach ($name in $names) {
    foreach ($run in 1..3) {
        $label = $name.Split(':')[-1]
        $process = Start-Process -FilePath $TestExe -ArgumentList @('--exact',$name,'--ignored','--nocapture') -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput (Join-Path $out "$label-$run.txt") -RedirectStandardError (Join-Path $out "$label-$run-stderr.txt")
        $code = $process.ExitCode
        $timings += [ordered]@{test=$name;run=$run;exit_code=$code}
        $timings | ConvertTo-Json | Set-Content (Join-Path $out 'timing-results.json')
        if ($code -ne 0) { exit 1 }
    }
    Write-Output "$name PASS (three processes)"
}
$process = Start-Process -FilePath $TestExe -ArgumentList @('--exact','r17_validation::r17_active_write_crash_recovery','--ignored','--nocapture') -WindowStyle Hidden -Wait -PassThru -RedirectStandardOutput (Join-Path $out 'active-crash.txt') -RedirectStandardError (Join-Path $out 'active-crash-stderr.txt')
$code = $process.ExitCode
Write-Output "Active crash exit=$code"
exit $code
