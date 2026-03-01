# AI CLI Manager

Windows host daemon for managing multiple AI CLI instances, plus a React PWA frontend.

## Repository layout

- `daemon/`: Rust daemon (`axum` REST + WebSocket + process runtime manager)
- `web/`: React + Vite PWA frontend
- `client/`: Tauri v2 desktop wrapper (reuses `web/` as single UI source)
- `packages/api-client/`: API client placeholder
- `docs/`: architecture, API, security, deployment docs

## Development quick start

1. Start daemon:
   - `cargo run --manifest-path daemon/Cargo.toml`
2. Start web dev server (optional, if not using daemon-hosted static):
   - `pnpm -C web dev`

Desktop wrapper dev (requires daemon running separately):

3. Start desktop app:
   - `pnpm dev:desktop`

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

## Desktop local build (Windows-first)

```powershell
pnpm build:desktop
```

Notes:

- The desktop client is connect-only in M8 (it does not start/stop daemon automatically).
- Default daemon URL in desktop protocol context falls back to `http://127.0.0.1:8765`.

## Windows deployment and troubleshooting

See:

- `docs/DEPLOY_WINDOWS.md`
- `docs/SECURITY.md`

## API specification

- `openapi.yaml` at repo root
- `docs/API.md` for WebSocket message protocol details
