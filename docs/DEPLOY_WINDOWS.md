# Windows Deployment Guide (Portable Zip)

This guide targets the M6 portable release (`ai-cli-manager-win-x64.zip`).

## 1. Install

1. Unzip to a stable folder, for example `C:\Apps\AI-CLI-Manager`.
2. Open PowerShell in `<install-root>\scripts`.
3. Start the daemon:
   - `.\start-daemon.ps1`

When the daemon is running:

- Web UI: `http://127.0.0.1:8765/`
- Health: `http://127.0.0.1:8765/health`

The daemon serves web assets from `<install-root>\web` via `AICLI_WEB_DIR`.

## 2. Read the token

From `<install-root>\scripts`:

- `.\show-token.ps1`

Use this token in the Settings page (`Authorization: Bearer <token>` for REST and `?token=` for browser WebSocket fallback).

## 3. Enable autostart (Current User)

From `<install-root>\scripts`:

- Install: `.\install-autostart.ps1`
- Remove: `.\remove-autostart.ps1`

Task details:

- Task name: `AI CLI Manager Daemon`
- Trigger: at logon (current user)
- Action: run `start-daemon.ps1 -TaskMode` with hidden PowerShell window

## 4. LAN access for mobile

1. Open the web Settings page.
2. Change `bind_address` to `0.0.0.0` or host LAN IP and apply.
3. Restart daemon.
4. Use host LAN URL + token on mobile browser.

Security notes:

- Keep token private.
- Restrict host firewall inbound scope.
- Avoid public or untrusted networks.

## 5. Troubleshooting

- Port in use:
  - Change daemon `port` in Settings and restart.
- UI opens but API returns 401:
  - Verify token in Settings matches `show-token.ps1`.
- Mobile cannot connect:
  - Check daemon bind address is not loopback.
  - Check Windows firewall allows inbound to daemon port on private networks.
- Autostart does not run:
  - Open Task Scheduler and inspect `AI CLI Manager Daemon` history.
  - Reinstall task with `.\remove-autostart.ps1` then `.\install-autostart.ps1`.

## 6. Desktop wrapper (M10, daemon-managed)

The Tauri desktop app is now available under `client/` and reuses the same web UI.

- Dev run: `pnpm dev:desktop`
- Local build: `pnpm build:desktop`

Important:

- Desktop wrapper can bootstrap daemon on startup when daemon binary is available.
- Settings page includes daemon lifecycle controls (bootstrap/start/stop/restart).
- Desktop close currently stops the daemon process started/managed by the wrapper.
- In desktop protocol context, default daemon base URL falls back to `http://127.0.0.1:8765`.

## 7. Download artifacts from GitHub Actions (M9)

Main branch pushes and manual workflow runs can publish two artifacts:

- `ai-cli-manager-win-x64`: portable daemon+web zip package.
- `ai-cli-manager-client-win-x64`: desktop wrapper local build output.

Download flow:

1. Open the repository `Actions` tab.
2. Select workflow `Build Artifacts`.
3. Open a successful run on branch `main` (or a manual `workflow_dispatch` run).
4. Download artifact `ai-cli-manager-win-x64`, then extract and follow sections 1-5.
5. Optionally download `ai-cli-manager-client-win-x64` for desktop wrapper distribution/testing.
