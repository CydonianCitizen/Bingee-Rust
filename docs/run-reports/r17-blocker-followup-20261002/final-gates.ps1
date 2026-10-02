$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
Set-Location $root
$gates = @(Get-Content (Join-Path $PSScriptRoot 'gates.json') -Raw | ConvertFrom-Json)
Copy-Item (Join-Path $PSScriptRoot 'gates.json') (Join-Path $PSScriptRoot 'pre-native-fixture-gates.json')
$results = @()
foreach ($gate in $gates) {
    $file = Join-Path $PSScriptRoot "$($gate.name).txt"
    if (Test-Path $file) { Copy-Item $file (Join-Path $PSScriptRoot "pre-native-fixture-$($gate.name).txt") }
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $parts = $gate.command.Split(' ')
    $arguments = $parts[1..($parts.Length - 1)]
    & cargo @arguments *> $file
    $code = $LASTEXITCODE
    $timer.Stop()
    $results += @{ name=$gate.name; command=$gate.command; exit_code=$code; elapsed_seconds=$timer.Elapsed.TotalSeconds }
    $results | ConvertTo-Json | Set-Content (Join-Path $PSScriptRoot 'gates.json')
    Write-Output "$($gate.name): exit $code, $([Math]::Round($timer.Elapsed.TotalSeconds,1)) s"
    if ($code -ne 0) { Get-Content $file -Tail 30; exit $code }
}
