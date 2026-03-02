# Desktop client (Tauri v2 wrapper)

This package is a desktop wrapper around the shared React UI in `../web`.

## Design

- Single UI source: all routes and pages live in `web/`.
- Tauri dev mode loads `http://127.0.0.1:5173` (started automatically).
- Tauri build mode compiles `web/dist` and embeds those static assets.
- Desktop wrapper includes daemon supervisor commands (bootstrap/start/stop/restart).
- Current lifecycle policy: closing desktop app stops daemon process managed by the wrapper.

## Commands

- `pnpm -C client dev`
  - Runs `tauri dev`.
  - Starts web Vite dev server via Tauri `beforeDevCommand`.
- `pnpm -C client build`
  - Runs `tauri build --no-bundle`.
  - Produces a local desktop build without installer packaging.

## Runtime notes

- When running under browser `http/https`, routing uses `BrowserRouter`.
- When running under Tauri custom protocol, routing falls back to `HashRouter` to keep deep links stable.
