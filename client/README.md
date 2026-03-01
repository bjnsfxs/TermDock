# Desktop client (Tauri v2) placeholder

This directory is intentionally a placeholder in the skeleton.

Recommended approach for Codex:
1. Create a Tauri v2 app here (React + Vite).
2. Reuse the UI from `../web/` (either by copying or by extracting a shared package under `packages/ui/`).
3. Add a settings page to set daemon address/token and a QR code generator.

Suggested commands (run manually / via Codex):
- `pnpm create tauri-app@latest client -- --template react-ts`
- Then move/merge files as needed.

Why placeholder?
- Tauri v2 templates change over time; generating from the official CLI avoids stale config.
