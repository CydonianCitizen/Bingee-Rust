param(
    [Parameter(Mandatory)] [string] $Package,
    [Parameter(Mandatory)] [string] $Target,
    [switch] $Offline
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$args = @('metadata', '--format-version', '1', '--locked', '--filter-platform', $Target)
if ($Offline) { $args += '--offline' }
$metadata = & cargo @args | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { throw 'cargo metadata failed.' }
$licenseRoot = Join-Path $Package 'licenses'
New-Item -ItemType Directory -Force -Path $licenseRoot | Out-Null
$slint = $metadata.packages | Where-Object name -eq 'slint' | Select-Object -First 1
if (-not $slint) { throw 'Slint package missing from Cargo metadata.' }
$slintLicenses = Join-Path (Split-Path $slint.manifest_path) 'LICENSES'
$index = [Collections.Generic.List[string]]::new()
foreach ($crate in ($metadata.packages | Where-Object name -ne 'bingee-desktop' | Sort-Object name,version)) {
    $source = Split-Path $crate.manifest_path
    $name = '{0}-{1}' -f $crate.name,$crate.version
    $destination = Join-Path $licenseRoot $name
    New-Item -ItemType Directory -Path $destination | Out-Null
    $files = @(Get-ChildItem -LiteralPath $source -File -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -match '^(LICENSE|LICENCE|COPYING|NOTICE|UNLICENSE)' })
    if ($crate.license_file) {
        $declared = Join-Path $source $crate.license_file
        if (Test-Path -LiteralPath $declared) { $files += Get-Item -LiteralPath $declared }
    }
    if ($crate.name -eq 'slint' -or $crate.name -like 'i-slint-*' -or $crate.name -eq 'slint-build' -or $crate.name -eq 'slint-macros') {
        Copy-Item -LiteralPath $slintLicenses -Destination (Join-Path $destination 'LICENSES') -Recurse
    } elseif ($files.Count -eq 0) {
        foreach ($id in @('MIT', 'Apache-2.0', 'BSL-1.0', 'Zlib')) {
            if ($crate.license -match [regex]::Escape($id)) {
                Copy-Item -LiteralPath (Join-Path $root "licenses/common/$id.txt") -Destination $destination
            }
        }
    } else {
        foreach ($file in ($files | Sort-Object FullName -Unique)) {
            Copy-Item -LiteralPath $file.FullName -Destination $destination
        }
    }
    if (-not @(Get-ChildItem -LiteralPath $destination -Force).Count) {
        throw "No license text for $name ($($crate.license))."
    }
    $index.Add("$name : $($crate.license)")
}
$index | Set-Content (Join-Path $licenseRoot 'INDEX.txt') -Encoding utf8NoBOM
Write-Output "License material: $($index.Count) resolved crates in $licenseRoot"
