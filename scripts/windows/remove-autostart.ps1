param(
  [string]$TaskName = "AI CLI Manager Daemon"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$task = Get-ScheduledTask -TaskName $TaskName -ErrorAction SilentlyContinue
if (-not $task) {
  Write-Host "Autostart task not found: $TaskName"
  exit 0
}

Unregister-ScheduledTask -TaskName $TaskName -Confirm:$false
Write-Host "Autostart task removed: $TaskName"
