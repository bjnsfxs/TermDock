param(
  [string]$DataDir = ""
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$candidateDirs = @()
if ($DataDir -and $DataDir.Trim().Length -gt 0) {
  $candidateDirs += $DataDir.Trim()
}
if ($env:AICLI_DATA_DIR -and $env:AICLI_DATA_DIR.Trim().Length -gt 0) {
  $candidateDirs += $env:AICLI_DATA_DIR.Trim()
}

$appData = [Environment]::GetFolderPath("ApplicationData")
$candidateDirs += (Join-Path $appData "aicli\ai-cli-manager\data")
$candidateDirs += (Join-Path $appData "com\aicli\ai-cli-manager\data")
$candidateDirs += (Join-Path $appData "ai-cli-manager\data")

$configPath = $null
foreach ($dir in ($candidateDirs | Select-Object -Unique)) {
  $candidate = Join-Path $dir "daemon.json"
  if (Test-Path $candidate) {
    $configPath = $candidate
    break
  }
}

if (-not $configPath) {
  throw "Cannot find daemon.json. Checked: $($candidateDirs -join ', ')"
}

$cfg = Get-Content $configPath -Raw | ConvertFrom-Json

Write-Host "Config file : $configPath"
Write-Host "bind_address: $($cfg.bind_address)"
Write-Host "port        : $($cfg.port)"
Write-Host "token       : $($cfg.token)"
