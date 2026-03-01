param(
  [switch]$TaskMode,
  [string]$DataDir = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$candidateRoots = @(
  (Resolve-Path (Join-Path $PSScriptRoot "..")).Path
)
$twoUp = Join-Path $PSScriptRoot "..\.."
if (Test-Path $twoUp) {
  $candidateRoots += (Resolve-Path $twoUp).Path
}
$candidateRoots = $candidateRoots | Select-Object -Unique

$daemonExe = $null
$webDir = $null

foreach ($root in $candidateRoots) {
  $packagedExe = Join-Path $root "bin\ai-cli-manager-daemon.exe"
  $packagedWeb = Join-Path $root "web"
  if ((Test-Path $packagedExe) -and (Test-Path $packagedWeb)) {
    $daemonExe = $packagedExe
    $webDir = $packagedWeb
    break
  }

  $devExe = Join-Path $root "daemon\target\release\ai-cli-manager-daemon.exe"
  $devWeb = Join-Path $root "web\dist"
  if ((Test-Path $devExe) -and (Test-Path $devWeb)) {
    $daemonExe = $devExe
    $webDir = $devWeb
    break
  }
}

if (-not $daemonExe) {
  throw "Daemon executable not found in expected locations. Checked roots: $($candidateRoots -join ', ')"
}

$env:AICLI_WEB_DIR = $webDir
if ($DataDir -and $DataDir.Trim().Length -gt 0) {
  $env:AICLI_DATA_DIR = $DataDir.Trim()
}

if (-not $TaskMode) {
  Write-Host "Starting daemon from: $daemonExe"
  Write-Host "AICLI_WEB_DIR=$webDir"
  if ($env:AICLI_DATA_DIR) {
    Write-Host "AICLI_DATA_DIR=$($env:AICLI_DATA_DIR)"
  }
}

& $daemonExe
exit $LASTEXITCODE
