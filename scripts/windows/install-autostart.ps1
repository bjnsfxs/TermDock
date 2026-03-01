param(
  [string]$TaskName = "AI CLI Manager Daemon"
)

$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest

$startScript = (Resolve-Path (Join-Path $PSScriptRoot "start-daemon.ps1")).Path
$userId = "{0}\{1}" -f $env:USERDOMAIN, $env:USERNAME
$escapedScript = $startScript.Replace('"', '`"')
$arguments = "-NoProfile -ExecutionPolicy Bypass -WindowStyle Hidden -File `"$escapedScript`" -TaskMode"

$action = New-ScheduledTaskAction -Execute "powershell.exe" -Argument $arguments
$trigger = New-ScheduledTaskTrigger -AtLogOn -User $userId
$principal = New-ScheduledTaskPrincipal -UserId $userId -LogonType InteractiveToken -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet `
  -AllowStartIfOnBatteries `
  -DontStopIfGoingOnBatteries `
  -StartWhenAvailable `
  -MultipleInstances IgnoreNew

Register-ScheduledTask `
  -TaskName $TaskName `
  -Action $action `
  -Trigger $trigger `
  -Principal $principal `
  -Settings $settings `
  -Force | Out-Null

Write-Host "Autostart task registered."
Write-Host "Task name: $TaskName"
Write-Host "User: $userId"
