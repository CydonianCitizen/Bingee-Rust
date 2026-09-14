<#
.SYNOPSIS
Release correctness gate, environment inventory, startup proxy, and idle CPU/memory.
.DESCRIPTION
Run from the checkout root in PowerShell 7 on Windows. No OS settings change.
The executable is not instrumented. Close other Bingee instances first.
Startup is process creation through input-idle plus a responsive main window;
this does NOT prove that posters are painted or that input has been processed.
Every startup sample uses a new process, with warm OS caches (no reboot/flush).
Output directories are unique; failures and incomplete runs remain on disk.
#>
param(
    [string]$Exe = 'target/release/bingee-desktop.exe',
    [string]$OutputRoot = 'docs/measurements/R4-rust-slint/raw',
    [int]$StartupSamples = 20,
    [int]$IdleRuns = 3,
    [int]$SettleSeconds = 5,
    [int]$IdleSeconds = 30
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($StartupSamples -lt 20 -or $IdleRuns -lt 3 -or $IdleSeconds -lt 30) {
    throw 'Formal core suite requires at least 20 startup samples and 3 idle runs of 30 seconds.'
}
$root = (git rev-parse --show-toplevel).Trim()
if ($LASTEXITCODE -ne 0) { throw 'Not a Git checkout.' }
Set-Location $root
$sourcePaths = @('Cargo.toml', 'Cargo.lock', 'build.rs', 'src', 'ui', 'benchmark/assets/posters')
if (git status --porcelain -- $sourcePaths) { throw 'Release inputs differ from HEAD.' }
if (Get-Process bingee-desktop -ErrorAction SilentlyContinue) { throw 'Close existing Bingee processes first.' }
foreach ($name in @('SLINT_BACKEND','SLINT_DEBUG_PERFORMANCE','SLINT_SCALE_FACTOR','RUSTFLAGS','CARGO_ENCODED_RUSTFLAGS','CARGO_TARGET_DIR','CARGO_BUILD_TARGET')) {
    if ([Environment]::GetEnvironmentVariable($name)) { throw "Remove benchmark override $name before running." }
}
$out = Join-Path $root (Join-Path $OutputRoot ([DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ')))
[void](New-Item -ItemType Directory -Path $out)
$commit = (git rev-parse HEAD).Trim()
git status --short --branch | Set-Content (Join-Path $out 'git-status.txt')
function Write-Json($value, [string]$name) {
    $value | ConvertTo-Json -Depth 12 | Set-Content (Join-Path $out $name) -Encoding utf8
}
function Read-Optional([scriptblock]$query) {
    try { & $query } catch { @{ unavailable = $_.Exception.Message } }
}
$gate = @()
foreach ($check in @(
    @{ tool='rustc'; arguments=@('--version') },
    @{ tool='cargo'; arguments=@('--version') },
    @{ tool='cargo'; arguments=@('fmt','--check') },
    @{ tool='cargo'; arguments=@('check') },
    @{ tool='cargo'; arguments=@('test') },
    @{ tool='cargo'; arguments=@('clippy','--all-targets','--all-features','--','-D','warnings') },
    @{ tool='cargo'; arguments=@('build','--release') }
)) {
    $arguments = $check.arguments
    $name = $check.tool + ' ' + ($arguments -join ' ')
    $log = Join-Path $out ('gate-' + $gate.Count + '.txt')
    & $check.tool @arguments *> $log
    $code = $LASTEXITCODE
    $gate += @{ command=$name; exit_code=$code; log=(Split-Path $log -Leaf) }
    Write-Json $gate 'gate.json'
    if ($code -ne 0) { throw "Correctness gate failed: $name; see $log. No measurements collected." }
}
$exePath = (Resolve-Path $Exe).Path
$builtExe = (Resolve-Path (Join-Path $root 'target/release/bingee-desktop.exe')).Path
if ($exePath -ne $builtExe) { throw 'Exe must be the release artifact just built: target/release/bingee-desktop.exe.' }
$database = Join-Path (Split-Path $exePath) 'bingee-spike.db'
if (-not (Test-Path $database)) { throw 'Existing seeded database required. Launch normally once, then rerun.' }
$assetDir = Join-Path $root 'benchmark/assets/posters'
foreach ($line in Get-Content (Join-Path $assetDir 'SHA256SUMS')) {
    if ($line -notmatch '^([a-fA-F0-9]{64})\s+\*?(.+)$') { throw 'Invalid asset manifest.' }
    if ((Get-FileHash (Join-Path $assetDir $Matches[2])).Hash -ne $Matches[1]) { throw "Asset checksum mismatch: $line" }
}
$assets = @(Get-ChildItem $assetDir -Filter '*.jpg')
if ($assets.Count -ne 100) { throw 'Expected 100 posters.' }
$environment = [ordered]@{
    utc = [DateTime]::UtcNow.ToString('o'); commit = $commit
    os = Read-Optional { Get-CimInstance Win32_OperatingSystem | Select-Object Caption,Version,BuildNumber,OSArchitecture }
    cpu = Read-Optional { Get-CimInstance Win32_Processor | Select-Object Name,NumberOfCores,NumberOfLogicalProcessors }
    ram = Read-Optional { Get-CimInstance Win32_ComputerSystem | Select-Object TotalPhysicalMemory }
    gpu = Read-Optional { Get-CimInstance Win32_VideoController | Select-Object Name,DriverVersion,CurrentHorizontalResolution,CurrentVerticalResolution,CurrentRefreshRate }
    battery = Read-Optional { Get-CimInstance Win32_Battery | Select-Object BatteryStatus,EstimatedChargeRemaining }
    ac = Read-Optional { Get-CimInstance -Namespace root/wmi -ClassName BatteryStatus | Select-Object PowerOnline,Charging,Discharging }
    defender = Read-Optional { Get-MpComputerStatus | Select-Object AntivirusEnabled,RealTimeProtectionEnabled }
    power_scheme = (powercfg /getactivescheme | Out-String).Trim()
    display_registry = Read-Optional { Get-ItemProperty 'HKCU:/Control Panel/Desktop' | Select-Object LogPixels,Win8DpiScaling }
    per_monitor_dpi = Read-Optional { Get-ChildItem 'HKCU:/Control Panel/Desktop/PerMonitorSettings' | Get-ItemProperty | Select-Object PSChildName,DpiValue }
    debugger = 'Launched directly by Start-Process; no debugger attached by this suite; external attachment not independently detected.'
    profile = 'cargo build --release; unchanged Cargo default release profile'
    runtime_overrides = 'SLINT_BACKEND, SLINT_DEBUG_PERFORMANCE, SLINT_SCALE_FACTOR, RUSTFLAGS, CARGO_ENCODED_RUSTFLAGS absent'
    tools = Read-Optional { Get-Command vmmap,vmmap64,wpr,wpa -ErrorAction SilentlyContinue | Select-Object Name,Source }
    exe = @{ path=$exePath; bytes=(Get-Item $exePath).Length; sha256=(Get-FileHash $exePath).Hash }
    database = @{ bytes=(Get-Item $database).Length; sha256=(Get-FileHash $database).Hash }
    posters = @{ count=$assets.Count; bytes=($assets | Measure-Object Length -Sum).Sum; min_bytes=($assets | Measure-Object Length -Minimum).Minimum; max_bytes=($assets | Measure-Object Length -Maximum).Maximum; manifest_sha256=(Get-FileHash (Join-Path $assetDir 'SHA256SUMS')).Hash }
    procedure = @{ startup_samples=$StartupSamples; idle_runs=$IdleRuns; settle_seconds=$SettleSeconds; idle_seconds=$IdleSeconds; polling_ms=10; cache_state='warm OS caches; fresh process'; startup_endpoint='WaitForInputIdle plus nonzero MainWindowHandle and Process.Responding; not paint completion'; window='unchanged preferred 1280x800 logical pixels; actual bounds not verified by this core suite' }
}
Write-Json $environment 'environment.json'
$target = ((rustc -vV | Select-String '^host:').ToString() -replace '^host:\s*','').Trim()
cargo tree --locked --target $target --edges normal,build --prefix none --format '{p}' > (Join-Path $out 'dependencies-runtime-build.txt')
if ($LASTEXITCODE -ne 0) { throw 'Dependency inventory failed.' }
cargo tree --locked --target $target --edges normal,build,dev --prefix none --format '{p}' > (Join-Path $out 'dependencies-all.txt')
if ($LASTEXITCODE -ne 0) { throw 'Dependency inventory failed.' }
cargo tree --locked --target $target --edges features -i slint > (Join-Path $out 'slint-features.txt')
if ($LASTEXITCODE -ne 0) { throw 'Feature inventory failed.' }
function Launch([string]$label) {
    $timer = [Diagnostics.Stopwatch]::StartNew()
    # This is an interactive GUI under test, intentionally visible.
    $process = Start-Process -FilePath $exePath -PassThru -RedirectStandardError (Join-Path $out "$label-stderr.txt")
    try {
        if (-not $process.WaitForInputIdle(15000)) { throw 'Input-idle timeout.' }
        do {
            $process.Refresh()
            if ($process.HasExited) { throw 'Process exited before startup endpoint.' }
            if ($timer.Elapsed.TotalSeconds -gt 20) { throw 'Main-window timeout.' }
            if ($process.MainWindowHandle -ne 0 -and $process.MainWindowTitle -eq 'Bingee Desktop' -and $process.Responding) { break }
            Start-Sleep -Milliseconds 10
        } while ($true)
        return @{ process=$process; startup_ms=$timer.Elapsed.TotalMilliseconds }
    } catch {
        if (-not $process.HasExited) { $process.Kill(); $process.WaitForExit() }
        throw
    }
}
function Close-BenchmarkProcess($process) {
    # MainWindowHandle can change while the backend creates its real window.
    $process.Refresh()
    [void]$process.CloseMainWindow()
    if (-not $process.WaitForExit(5000)) {
        $process.Kill(); $process.WaitForExit()
        throw 'Graceful exit failed; forced exit invalidates run.'
    }
    if ($process.ExitCode -ne 0) { throw "Nonzero process exit: $($process.ExitCode)" }
    $process.Dispose()
}
try {
    for ($sample = 1; $sample -le $StartupSamples; $sample++) {
        $run = Launch "startup-$sample"
        $row = [pscustomobject]@{ commit=$commit; sample=$sample; pid=$run.process.Id; startup_proxy_ms=$run.startup_ms; valid=$true; reason='' }
        # Outside the timed interval; allow first paint/startup to finish before closing.
        Start-Sleep -Seconds 1
        try { Close-BenchmarkProcess $run.process } catch { $row.valid=$false; $row.reason=$_.Exception.Message }
        $row | Export-Csv (Join-Path $out 'startup.csv') -Append -NoTypeInformation
        if (-not $row.valid) { throw $row.reason }
        Start-Sleep -Milliseconds 250
    }
    for ($sample = 1; $sample -le $IdleRuns; $sample++) {
        $run = Launch "idle-$sample"
        $process = $run.process
        try {
            Start-Sleep -Seconds $SettleSeconds
            $process.Refresh()
            $cpuStart = $process.TotalProcessorTime.TotalSeconds
            $timer = [Diagnostics.Stopwatch]::StartNew()
            $startPrivate = $process.PrivateMemorySize64
            for ($second = 1; $second -le $IdleSeconds; $second++) {
                Start-Sleep -Seconds 1
                $process.Refresh()
                if ($process.HasExited) { throw 'Process exited during idle interval.' }
                [pscustomobject]@{ commit=$commit; run=$sample; elapsed_s=$timer.Elapsed.TotalSeconds; cpu_s=$process.TotalProcessorTime.TotalSeconds-$cpuStart; working_set_bytes=$process.WorkingSet64; private_bytes=$process.PrivateMemorySize64; threads=$process.Threads.Count; handles=$process.HandleCount } |
                    Export-Csv (Join-Path $out 'idle-samples.csv') -Append -NoTypeInformation
            }
            $elapsed = $timer.Elapsed.TotalSeconds
            $cpu = $process.TotalProcessorTime.TotalSeconds - $cpuStart
            [pscustomobject]@{ commit=$commit; run=$sample; elapsed_s=$elapsed; cpu_s=$cpu; percent_one_core=100*$cpu/$elapsed; percent_machine=100*$cpu/$elapsed/[Environment]::ProcessorCount; private_start_bytes=$startPrivate; private_end_bytes=$process.PrivateMemorySize64; working_set_end_bytes=$process.WorkingSet64 } |
                Export-Csv (Join-Path $out 'idle.csv') -Append -NoTypeInformation
            $process.Modules | Select-Object ModuleName,ModuleMemorySize | Export-Csv (Join-Path $out "idle-$sample-modules.csv") -NoTypeInformation
        } finally { Close-BenchmarkProcess $process }
    }
    if ((Get-FileHash $exePath).Hash -ne $environment.exe.sha256) { throw 'Executable changed during measurement.' }
    if ((Get-FileHash $database).Hash -ne $environment.database.sha256) { throw 'Database changed during measurement.' }
    Write-Json @{ status='core complete; interaction and visual measurements still required'; commit=$commit; utc=[DateTime]::UtcNow.ToString('o') } 'completion.json'
} catch {
    Write-Json @{ status='failed or incomplete'; error=$_.Exception.Message; utc=[DateTime]::UtcNow.ToString('o') } 'failure.json'
    throw
}
Write-Output $out
