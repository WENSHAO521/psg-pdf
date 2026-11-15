<#
.SYNOPSIS
  Install, run and uninstall both PSG products' MSIs, as a user would (per machine, silently):
  each on its own and side by side, uninstalling either leaves the other, and, given a 0.18.x
  "PSG PDF Suite" MSI, the migration from it (docs/products.md, "Migrating from the suite").
.DESCRIPTION
  Changes the machine: run it on a throwaway VM or CI runner, as an administrator.
  - PSG PDF continues the suite's UpgradeCode, so installing it replaces the suite.
  - PSG Academic PDF refuses to install over the suite (it would leave two copies of itself), and
    installs once PSG PDF has replaced it.
.EXAMPLE
  pwsh packaging/windows/test-install.ps1 -Dir dist/release -SuiteMsi suite/psg-pdf-0.18.1-beta.7-windows-x64.msi
#>
param(
  [Parameter(Mandatory)] [string] $Dir,
  [string] $SuiteMsi = ''
)
$ErrorActionPreference = 'Stop'
$Logs = Join-Path ([IO.Path]::GetTempPath()) 'psg-install-logs'
New-Item -ItemType Directory -Force -Path $Logs | Out-Null

function Find-Msi([string] $Stem) {
  $msi = Get-ChildItem -LiteralPath $Dir -Filter "$Stem-*-windows-x64.msi" | Where-Object { $_.Name -match "^$Stem-\d" } | Select-Object -First 1
  if (-not $msi) { throw "no $Stem MSI in $Dir" }
  return $msi.FullName
}
$PdfMsi = Find-Msi 'psg-pdf'
$AcademicMsi = Find-Msi 'psg-academic-pdf'
Write-Host "PSG PDF: $PdfMsi"
Write-Host "PSG Academic PDF: $AcademicMsi"

function Step([string] $What) { Write-Host ''; Write-Host "==> $What" }
function Invoke-Msi([string] $Verb, [string] $Msi, [string] $Name) {
  $log = Join-Path $Logs ("{0}-{1}-{2}.log" -f $Name, $Verb.TrimStart('/'), (Get-Date -Format 'HHmmssfff'))
  $p = Start-Process msiexec.exe -ArgumentList @($Verb, "`"$Msi`"", '/qn', 'ALLUSERS=1', '/l*v', "`"$log`"") -Wait -PassThru
  return @{ Code = $p.ExitCode; Log = $log }
}
function Install([string] $Msi, [string] $Name) {
  $r = Invoke-Msi '/i' $Msi $Name
  if ($r.Code -notin 0, 3010) { Get-Content -LiteralPath $r.Log -Tail 60 | Write-Host; throw "installing $Name failed ($($r.Code))" }
}
function Uninstall([string] $Msi, [string] $Name) {
  $r = Invoke-Msi '/x' $Msi $Name
  if ($r.Code -notin 0, 3010) { Get-Content -LiteralPath $r.Log -Tail 60 | Write-Host; throw "uninstalling $Name failed ($($r.Code))" }
}
# Programs and Features entries, from both registry views.
function Get-Entries([string] $DisplayName) {
  @('HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\*', 'HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*') |
    ForEach-Object { Get-ItemProperty -Path $_ -ErrorAction SilentlyContinue } |
    Where-Object { $_.PSObject.Properties['DisplayName'] -and $_.DisplayName -eq $DisplayName }
}
function Get-AppPath([string] $Exe) {
  foreach ($key in "HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\$Exe", "HKLM:\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\App Paths\$Exe") {
    $v = (Get-ItemProperty -LiteralPath $key -ErrorAction SilentlyContinue).'(default)'
    if ($v) { return $v.Trim('"') }
  }
  return $null
}
function Assert-Installed([string] $Name, [string] $Exe, [string[]] $Arguments) {
  $entries = @(Get-Entries $Name)
  if ($entries.Count -ne 1) { throw "expected one '$Name' entry in Programs and Features, found $($entries.Count)" }
  $path = Get-AppPath $Exe
  if (-not $path -or -not (Test-Path -LiteralPath $path)) { throw "$Name's $Exe is not installed (App Paths: '$path')" }
  foreach ($a in $Arguments) {
    $p = Start-Process -FilePath $path -ArgumentList $a -Wait -PassThru -WindowStyle Hidden
    if ($p.ExitCode -ne 0) { throw "$path $a exited with $($p.ExitCode)" }
  }
  Write-Host "  $Name installed at $path"
  return $path
}
function Assert-Gone([string] $Name, [string] $Exe, [string] $OldPath) {
  if (@(Get-Entries $Name).Count -ne 0) { throw "'$Name' is still in Programs and Features" }
  if (Get-AppPath $Exe) { throw "$Exe still has an App Paths entry" }
  if ($OldPath -and (Test-Path -LiteralPath $OldPath)) { throw "$OldPath is still there" }
}

Step 'PSG PDF alone'
Install $PdfMsi 'psg-pdf'
$pdfExe = Assert-Installed 'PSG PDF' 'psg-pdf.exe' @('--version')
if (Get-AppPath 'psg-academic-pdf.exe') { throw 'PSG PDF installed psg-academic-pdf.exe' }

Step 'PSG Academic PDF beside PSG PDF'
Install $AcademicMsi 'psg-academic-pdf'
$academicExe = Assert-Installed 'PSG Academic PDF' 'psg-academic-pdf.exe' @('--version', 'templates')
if ((Split-Path $pdfExe) -eq (Split-Path $academicExe)) { throw 'both products were installed into one folder' }
Assert-Installed 'PSG PDF' 'psg-pdf.exe' @('--version') | Out-Null

Step 'Uninstalling PSG Academic PDF leaves PSG PDF'
Uninstall $AcademicMsi 'psg-academic-pdf'
Assert-Gone 'PSG Academic PDF' 'psg-academic-pdf.exe' $academicExe
Assert-Installed 'PSG PDF' 'psg-pdf.exe' @('--version') | Out-Null

Step 'Uninstalling PSG PDF leaves PSG Academic PDF'
Install $AcademicMsi 'psg-academic-pdf'
Uninstall $PdfMsi 'psg-pdf'
Assert-Gone 'PSG PDF' 'psg-pdf.exe' $pdfExe
Assert-Installed 'PSG Academic PDF' 'psg-academic-pdf.exe' @('--version') | Out-Null
Uninstall $AcademicMsi 'psg-academic-pdf'
Assert-Gone 'PSG Academic PDF' 'psg-academic-pdf.exe' $academicExe

if ($SuiteMsi) {
  Step "Migrating from the PSG PDF Suite ($SuiteMsi)"
  Install $SuiteMsi 'suite'
  $suiteEntries = @(Get-Entries 'PSG PDF Suite')
  if ($suiteEntries.Count -ne 1) { throw "the suite did not install ('PSG PDF Suite' entries: $($suiteEntries.Count))" }
  $suiteAcademic = Get-AppPath 'psg-academic-pdf.exe'
  if (-not $suiteAcademic) {
    $suiteAcademic = Join-Path (Split-Path (Get-AppPath 'psg-pdf.exe')) 'psg-academic-pdf.exe'
  }
  # PSG Academic PDF first: it must refuse, not install a second copy beside the suite's.
  $r = Invoke-Msi '/i' $AcademicMsi 'academic-over-suite'
  if ($r.Code -ne 1603) { throw "PSG Academic PDF over the suite: expected the launch condition to stop it (1603), got $($r.Code)" }
  if (-not (Select-String -LiteralPath $r.Log -Pattern 'PSG PDF Suite' -Quiet)) { throw 'the refusal does not name the suite' }
  Write-Host '  PSG Academic PDF refused to install over the suite, as it should'
  # PSG PDF replaces the suite (major upgrade over the same UpgradeCode).
  Install $PdfMsi 'psg-pdf-over-suite'
  if (@(Get-Entries 'PSG PDF Suite').Count -ne 0) { throw 'the suite is still installed after PSG PDF replaced it' }
  if (Test-Path -LiteralPath $suiteAcademic) { throw "the suite's $suiteAcademic is still there" }
  $pdfExe = Assert-Installed 'PSG PDF' 'psg-pdf.exe' @('--version')
  Install $AcademicMsi 'psg-academic-pdf-after-suite'
  $academicExe = Assert-Installed 'PSG Academic PDF' 'psg-academic-pdf.exe' @('--version')
  Uninstall $AcademicMsi 'psg-academic-pdf'
  Uninstall $PdfMsi 'psg-pdf'
  Assert-Gone 'PSG PDF' 'psg-pdf.exe' $pdfExe
  Assert-Gone 'PSG Academic PDF' 'psg-academic-pdf.exe' $academicExe
}

Write-Host ''
Write-Host 'install tests passed'
