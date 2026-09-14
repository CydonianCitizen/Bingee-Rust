<# Separate diagnostic run; never merge its CPU/memory with the baseline. #>
param([Parameter(Mandatory)][string]$CoreRunDirectory)
$ErrorActionPreference = 'Stop'
$record = Get-Content (Join-Path $CoreRunDirectory 'environment.json') -Raw | ConvertFrom-Json
if (-not (Test-Path (Join-Path $CoreRunDirectory 'completion.json'))) { throw 'Completed core gate/run required.' }
if ((Get-FileHash $record.exe.path).Hash -ne $record.exe.sha256) { throw 'Release executable hash differs.' }
if (Get-Process bingee-desktop -ErrorAction SilentlyContinue) { throw 'Close existing Bingee first.' }
$out = Join-Path $CoreRunDirectory ('lazy-' + [DateTime]::UtcNow.ToString('yyyyMMddTHHmmssfffZ'))
[void](New-Item -ItemType Directory $out)
$previous = [Environment]::GetEnvironmentVariable('SLINT_DEBUG_PERFORMANCE')
try {
    $env:SLINT_DEBUG_PERFORMANCE = 'refresh_lazy,console'
    # Visible GUI under test; no injected input or deliberate focus change.
    $app = Start-Process $record.exe.path -PassThru -RedirectStandardError (Join-Path $out 'stderr.txt')
} finally {
    [Environment]::SetEnvironmentVariable('SLINT_DEBUG_PERFORMANCE', $previous)
}
try {
    $deadline = [DateTime]::UtcNow.AddSeconds(20)
    do {
        Start-Sleep -Milliseconds 20
        $app.Refresh()
        if ($app.HasExited -or [DateTime]::UtcNow -gt $deadline) { throw 'Window startup failed.' }
    } until ($app.MainWindowTitle -eq 'Bingee Desktop' -and $app.Responding)
    Start-Sleep -Seconds 5
    $start = [DateTime]::UtcNow
    Start-Sleep -Seconds 30
    @{ commit=$record.commit; exe_sha256=$record.exe.sha256; mode='refresh_lazy,console'; idle_start_utc=$start.ToString('o'); idle_end_utc=[DateTime]::UtcNow.ToString('o'); focus='not changed or independently observed'; observation='stderr only; no visual verification' } |
        ConvertTo-Json | Set-Content (Join-Path $out 'interval.json')
} finally {
    $app.Refresh()
    [void]$app.CloseMainWindow()
    if (-not $app.WaitForExit(5000)) { $app.Kill(); $app.WaitForExit(); throw 'Diagnostic close failed.' }
}
Write-Output $out
