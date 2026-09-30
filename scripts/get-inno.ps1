$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not $IsWindows) { throw 'Inno Setup is a Windows tool.' }
$root = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$dir = Join-Path $root 'target/tools/inno6'
$compiler = Join-Path $dir 'ISCC.exe'
if (Test-Path -LiteralPath $compiler) {
    if ((Get-FileHash -LiteralPath $compiler -Algorithm SHA256).Hash -ne '0A8757031B33777E4C9CBFFEE40F11A5062B36D25CBE144C1DB73B6102B80AD7') {
        throw 'Unexpected Inno Setup compiler hash.'
    }
    Write-Output $compiler
    exit 0
}
$download = Join-Path $root 'target/tools/innosetup-6.7.3.exe'
New-Item -ItemType Directory -Force (Split-Path $download) | Out-Null
if (-not (Test-Path -LiteralPath $download)) {
    Invoke-WebRequest -Uri 'https://github.com/jrsoftware/issrc/releases/download/is-6_7_3/innosetup-6.7.3.exe' -OutFile $download
}
$expected = '9C73C3BAE7ED48D44112A0F48E66742C00090BDB5BEF71D9D3C056C66E97B732'
if ((Get-FileHash -LiteralPath $download -Algorithm SHA256).Hash -ne $expected) { throw 'Inno Setup download hash mismatch.' }
$signature = Get-AuthenticodeSignature -LiteralPath $download
if ($signature.Status -ne 'Valid' -or $signature.SignerCertificate.Subject -notlike 'CN=Pyrsys B.V.*') {
    throw 'Inno Setup download signature is invalid.'
}
$process = Start-Process -FilePath $download -ArgumentList @('/PORTABLE=1','/SILENT','/CURRENTUSER','/NORESTART',"/DIR=$dir") -WindowStyle Hidden -Wait -PassThru
if ($process.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $compiler)) { throw 'Portable Inno Setup installation failed.' }
if ((Get-FileHash -LiteralPath $compiler -Algorithm SHA256).Hash -ne '0A8757031B33777E4C9CBFFEE40F11A5062B36D25CBE144C1DB73B6102B80AD7') {
    throw 'Installed Inno Setup compiler hash mismatch.'
}
Write-Output $compiler
