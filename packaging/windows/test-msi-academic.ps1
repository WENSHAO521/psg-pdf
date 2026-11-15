<#
.SYNOPSIS
  Check PSG Academic PDF's compiled MSI without installing it: its own identity, dual scope,
  shortcuts, manuscript associations, templates, and the old suite's detection.
.EXAMPLE
  pwsh packaging/windows/test-msi-academic.ps1 dist/release/psg-academic-pdf-0.19.0-windows-x64.msi
#>
param([Parameter(Mandatory)] [string] $Path)
$ErrorActionPreference = 'Stop'
$Installer = New-Object -ComObject WindowsInstaller.Installer
$Database = $Installer.OpenDatabase((Resolve-Path -LiteralPath $Path).Path, 0)

function Read-Rows([string] $Sql, [int] $Columns) {
  $view = $Database.OpenView($Sql)
  try {
    [void] $view.Execute()
    while ($record = $view.Fetch()) { ,@(for ($i = 1; $i -le $Columns; $i++) { $record.StringData($i) }) }
  } finally { [void] $view.Close() }
}
function Read-Row([string] $Sql, [int] $Columns) {
  $rows = @(Read-Rows $Sql $Columns)
  if ($rows.Count -lt 1) { throw "MSI row missing: $Sql" }
  return ,$rows[0]
}
function Assert-Equal($Actual, $Expected, [string] $What) {
  if ($Actual -cne $Expected) { throw "$What`: expected '$Expected', got '$Actual'" }
}

# The shared dual-scope contract, against this product's policy (separate process: one
# Windows Installer session per process).
& (Get-Process -Id $PID).Path -NoProfile -ExecutionPolicy Bypass -File (Join-Path $PSScriptRoot 'check-install-policy.ps1') -MsiPath $Path -PolicyPath (Join-Path $PSScriptRoot 'install-policy-academic.json')
if ($LASTEXITCODE -ne 0) { throw 'Dual-scope installer contract failed.' }
$policy = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'install-policy-academic.json') | ConvertFrom-Json

$props = @{}
foreach ($row in (Read-Rows 'SELECT `Property`, `Value` FROM `Property`' 2)) { $props[$row[0]] = $row[1] }
Assert-Equal $props['ProductName'] 'PSG Academic PDF' 'Product name'
Assert-Equal $props['Manufacturer'] 'Panorama Scholarly Group' 'Manufacturer'
Assert-Equal $props['UpgradeCode'].Trim('{}') $policy.upgradeCode 'UpgradeCode (its own, never the suite''s)'
Assert-Equal $props['ALLUSERS'] '2' 'Dual-scope package'

# Only this product's files: no psg-pdf.exe, no CLI, no OCR models.
$names = @(Read-Rows 'SELECT `FileName` FROM `File`' 1 | ForEach-Object { ($_[0] -split '\|')[-1] })
if ($names -notcontains 'psg-academic-pdf.exe') { throw 'psg-academic-pdf.exe is missing' }
foreach ($foreign in 'psg-pdf.exe', 'pdfcraft-cli.exe', 'text-detection.onnx') {
  if ($names -contains $foreign) { throw "$foreign belongs to PSG PDF, not to this MSI" }
}
# Templates in templates\academic beside the exe, where the app looks.
$dirs = @{}
foreach ($row in (Read-Rows 'SELECT `Directory`, `Directory_Parent`, `DefaultDir` FROM `Directory`' 3)) { $dirs[$row[0]] = $row }
Assert-Equal $dirs['TemplatesRoot'][1] 'INSTALLFOLDER' 'templates folder parent'
Assert-Equal (($dirs['TemplatesFolder'][2] -split '\|')[-1]) 'academic' 'templates\academic folder'
if ($names -notcontains 'template.toml' -or $names -notcontains 'psg-article.cls') { throw 'the journal templates are missing' }

# Plain shortcuts to the exe, the desktop one switchable.
foreach ($entry in @(@('StartMenuShortcut', 'ProgramMenuFolder', 'AcademicStartMenuShortcut', ''),
                     @('DesktopShortcut', 'DesktopFolder', 'AcademicDesktopShortcut', 'INSTALLDESKTOPSHORTCUT = 1'))) {
  $row = Read-Row ('SELECT `Directory_`, `Target`, `Icon_`, `Component_` FROM `Shortcut` WHERE `Shortcut` = ''' + $entry[0] + '''') 4
  Assert-Equal $row[0] $entry[1] "$($entry[0]) directory"
  Assert-Equal $row[1] '[#AcademicExe]' "$($entry[0]) target"
  Assert-Equal $row[2] '' "$($entry[0]) icon"
  Assert-Equal $row[3] $entry[2] "$($entry[0]) component"
  $component = Read-Row ('SELECT `Condition` FROM `Component` WHERE `Component` = ''' + $entry[2] + '''') 1
  Assert-Equal $component[0] $entry[3] "$($entry[2]) condition"
}
if (@(Read-Rows 'SELECT `Control` FROM `Control` WHERE `Dialog_` = ''InstallWelcome'' AND `Control` = ''PdfContextMenu''' 1).Count) {
  throw 'The PDF context-menu checkbox is PSG PDF''s; this MSI adds no menu'
}

# Manuscripts are offered under Open with, never made the default.
foreach ($ext in '.tex', '.docx') {
  $row = Read-Row ('SELECT `Value` FROM `Registry` WHERE `Key` = ''Software\Classes\' + $ext + '\OpenWithProgids'' AND `Name` = ''PSGAcademicPDF.Manuscript''') 1
  Assert-Equal $row[0] '' "$ext Open with"
  if (@(Read-Rows ('SELECT `Registry` FROM `Registry` WHERE `Key` = ''Software\Classes\' + $ext + ''' AND `Name` IS NULL') 1).Count) { throw "$ext default app must not be set" }
}

# The old suite is detected, never removed: OnlyDetect (2) + VersionMinInclusive (256), up to the split.
$upgrade = Read-Row 'SELECT `UpgradeCode`, `VersionMin`, `VersionMax`, `Attributes` FROM `Upgrade` WHERE `ActionProperty` = ''PSGSUITEFOUND''' 4
Assert-Equal $upgrade[0] '{C984AB08-3E67-48B7-8DC2-6F1428165299}' 'suite UpgradeCode'
Assert-Equal $upgrade[2] '0.19.0' 'suite detection maximum'
if ((([int] $upgrade[3]) -band 2) -eq 0) { throw "the suite must only be detected (attributes $($upgrade[3]))" }
$launch = @(Read-Rows 'SELECT `Condition` FROM `LaunchCondition`' 1 | Where-Object { $_[0] -match 'PSGSUITEFOUND' })
if ($launch.Count -ne 1) { throw 'expected one launch condition that stops while the suite is installed' }
$session = $Installer.OpenPackage((Resolve-Path -LiteralPath $Path).Path, 1)
try {
  foreach ($case in @(@('', '', '', '', 1), @('', '{OLD}', '', '', 0), @('', '', '1', '', 0), @('', '', '', '1', 0), @('1', '{OLD}', '', '', 1))) {
    $session.Property('Installed') = $case[0]
    $session.Property('PSGSUITEFOUND') = $case[1]
    $session.Property('PSGSUITEACADEMICU') = $case[2]
    $session.Property('PSGSUITEACADEMICM') = $case[3]
    Assert-Equal ([int] $session.EvaluateCondition($launch[0][0])) $case[4] "suite guard ($($case -join ','))"
  }
} finally { [void] [Runtime.InteropServices.Marshal]::FinalReleaseComObject($session) }

[void] [Runtime.InteropServices.Marshal]::FinalReleaseComObject($Database)
[void] [Runtime.InteropServices.Marshal]::FinalReleaseComObject($Installer)
Write-Output 'ok PSG Academic PDF MSI: own identity, dual scope, shortcuts, manuscript Open with, templates, suite detected and never removed'
