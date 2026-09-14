param([Parameter(Mandatory)][string]$Output)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Path $Output -ErrorAction Stop | Out-Null
$checks = @(
    @('fmt','--check'), @('check'), @('test'),
    @('clippy','--all-targets','--all-features','--','-D','warnings'),
    @('build','--release')
)
$gate = @()
foreach ($arguments in $checks) {
    $log = "gate-$($gate.Count).txt"
    & cargo @arguments *> (Join-Path $Output $log)
    $code = $LASTEXITCODE
    $gate += [pscustomobject]@{command="cargo $($arguments -join ' ')"; exit_code=$code; log=$log}
    $gate | ConvertTo-Json | Set-Content (Join-Path $Output 'gate.json')
    if ($code -ne 0) { throw "Gate failed: cargo $($arguments -join ' '); see $Output/$log" }
}
Get-FileHash target/release/bingee-desktop.exe | ConvertTo-Json | Set-Content (Join-Path $Output 'executable.json')
Get-Content (Join-Path $Output 'gate.json')
Get-Content (Join-Path $Output 'executable.json')
