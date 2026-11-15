<#
.SYNOPSIS
  Make a free, self-signed Authenticode certificate for PSG PDF and PSG Academic PDF.

.DESCRIPTION
  Run once, on your own Windows machine (Windows PowerShell 5.1 or PowerShell 7). It writes, into
  -OutDir:
    psg-codesign.pfx          the certificate with its private key (keep it secret, back it up)
    psg-codesign.pfx.base64   the same, base64: the WINDOWS_CERTIFICATE repository secret
    psg-codesign-password.txt the .pfx password: the WINDOWS_CERTIFICATE_PASSWORD secret
    psg-codesign.cer          the public certificate only: what each computer trusts, safe to share

  The in-app updater installs an update only when the running copy and the download carry valid
  Authenticode signatures from the same publisher. A self-signed certificate is valid only on a
  computer that trusts it, so each computer that should update trusts psg-codesign.cer once
  (docs/signing-self-signed.md). Windows SmartScreen still warns on a first download: only a
  certificate from a public authority builds that reputation.

  The certificate is removed from your certificate store once exported, so the .pfx is its only
  copy. Losing it means a new certificate, and every computer trusting the new .cer.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File packaging\windows\new-signing-cert.ps1 -OutDir $HOME\psg-signing
#>
param(
  [string] $OutDir = (Join-Path $HOME 'psg-signing'),
  [string] $Subject = 'CN=Panorama Scholarly Group',
  [int] $Years = 10
)
$ErrorActionPreference = 'Stop'

if (-not (Get-Command New-SelfSignedCertificate -ErrorAction SilentlyContinue)) {
  throw 'New-SelfSignedCertificate is missing: run this on Windows 10 or later.'
}
$pfx = Join-Path $OutDir 'psg-codesign.pfx'
if (Test-Path -LiteralPath $pfx) {
  # Overwriting would lose the key every trusting computer relies on.
  throw "$pfx already exists. Move it away first if you really want a new certificate."
}
New-Item -ItemType Directory -Force -Path $OutDir | Out-Null

$cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject $Subject `
  -KeyAlgorithm RSA -KeyLength 3072 -HashAlgorithm SHA256 -KeyExportPolicy Exportable `
  -NotAfter (Get-Date).AddYears($Years) -CertStoreLocation Cert:\CurrentUser\My

try {
  # 32 random bytes as base64: no characters that a secret or a shell would mangle.
  $bytes = New-Object byte[] 32
  [Security.Cryptography.RandomNumberGenerator]::Create().GetBytes($bytes)
  $password = [Convert]::ToBase64String($bytes)
  $secure = ConvertTo-SecureString -String $password -AsPlainText -Force

  Export-PfxCertificate -Cert $cert -FilePath $pfx -Password $secure | Out-Null
  Export-Certificate -Cert $cert -FilePath (Join-Path $OutDir 'psg-codesign.cer') | Out-Null
  [IO.File]::WriteAllText((Join-Path $OutDir 'psg-codesign.pfx.base64'), [Convert]::ToBase64String([IO.File]::ReadAllBytes($pfx)))
  [IO.File]::WriteAllText((Join-Path $OutDir 'psg-codesign-password.txt'), $password)
}
finally {
  Remove-Item -LiteralPath "Cert:\CurrentUser\My\$($cert.Thumbprint)" -DeleteKey -ErrorAction SilentlyContinue
}

Write-Output "Made $Subject (thumbprint $($cert.Thumbprint), valid until $($cert.NotAfter.ToString('yyyy-MM-dd'))) in $OutDir"
Write-Output 'Next: add psg-codesign.pfx.base64 as WINDOWS_CERTIFICATE and psg-codesign-password.txt as WINDOWS_CERTIFICATE_PASSWORD'
Write-Output '(GitHub: Settings > Secrets and variables > Actions), then trust psg-codesign.cer on each computer.'
