# Security

## MVP security baseline (implemented)

- Daemon default bind: `127.0.0.1:8765` (not LAN-exposed by default).
- Daemon can host bundled web UI from `AICLI_WEB_DIR` (default: `<daemon_exe_dir>/web`).
- All `/api/v1/*` require bearer auth (`master` token or approved `device` token).
- Privileged REST endpoints (`/api/v1/settings`, `/api/v1/system/*`, `/api/v1/auth/pair/start`, `/api/v1/auth/pair/pending`, `/api/v1/auth/pair/decision`, `/api/v1/auth/devices*`) require `master` token.
- All `/ws/v1/*` require bearer auth (`master` or approved `device` token):
  - native/non-browser clients: `Authorization: Bearer <token>`.
  - browser clients: `?token=<token>` query fallback.
- Token is generated on first run and persisted in daemon config (`daemon.json` in data dir).
- Token rotation endpoint (`POST /api/v1/auth/token/rotate`) invalidates old token immediately.
- `PUT /api/v1/settings` can only change `bind_address` from loopback clients.
- Pairing now uses one-time pair sessions (`pair_id + pair_secret`) and explicit desktop approval before issuing a device token.
- Device tokens can be revoked from Settings (`/api/v1/auth/devices/{id}`).

## LAN access guidance

When you set daemon bind to `0.0.0.0` or a LAN IP, anyone on the reachable network can attempt access.

Before enabling LAN:

- Rotate token, then share only through trusted channels.
- Use a strong private network (trusted home/office LAN, not public Wi-Fi).
- Restrict host firewall inbound rules to trusted subnets/devices.
- Keep token out of logs, screenshots, and shell history where possible.
- Prefer rotating token after sharing QR/pairing data across devices.

## Local deployment notes

- Portable package scripts (`start-daemon.ps1`, `install-autostart.ps1`) only set environment variables for the daemon process; they do not persist system-wide secrets.
- Browser-side token is stored in `localStorage` for MVP convenience. Treat host browser profile as sensitive.
- Scheduled task mode runs under the current user account (`InteractiveToken`, least privilege).

## Current limitations

- Token at rest is plain text in config file (OS ACL protected, not encrypted).
- CORS is permissive (`*`) for MVP browser connectivity.
- Pair session delivery token is returned once by status poll; callers should persist securely on client side.

## TLS roadmap (post-MVP)

1. Add optional HTTPS/WSS listener with configured cert/key paths.
2. Support self-signed cert bootstrap and certificate fingerprint pinning in clients.
3. Add client-side trust onboarding UX (verify fingerprint before storing token).
4. Add recommendation for reverse proxy + LAN TLS termination as alternative deployment path.
