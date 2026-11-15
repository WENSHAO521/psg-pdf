<#
.SYNOPSIS
  Authenticode-sign files with signtool, using whichever signing material is configured.

.DESCRIPTION
  Tries, in order:
    1. A code-signing certificate file:   WINDOWS_CERTIFICATE (base64 .pfx), WINDOWS_CERTIFICATE_PASSWORD
    2. Azure Trusted Signing:             AZURE_TENANT_ID, AZURE_CLIENT_ID, AZURE_CLIENT_SECRET,
                                          AZURE_SIGNING_ENDPOINT (e.g. https://eus.codesigning.azure.net),
                                          AZURE_SIGNING_ACCOUNT, AZURE_CERT_PROFILE
  The certificate may be self-signed (packaging/windows/new-signing-cert.ps1, free): Windows and
  the updater then accept the files only on machines that trust it (docs/signing-self-signed.md).
  With neither, it prints a warning and leaves the files unsigned (exit code 0), so test builds
  still produce installers.

  This is the one place to adapt when the signing setup changes (an HSM/cloud key, a different
  timestamp server, ...). Optional overrides: WINDOWS_TIMESTAMP_URL, SIGNTOOL (path to signtool.exe).

.EXAMPLE
  pwsh packaging/windows/sign.ps1 dist/psg-pdf.exe dist/Pdfcraft.msi
#>
param(
  [Parameter(Mandatory = $true, ValueFromRemainingArguments = $true)]
  [string[]] $Files
)
$ErrorActionPreference = 'Stop'

function Write-Warn([string] $Message) {
  if ($env:GITHUB_ACTIONS) { Write-Output "::warning::$Message" } else { Write-Warning $Message }
}

function Find-SignTool {
  if ($env:SIGNTOOL) { return $env:SIGNTOOL }
  $onPath = Get-Command signtool.exe -ErrorAction SilentlyContinue
  if ($onPath) { return $onPath.Source }
  $kits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
  $found = Get-ChildItem -Path $kits -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\signtool\.exe$' } |
    Sort-Object FullName -Descending | Select-Object -First 1
  if (-not $found) { throw 'signtool.exe not found (install the Windows SDK)' }
  return $found.FullName
}

function Invoke-SignTool([string[]] $Arguments) {
  & $script:SignTool @Arguments
  if ($LASTEXITCODE -ne 0) { throw "signtool $($Arguments[0]) failed with exit code $LASTEXITCODE" }
}

$Files = $Files | ForEach-Object { (Resolve-Path $_).Path }
$haveCert = [bool]$env:WINDOWS_CERTIFICATE
$azureVars = 'AZURE_TENANT_ID', 'AZURE_CLIENT_ID', 'AZURE_CLIENT_SECRET', 'AZURE_SIGNING_ENDPOINT', 'AZURE_SIGNING_ACCOUNT', 'AZURE_CERT_PROFILE'
$haveAzure = -not ($azureVars | Where-Object { -not [Environment]::GetEnvironmentVariable($_) })

if (-not $haveCert -and -not $haveAzure) {
  Write-Warn "Windows signing secrets not set (WINDOWS_CERTIFICATE or AZURE_*): leaving unsigned: $($Files -join ', ')"
  exit 0
}

$script:SignTool = Find-SignTool
$common = @('sign', '/v', '/fd', 'SHA256', '/td', 'SHA256', '/d', 'PSG PDF', '/du', 'https://github.com/storytold/pdfcraft')
$tmp = Join-Path ([IO.Path]::GetTempPath()) "pdfcraft-sign-$([IO.Path]::GetRandomFileName())"
New-Item -ItemType Directory -Path $tmp | Out-Null

try {
  if ($haveCert) {
    Write-Output "Signing with WINDOWS_CERTIFICATE: $($Files -join ', ')"
    $pfx = Join-Path $tmp 'cert.pfx'
    [IO.File]::WriteAllBytes($pfx, [Convert]::FromBase64String($env:WINDOWS_CERTIFICATE))
    $ts = if ($env:WINDOWS_TIMESTAMP_URL) { $env:WINDOWS_TIMESTAMP_URL } else { 'http://timestamp.digicert.com' }
    $signArgs = $common + @('/tr', $ts, '/f', $pfx)
    if ($env:WINDOWS_CERTIFICATE_PASSWORD) { $signArgs += @('/p', $env:WINDOWS_CERTIFICATE_PASSWORD) }
    Invoke-SignTool ($signArgs + $Files)
    # A self-signed certificate (packaging/windows/new-signing-cert.ps1) chains to no public root,
    # so `signtool verify /pa` below and verify-signatures.sh would call every file untrusted.
    # Trust its public half on the CI runner only (a throwaway machine), so both checks still
    # prove the files carry a valid signature from this certificate. Never on a person's machine:
    # trusting a root is their decision (docs/signing-self-signed.md).
    $cert = [Security.Cryptography.X509Certificates.X509Certificate2]::new($pfx, $env:WINDOWS_CERTIFICATE_PASSWORD)
    if ($cert.Subject -eq $cert.Issuer) {
      if ($env:GITHUB_ACTIONS -eq 'true') {
        $cer = Join-Path $tmp 'cert.cer'
        [IO.File]::WriteAllBytes($cer, $cert.Export([Security.Cryptography.X509Certificates.X509ContentType]::Cert))
        Write-Output "Self-signed certificate ($($cert.Subject), $($cert.Thumbprint)): trusting it on this runner to verify"
        & certutil.exe -f -addstore Root $cer | Out-Null
        if ($LASTEXITCODE -ne 0) { throw "certutil couldn't trust the self-signed certificate on this runner" }
      }
      else {
        Write-Warn "Self-signed certificate ($($cert.Subject)): signtool verify passes only where it is trusted (docs/signing-self-signed.md)"
      }
    }
  }
  else {
    Write-Output "Signing with Azure Trusted Signing ($env:AZURE_SIGNING_ACCOUNT / $env:AZURE_CERT_PROFILE): $($Files -join ', ')"
    # The dlib authenticates with DefaultAzureCredential, which reads AZURE_TENANT_ID,
    # AZURE_CLIENT_ID and AZURE_CLIENT_SECRET from the environment.
    $trustedSigningVersion = '1.0.95'
    $trustedSigningSha256 = '3bfcf1e0a3cb42af1692f0a8ed45c15de070c2de86f28a59b2795d904d8a920f'
    $pkg = Join-Path $tmp 'trusted-signing.zip'
    $uri = "https://api.nuget.org/v3-flatcontainer/microsoft.trusted.signing.client/$trustedSigningVersion/microsoft.trusted.signing.client.$trustedSigningVersion.nupkg"
    Invoke-WebRequest -Uri $uri -OutFile $pkg
    $actualHash = (Get-FileHash -Path $pkg -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($actualHash -ne $trustedSigningSha256) {
      throw "Microsoft.Trusted.Signing.Client $trustedSigningVersion checksum mismatch"
    }
    Expand-Archive -Path $pkg -DestinationPath (Join-Path $tmp 'client') -Force
    $dlib = Get-ChildItem -Path (Join-Path $tmp 'client') -Recurse -Filter Azure.CodeSigning.Dlib.dll |
      Where-Object { $_.FullName -match '\\x64\\' } | Select-Object -First 1
    if (-not $dlib) { throw 'Azure.CodeSigning.Dlib.dll not found in Microsoft.Trusted.Signing.Client' }
    $metadata = Join-Path $tmp 'metadata.json'
    @{
      Endpoint               = $env:AZURE_SIGNING_ENDPOINT
      CodeSigningAccountName = $env:AZURE_SIGNING_ACCOUNT
      CertificateProfileName = $env:AZURE_CERT_PROFILE
    } | ConvertTo-Json | Set-Content -Path $metadata -Encoding utf8
    $ts = if ($env:WINDOWS_TIMESTAMP_URL) { $env:WINDOWS_TIMESTAMP_URL } else { 'http://timestamp.acs.microsoft.com' }
    Invoke-SignTool ($common + @('/tr', $ts, '/dlib', $dlib.FullName, '/dmdf', $metadata) + $Files)
  }
  Invoke-SignTool (@('verify', '/pa', '/v') + $Files)
}
finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
