<# Run only missing R4 latency cohorts. Build/checks must precede this script. #>
param([string]$OutputRoot = 'docs/measurements/R4-rust-slint/raw')
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ((git branch --show-current).Trim() -ne 'spike/rust-slint') { throw 'Wrong branch' }
if ((git rev-parse HEAD).Trim() -ne '95feb454864907a29bcf84f629278dc861e10336') { throw 'Wrong benchmark commit' }
foreach ($name in @('SLINT_BACKEND','SLINT_DEBUG_PERFORMANCE','SLINT_SCALE_FACTOR','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','CARGO_BUILD_TARGET','CARGO_TARGET_DIR')) {
    if ([Environment]::GetEnvironmentVariable($name)) { throw "Unexpected override $name" }
}
if (Get-Process bingee-desktop,bingee-r4-latency,bingee-r4-frozen -ErrorAction SilentlyContinue) { throw 'Close existing benchmark processes' }
$exe = (Resolve-Path 'target/release/bingee-r4-latency.exe').Path
$database = (Resolve-Path 'target/release/bingee-spike.db').Path
$databaseHash = '61918DB3B71A4B0F77154ABB498268F8D7F327469355B569AAD993B58C69FEC3'
if ((Get-FileHash $database).Hash -ne $databaseHash) { throw 'Frozen database mismatch' }
foreach ($line in Get-Content benchmark/assets/posters/SHA256SUMS) {
    if ($line -notmatch '^([a-fA-F0-9]{64})\s+\*?(.+)$') { throw 'Bad poster manifest' }
    if ((Get-FileHash (Join-Path 'benchmark/assets/posters' $Matches[2])).Hash -ne $Matches[1]) { throw 'Poster mismatch' }
}
$out = Join-Path (Resolve-Path $OutputRoot).Path ('latency-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
New-Item -ItemType Directory $out | Out-Null
$inputs = @('Cargo.toml','Cargo.lock','build.rs','src/main.rs','src/library.rs','src/db.rs','src/poster.rs','ui/app-window.slint','benchmark/r4_latency.rs','benchmark/scripts/r4-latency.ps1')
$hashes = @($inputs | ForEach-Object { [pscustomobject]@{path=$_; sha256=(Get-FileHash $_).Hash} })
$environment = [ordered]@{
    utc=[DateTime]::UtcNow.ToString('o'); commit=(git rev-parse HEAD).Trim()
    executable=(Get-FileHash $exe).Hash; executable_bytes=(Get-Item $exe).Length
    frozen_executable=(Get-FileHash target/release/bingee-r4-frozen.exe).Hash
    database=$databaseHash; inputs=$hashes
    os=(Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,BuildNumber,OSArchitecture)
    cpu=(Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors)
    ac=(Get-CimInstance -Namespace root/wmi -ClassName BatteryStatus | Select-Object PowerOnline,Charging,Discharging)
    defender=(Get-MpComputerStatus | Select-Object AntivirusEnabled,RealTimeProtectionEnabled)
    power_scheme=(powercfg /getactivescheme | Out-String).Trim()
    rust=(rustc --version | Out-String).Trim(); cargo=(cargo --version | Out-String).Trim()
    profile='release, r4-measurement feature; original Slint/default dependency features'
    cache_state='OS file caches warm; separate processes; SQLite page cache warmed by existing open count transaction'
    profiler='none attached by runner'; debugger='none attached by runner'
}
$environment | ConvertTo-Json -Depth 10 | Set-Content (Join-Path $out 'environment.json')
git diff -- Cargo.toml src/main.rs src/library.rs | Set-Content (Join-Path $out 'instrumentation.patch')
Copy-Item benchmark/r4_latency.rs (Join-Path $out 'r4_latency.rs')
Copy-Item benchmark/scripts/r4-latency.ps1 (Join-Path $out 'runner.ps1')
$runs = @()
foreach ($mode in @('sqlite','posters','ui')) {
    for ($run=1; $run -le 3; $run++) {
        $csv = Join-Path $out "$mode-$run.csv"
        $started = [DateTime]::UtcNow.ToString('o')
        # UI cohort intentionally opens the actual application for callback timing.
        $style = if ($mode -eq 'ui') { 'Normal' } else { 'Hidden' }
        $process = Start-Process -FilePath $exe -ArgumentList @('--r4-latency',$mode,('"'+$csv+'"')) -WindowStyle $style -PassThru -RedirectStandardError (Join-Path $out "$mode-$run-stderr.txt")
        $timedOut = $false
        while (!$process.WaitForExit(1000)) {
            if (([DateTime]::UtcNow-[DateTime]::Parse($started)).TotalSeconds -gt 180) {
                $timedOut=$true
                $process.Kill()
                $process.WaitForExit()
                break
            }
        }
        $valid = !$timedOut -and $process.ExitCode -eq 0 -and (Test-Path ([IO.Path]::ChangeExtension($csv,'complete')))
        $reason = if ($valid) { '' } elseif ($timedOut) { '180 second timeout; forced exit' } else { 'nonzero exit or missing completion marker; see stderr' }
        $runs += [pscustomobject]@{mode=$mode; run=$run; pid=$process.Id; started_utc=$started; ended_utc=[DateTime]::UtcNow.ToString('o'); exit_code=$process.ExitCode; valid=$valid; reason=$reason}
        $runs | ConvertTo-Json | Set-Content (Join-Path $out 'runs.json')
        $process.Dispose()
        Write-Output "$mode-$run valid=$valid $reason"
    }
}
foreach ($inputFile in $hashes) {
    if ((Get-FileHash $inputFile.path).Hash -ne $inputFile.sha256) { throw "Source changed during run: $($inputFile.path)" }
}
if ((Get-FileHash $exe).Hash -ne $environment.executable) { throw 'Executable changed during run' }
if ((Get-FileHash $database).Hash -ne $databaseHash) { throw 'Database changed during run' }
@{utc=[DateTime]::UtcNow.ToString('o'); valid_runs=@($runs | Where-Object valid).Count; invalid_runs=@($runs | Where-Object { !$_.valid }).Count; inputs_unchanged=$true} | ConvertTo-Json | Set-Content (Join-Path $out 'verification.json')
Write-Output $out
