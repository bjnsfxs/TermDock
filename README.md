# AI CLI Manager

Windows host daemon for managing multiple AI CLI instances, plus a React PWA frontend.

## Repository layout

- `daemon/`: Rust daemon (`axum` REST + WebSocket + process runtime manager)
- `web/`: React + Vite PWA frontend
- `client/`: desktop wrapper placeholder (not part of M6 deliverable)
- `packages/api-client/`: API client placeholder
- `docs/`: architecture, API, security, deployment docs

## Development quick start

1. Start daemon:
   - `cargo run --manifest-path daemon/Cargo.toml`
2. Start web dev server (optional, if not using daemon-hosted static):
   - `pnpm -C web dev`

Default daemon bind is `127.0.0.1:8765`.

Health check:

```bash
curl http://127.0.0.1:8765/health
```

## Daemon-hosted web UI

Daemon serves static frontend assets from `AICLI_WEB_DIR`.

- `AICLI_WEB_DIR` set: use that directory
- unset: defaults to `<daemon_exe_dir>/web`

When `web/dist` is packaged into that location:

- `GET /` serves UI entry page
- frontend routes (for example `/instances/:id/term`) fall back to `index.html`
- API and WS routes remain under `/api/v1/*` and `/ws/v1/*`

## Build portable release (Windows)

```powershell
pnpm release:portable
```

Output:

- Stage directory: `artifacts/ai-cli-manager-win-x64/`
- Zip archive: `artifacts/ai-cli-manager-win-x64.zip`

Portable package layout:

- `bin/ai-cli-manager-daemon.exe`
- `web/` (built static assets)
- `scripts/` (`start-daemon.ps1`, `install-autostart.ps1`, `remove-autostart.ps1`, `show-token.ps1`)
- `README-WINDOWS.md`

## Windows deployment and troubleshooting

See:

- `docs/DEPLOY_WINDOWS.md`
- `docs/SECURITY.md`

## API specification

- `openapi.yaml` at repo root
- `docs/API.md` for WebSocket message protocol details
