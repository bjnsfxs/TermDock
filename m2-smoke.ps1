# m2-smoke.ps1
$ErrorActionPreference = "Stop"

# 改成你的 daemon 地址/端口（默认 8765）
$hostBase = "http://127.0.0.1:8765"
$base = "$hostBase/api/v1"

# 如果你启用了鉴权，把 token 填上；没启用就留空
$headers = @{}
# $headers = @{ Authorization = "Bearer YOUR_TOKEN" }

function Get-Health {
  try { return Invoke-RestMethod -Uri "$hostBase/health" -Headers $headers }
  catch { return Invoke-RestMethod -Uri "$base/health" -Headers $headers }
}

Write-Host "== Health =="
$h = Get-Health
$h | ConvertTo-Json -Depth 5

Write-Host "== Create pipes instance =="
$body = @{
  name = "m2-pipes-demo"
  enabled = $true
  command = "powershell"
  args_json = '["-NoProfile","-Command","Write-Output ''hi''; Start-Sleep 1; Write-Output ''bye''"]'
  cwd = $null
  env_json = "{}"
  use_pty = $false
  config_mode = "none"
  restart_policy = "never"
  auto_start = $false
} | ConvertTo-Json -Depth 10

$inst = Invoke-RestMethod -Method Post -Uri "$base/instances" -Headers $headers -ContentType "application/json" -Body $body
$id = $inst.id
Write-Host "Created: $id"

Write-Host "== Start pipes instance =="
Invoke-RestMethod -Method Post -Uri "$base/instances/$id/start" -Headers $headers | Out-Null
Start-Sleep 2

Write-Host "== Get instance status =="
Invoke-RestMethod -Method Get -Uri "$base/instances/$id" -Headers $headers | ConvertTo-Json -Depth 10

Write-Host "== Output tail (decode base64 if present) =="
$out = Invoke-RestMethod -Method Get -Uri "$base/instances/$id/output?tail=4096" -Headers $headers
$out | ConvertTo-Json -Depth 10

if ($out.data) {
  $bytes = [Convert]::FromBase64String($out.data)
  $text = [Text.Encoding]::UTF8.GetString($bytes)
  Write-Host "---- decoded tail ----"
  Write-Host $text
} else {
  Write-Host "NOTE: output response has no .data field; adjust script for your response shape."
}

Write-Host "== Create PTY instance (should 501) =="
$bodyPty = @{
  name = "m2-pty-should-fail"
  enabled = $true
  command = "powershell"
  args_json = '["-NoProfile","-Command","Write-Output ''hello''"]'
  cwd = $null
  env_json = "{}"
  use_pty = $true
  config_mode = "none"
  restart_policy = "never"
  auto_start = $false
} | ConvertTo-Json -Depth 10

$instPty = Invoke-RestMethod -Method Post -Uri "$base/instances" -Headers $headers -ContentType "application/json" -Body $bodyPty
$idPty = $instPty.id
Write-Host "Created PTY instance: $idPty"

try {
  Invoke-RestMethod -Method Post -Uri "$base/instances/$idPty/start" -Headers $headers | Out-Null
  Write-Host "ERROR: expected 501 but start succeeded"
} catch {
  Write-Host "Got expected error:"
  Write-Host $_.Exception.Message
  if ($_.ErrorDetails -and $_.ErrorDetails.Message) {
    Write-Host "---- server body ----"
    Write-Host $_.ErrorDetails.Message
  }
}

Write-Host "== Cleanup =="
Invoke-RestMethod -Method Delete -Uri "$base/instances/$id" -Headers $headers | Out-Null
Invoke-RestMethod -Method Delete -Uri "$base/instances/$idPty" -Headers $headers | Out-Null
Write-Host "Done."