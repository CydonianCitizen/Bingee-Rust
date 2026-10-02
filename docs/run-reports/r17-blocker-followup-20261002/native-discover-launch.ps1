$ErrorActionPreference = 'Stop'
$root = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$rows = Get-Content (Join-Path $root 'target/r17-native-test-build.jsonl') | ForEach-Object { $_ | ConvertFrom-Json }
$exe = ($rows | Where-Object { $_.reason -eq 'compiler-artifact' -and $_.profile.test -and $_.executable } | Select-Object -Last 1).executable
$profile = Join-Path $root 'target/r17-blocker-followup-20261002/native-discover-fixture'
New-Item -ItemType Directory -Force $profile | Out-Null
$saved = $env:BINGEE_R17_NATIVE_HOME
try {
    $env:BINGEE_R17_NATIVE_HOME = $profile
    $p = Start-Process $exe -ArgumentList '--exact r17_validation::r17_native_keyboard_fixture --ignored --nocapture --test-threads=1' -WorkingDirectory $profile -WindowStyle Normal -PassThru -RedirectStandardOutput (Join-Path $PSScriptRoot 'native-discover-stdout.txt') -RedirectStandardError (Join-Path $PSScriptRoot 'native-discover-stderr.txt')
} finally { $env:BINGEE_R17_NATIVE_HOME = $saved }
$record = @{ pid=$p.Id; executable=$exe; binary_sha256=(Get-FileHash $exe).Hash; profile=$profile; launched_utc=[DateTime]::UtcNow.ToString('o') }
$record | ConvertTo-Json | Set-Content (Join-Path $PSScriptRoot 'native-discover-launch.json')
Write-Output ($record | ConvertTo-Json -Compress)
if (-not $p.WaitForExit(600000)) { throw 'Native Discover validation exceeded its bounded session.' }
$p.WaitForExit()
$record['exit_code'] = $p.ExitCode
$record['exited_utc'] = [DateTime]::UtcNow.ToString('o')
$record | ConvertTo-Json | Set-Content (Join-Path $PSScriptRoot 'native-discover-exit.json')
Get-Content (Join-Path $PSScriptRoot 'native-discover-stdout.txt')
Get-Content (Join-Path $PSScriptRoot 'native-discover-stderr.txt')
if ($p.ExitCode -ne 0) { throw "Native fixture exit $($p.ExitCode)" }
