param([Parameter(Mandatory)][string]$RunDirectory)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$runs = @(Get-Content (Join-Path $RunDirectory 'runs.json') -Raw | ConvertFrom-Json)
$rows = @()
foreach ($run in $runs) {
    if (!$run.valid) { continue }
    $samples = @(Import-Csv (Join-Path $RunDirectory "$($run.mode)-$($run.run).csv"))
    $expectedCount = switch ($run.mode) { sqlite { 1250 }; posters { 2650 }; ui { 1450 } }
    if ($samples.Count -ne $expectedCount) { throw "Wrong row count: $($run.mode)-$($run.run): $($samples.Count)" }
    foreach ($sample in $samples) {
        if ([double]$sample.elapsed_ns -lt 0) { throw 'Negative duration' }
        $sample | Add-Member -NotePropertyName process_run -NotePropertyValue $run.run
        $sample | Add-Member -NotePropertyName mode -NotePropertyValue $run.mode
    }
    $rows += $samples
}
$statistics = @($rows | Group-Object category,case | ForEach-Object {
    $group = @($_.Group)
    $values = @($group | ForEach-Object { [double]$_.elapsed_ns / 1000000 } | Sort-Object)
    $n = $values.Count
    $median = if ($n % 2) { $values[[int][math]::Floor($n/2)] } else { ($values[$n/2-1]+$values[$n/2])/2 }
    [pscustomobject]@{
        category=$group[0].category; case=$group[0].case; n=$n
        median_ms=$median; p95_ms=$values[[math]::Ceiling(.95*$n)-1]; min_ms=$values[0]; max_ms=$values[-1]
        hits=($group | Measure-Object hits -Sum).Sum; misses=($group | Measure-Object misses -Sum).Sum
        evictions=($group | Measure-Object evictions -Sum).Sum
    }
})
$statistics | ConvertTo-Json -Depth 5 | Set-Content (Join-Path $RunDirectory 'latency-statistics.json')
$markdown = @('| Category | Case | n | Median ms | p95 ms | Max ms |', '| --- | --- | ---: | ---: | ---: | ---: |')
$markdown += $statistics | ForEach-Object { '| {0} | {1} | {2} | {3:F6} | {4:F6} | {5:F6} |' -f $_.category,$_.case,$_.n,$_.median_ms,$_.p95_ms,$_.max_ms }
$markdown | Set-Content (Join-Path $RunDirectory 'latency-table.md')
$rows | Where-Object { $_.category -eq 'sqlite' -and $_.sample -eq '0' } | Export-Csv (Join-Path $RunDirectory 'sqlite-first-calls.csv') -NoTypeInformation
$statistics | Format-Table category,case,n,median_ms,p95_ms,max_ms
