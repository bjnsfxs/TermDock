param(
  [string]$ArtifactName = "ai-cli-manager-win-x64"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$repoRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
$artifactRoot = Join-Path $repoRoot "artifacts"
$stageDir = Join-Path $artifactRoot $ArtifactName
$zipPath = Join-Path $artifactRoot "$ArtifactName.zip"

Write-Host "Repo root: $repoRoot"
Write-Host "Artifact name: $ArtifactName"

if (Test-Path $stageDir) {
  Remove-Item -Recurse -Force $stageDir
}
if (Test-Path $zipPath) {
  Remove-Item -Force $zipPath
}

New-Item -ItemType Directory -Path $artifactRoot -Force | Out-Null
New-Item -ItemType Directory -Path $stageDir -Force | Out-Null

$binDir = Join-Path $stageDir "bin"
$webDir = Join-Path $stageDir "web"
$scriptsDir = Join-Path $stageDir "scripts"
New-Item -ItemType Directory -Path $binDir, $webDir, $scriptsDir -Force | Out-Null

Write-Host "Building web..."
pnpm -C (Join-Path $repoRoot "web") build

Write-Host "Building daemon release..."
cargo build --release --manifest-path (Join-Path $repoRoot "daemon\Cargo.toml")

$daemonExe = Join-Path $repoRoot "daemon\target\release\ai-cli-manager-daemon.exe"
if (-not (Test-Path $daemonExe)) {
  throw "Daemon binary not found at: $daemonExe"
}

$webDistDir = Join-Path $repoRoot "web\dist"
if (-not (Test-Path $webDistDir)) {
  throw "Web build output not found at: $webDistDir"
}

$windowsScriptsDir = Join-Path $repoRoot "scripts\windows"
if (-not (Test-Path $windowsScriptsDir)) {
  throw "Windows scripts not found at: $windowsScriptsDir"
}

Copy-Item $daemonExe (Join-Path $binDir "ai-cli-manager-daemon.exe") -Force
Copy-Item (Join-Path $webDistDir "*") $webDir -Recurse -Force
Copy-Item (Join-Path $windowsScriptsDir "*.ps1") $scriptsDir -Force
Copy-Item (Join-Path $repoRoot "docs\DEPLOY_WINDOWS.md") (Join-Path $stageDir "README-WINDOWS.md") -Force

Write-Host "Compressing artifact..."
Compress-Archive -Path (Join-Path $stageDir "*") -DestinationPath $zipPath -Force

Write-Host "Portable artifact created:"
Write-Host "  Stage: $stageDir"
Write-Host "  Zip  : $zipPath"
