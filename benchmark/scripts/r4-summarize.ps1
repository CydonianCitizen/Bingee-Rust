param([Parameter(Mandatory)][string]$RunDirectory)
$ErrorActionPreference = 'Stop'
function Get-Statistics([double[]]$Values) {
    $sorted = @($Values | Sort-Object)
    $n = $sorted.Count
    if ($n -eq 0) { throw 'Cannot summarize zero valid samples.' }
    $middle = [int][math]::Floor($n / 2)
    $median = if ($n % 2) { $sorted[$middle] } else { ($sorted[$middle-1] + $sorted[$middle]) / 2 }
    [ordered]@{ n=$n; median=$median; p95=$sorted[[int][math]::Ceiling(0.95*$n)-1]; min=$sorted[0]; max=$sorted[$n-1] }
}
# A small independent check catches percentile-index and even-median mistakes.
$check = Get-Statistics (1..20)
if ($check.median -ne 10.5 -or $check.p95 -ne 19) { throw 'Statistics self-check failed.' }
if (-not (Test-Path (Join-Path $RunDirectory 'completion.json'))) { throw 'Run is incomplete; inspect raw failure record.' }
$startup = @(Import-Csv (Join-Path $RunDirectory 'startup.csv'))
$valid = @($startup | Where-Object valid -eq 'True')
$idle = @(Import-Csv (Join-Path $RunDirectory 'idle.csv'))
$result = [ordered]@{
    startup_proxy_ms = Get-Statistics @($valid | ForEach-Object { [double]$_.startup_proxy_ms })
    excluded_startup = @($startup | Where-Object valid -ne 'True')
    idle = $idle
    percentile_method = 'Nearest rank: ceil(p*n)-1, zero-based; median averages the two middle observations for even n.'
}
$result | ConvertTo-Json -Depth 8 | Set-Content (Join-Path $RunDirectory 'statistics.json') -Encoding utf8
$result | ConvertTo-Json -Depth 8
