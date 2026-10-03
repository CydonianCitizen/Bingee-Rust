param([Parameter(Mandatory)] [string] $WorkDir)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows) { throw 'Run on Windows.' }
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$version = (cargo metadata --manifest-path (Join-Path $root 'Cargo.toml') --no-deps --format-version 1 --locked | ConvertFrom-Json).packages |
    Where-Object name -eq 'bingee-desktop' | Select-Object -ExpandProperty version
$setup = Join-Path $root "dist/bingee-desktop-$version-windows-x64-setup.exe"
if (-not (Test-Path -LiteralPath $setup)) { throw 'Build installer first.' }
$WorkDir = [IO.Path]::GetFullPath($WorkDir)
if ($WorkDir -eq [IO.Path]::GetPathRoot($WorkDir)) { throw 'WorkDir cannot be a filesystem root.' }
$WorkDir = Join-Path $WorkDir ("run-{0}-{1}" -f [DateTime]::UtcNow.ToString('yyyyMMddHHmmssfff'), $PID)
New-Item -ItemType Directory -Force $WorkDir | Out-Null
$install = Join-Path $WorkDir 'installed'
$profile = Join-Path $WorkDir 'profile'
$database = Join-Path $profile 'data/bingee.db'

function Install-App {
    $process = Start-Process -FilePath $setup -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART','/SP-',"/DIR=$install", "/LOG=$WorkDir/setup.log") -WindowStyle Hidden -PassThru -Wait
    if ($process.ExitCode -ne 0) { throw "Installer exit code $($process.ExitCode)." }
    if (-not (Test-Path -LiteralPath (Join-Path $install 'bingee-desktop.exe'))) { throw 'Executable missing after install.' }
}

function Launch-App {
    $saved = $env:BINGEE_HOME
    $env:BINGEE_HOME = $profile
    try {
        $app = Start-Process -FilePath (Join-Path $install 'bingee-desktop.exe') -WindowStyle Hidden -PassThru
    } finally { $env:BINGEE_HOME = $saved }
    try {
        $deadline = [DateTime]::UtcNow.AddSeconds(30)
        do {
            Start-Sleep -Milliseconds 50
            $app.Refresh()
            if ($app.HasExited) { throw 'Installed app exited early.' }
            if ([DateTime]::UtcNow -gt $deadline) { throw 'Installed app did not open.' }
        } until ($app.MainWindowHandle -ne 0 -and (Test-Path -LiteralPath $database))
    } finally {
        $app.Refresh()
        if (-not $app.HasExited) {
            [void]$app.CloseMainWindow()
            if (-not $app.WaitForExit(5000)) { $app.Kill(); $app.WaitForExit() }
        }
    }
}

Install-App
Launch-App
$before = [Convert]::ToBase64String([IO.File]::ReadAllBytes($database))
Install-App
Launch-App
if ([Convert]::ToBase64String([IO.File]::ReadAllBytes($database)) -ne $before) { throw 'Upgrade changed the existing schema v5 database.' }
$uninstall = Join-Path $install 'unins000.exe'
if (-not (Test-Path -LiteralPath $uninstall)) { throw 'Uninstaller missing.' }
$process = Start-Process -FilePath $uninstall -ArgumentList @('/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART') -WindowStyle Hidden -PassThru -Wait
if ($process.ExitCode -ne 0) { throw "Uninstaller exit code $($process.ExitCode)." }
if (Test-Path -LiteralPath (Join-Path $install 'bingee-desktop.exe')) { throw 'Executable remains after uninstall.' }
if (-not (Test-Path -LiteralPath $database)) { throw 'Uninstall removed user data.' }
Write-Output "Installer smoke passed: install, launch, schema v5 profile, reinstall, uninstall, data retained. Portable smoke covers graceful exit. Evidence: $WorkDir"
