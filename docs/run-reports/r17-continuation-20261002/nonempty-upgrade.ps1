param([Parameter(Mandatory)][string]$WorkDir)
$ErrorActionPreference = 'Stop'
$termination = [Collections.Generic.List[string]]::new()
$root = (Resolve-Path (Join-Path $PSScriptRoot '../../..')).Path
$work = Join-Path ([IO.Path]::GetFullPath($WorkDir)) ("upgrade-" + [DateTime]::UtcNow.ToString('yyyyMMddHHmmssfff'))
$install = Join-Path $work 'installed'
$profile = Join-Path $work 'profile'
$database = Join-Path $profile 'data/bingee.db'
$old = Join-Path $root 'target/r16-revalidation-20261001/windows/bingee-desktop-windows-x64-setup.exe'
$new = Join-Path $root 'dist/bingee-desktop-windows-x64-setup.exe'
New-Item -ItemType Directory -Force (Join-Path $profile 'data') | Out-Null
$inspection = Get-Content (Join-Path $root 'docs/run-reports/r16-native-revalidation-20261001/artifact-inspection.json') -Raw | ConvertFrom-Json
$oldManifest = ($inspection | Where-Object platform -eq 'bingee-Windows-X64').packages
$entry = $oldManifest | Where-Object artifact_filename -eq 'bingee-desktop-windows-x64-setup.exe'
if ($entry.build_commit -ne '6e24b2a109c8fa041075adcbcf1001bc4646cd82' -or
    (Get-FileHash $old).Hash -ne $entry.sha256) { throw 'Old installer provenance mismatch' }

function Install-Candidate([string]$Setup) {
    $p = Start-Process -FilePath $Setup -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/SP-',"/DIR=$install","/LOG=$work/setup.log") -WindowStyle Hidden -PassThru -Wait
    if ($p.ExitCode -ne 0) { throw "Installer exit=$($p.ExitCode)" }
}
function Launch-Candidate {
    $saved = $env:BINGEE_HOME
    $env:BINGEE_HOME = $profile
    try { $p = Start-Process -FilePath (Join-Path $install 'bingee-desktop.exe') -WindowStyle Hidden -PassThru }
    finally { $env:BINGEE_HOME = $saved }
    try {
        $deadline = [DateTime]::UtcNow.AddSeconds(30)
        do {
            Start-Sleep -Milliseconds 50
            $p.Refresh()
            if ($p.HasExited -or [DateTime]::UtcNow -gt $deadline) { throw 'No responsive installed app' }
        } until ($p.MainWindowHandle -ne 0 -and $p.Responding)
        Start-Sleep -Seconds 2
    } finally {
        $closeRequested = $p.CloseMainWindow()
        if ($p.WaitForExit(10000)) { $termination.Add('graceful') }
        else { $p.Kill(); $p.WaitForExit(); $termination.Add(('controlled kill; close requested=' + $closeRequested)) }
    }
    if ($termination[$termination.Count - 1] -eq 'graceful' -and $p.ExitCode -ne 0) { throw "App exit=$($p.ExitCode)" }
    $log = Get-Content (Join-Path $profile 'logs/bingee-desktop.log') -Raw
    if (-not $log.Contains('Library opened: schema version 5, 1000 titles')) { throw 'Populated Library did not open' }
}

Install-Candidate $old
Copy-Item -LiteralPath (Join-Path $root 'target/r17-validation-20261001/seed.db') -Destination $database
Launch-Candidate
$before = (Get-FileHash $database).Hash
Install-Candidate $new
if ((Get-FileHash (Join-Path $install 'bingee-desktop.exe')).Hash -ne (Get-FileHash (Join-Path $root 'target/release/bingee-desktop.exe')).Hash) { throw 'Installed executable mismatch' }
Launch-Candidate
if ((Get-FileHash $database).Hash -ne $before) { throw 'Upgrade changed populated user data' }
$p = Start-Process -FilePath (Join-Path $install 'unins000.exe') -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART') -WindowStyle Hidden -PassThru -Wait
if ($p.ExitCode -ne 0 -or (Test-Path (Join-Path $install 'bingee-desktop.exe'))) { throw 'Uninstall failed' }
if ((Get-FileHash $database).Hash -ne $before) { throw 'Uninstall changed populated user data' }
$result = [ordered]@{
    old_commit = $entry.build_commit
    old_installer_sha256 = (Get-FileHash $old).Hash
    new_installer_sha256 = (Get-FileHash $new).Hash
    database_sha256 = $before
    profile = $profile
    termination = @($termination)
    native_graceful_close_verified = -not (@($termination) | Where-Object { $_ -ne 'graceful' })
    verdict = 'PASS: pushed R16 installer to local corrected candidate; 1000-title nonempty v5 profile opens and remains byte-identical after controlled stop, upgrade and uninstall; see termination scope'
}
$result | ConvertTo-Json | Set-Content (Join-Path $PSScriptRoot 'nonempty-upgrade.json')
$result | ConvertTo-Json


