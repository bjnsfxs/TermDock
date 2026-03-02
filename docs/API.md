# API (REST + WebSocket)

## REST

REST endpoints are described in **OpenAPI**:

- `../openapi.yaml`

Base URL example:

- `http://127.0.0.1:8765`
- REST prefix: `/api/v1`

Bundled web UI hosting:

- `GET /` serves the frontend entry page when static assets are present.
- Non-API frontend routes fall back to `index.html` (SPA history routing).
- API and WebSocket namespaces remain under `/api/v1/*` and `/ws/v1/*`.

Auth:

- `Authorization: Bearer <token>`
- Accepted token kinds:
  - `master` token (from daemon config / settings)
  - `device` token (issued through pair approval flow)
- Privileged endpoints (`settings`, `system shutdown`, pairing/admin endpoints) require `master` token.
- Runtime control endpoints (`instances`, `output`, WS terminal/events) accept both `master` and active `device` tokens.

`/health` is public.

Settings and token management:

- `GET /api/v1/settings` returns current daemon `bind_address`, `port`, `data_dir`, and `token`.
- `PUT /api/v1/settings` updates daemon settings persisted on disk.
- `POST /api/v1/auth/token/rotate` rotates bearer token immediately (old token becomes invalid).
- Updating `bind_address` is only allowed when the request comes from a loopback client (`127.0.0.1` / `::1`).
- `PUT /api/v1/settings` validates `bind_address` + `port` as a parseable socket address and returns `400` when invalid.
- Changing `bind_address`/`port` requires daemon restart to take effect.

Pairing and device trust management:

- `POST /api/v1/auth/pair/start` (master + loopback): create one-time pair session and return pair URI.
- `POST /api/v1/auth/pair/complete` (public): mobile/client submits `pair_id + pair_secret + device_name`.
- `GET /api/v1/auth/pair/status/{pair_id}?secret=...` (public): poll pair result; approved flow returns device token once.
- `GET /api/v1/auth/pair/pending` (master): list pending approval requests.
- `POST /api/v1/auth/pair/decision` (master): approve/reject pending request.
- `GET /api/v1/auth/devices` (master): list trusted devices.
- `DELETE /api/v1/auth/devices/{device_id}` (master): revoke trusted device token.

System lifecycle:

- `POST /api/v1/system/shutdown` (master + loopback): graceful daemon shutdown trigger.

## WebSocket

OpenAPI cannot fully model WebSocket frame flows. This document defines the message protocol used by:

- `GET /ws/v1/events`
- `GET /ws/v1/term/{id}`

### Common rules

Handshake:

- Upgrade to WebSocket with HTTP `GET`.
- **Browser clients cannot set custom headers on WebSocket**, so the daemon MUST support at least one of:
  1) `?token=<token>` query parameter (**recommended for MVP**), or
  2) token via `Sec-WebSocket-Protocol` subprotocol trick.

This implementation supports `?token=<token>` and validates against master/device tokens.

Frame types:

- **Text frames**: UTF-8 JSON objects.
- **Binary frames**: raw bytes (ArrayBuffer in browsers).

Client identity:

- Client SHOULD generate a random `client_id` (UUID) and include it in the first text frame after connect:
  ```json
  {"type":"hello","client_id":"...","client_name":"web|desktop|mobile"}
  ```

Server may reply:
```json
{"type":"hello","daemon_version":"0.1.0"}
```

### 1) Global events: `/ws/v1/events`

Server -> Client: **JSON text frames** only.

Current daemon behavior:
- Emits `instance_status` as the primary realtime event (lifecycle + client attach count + periodic metrics updates).
- `metrics` event shape remains reserved/optional for future compatibility.

Event: instance status changes
```json
{
  "type": "instance_status",
  "id": "8d8a4d3c-3d50-4d87-9a7f-4df9dbf9c9f8",
  "runtime": {
    "status": "running",
    "pid": 12345,
    "started_at": "2026-03-01T12:34:56Z",
    "exit_code": null,
    "cpu_percent": 2.5,
    "mem_bytes": 104857600,
    "clients_attached": 1
  }
}
```

Event: metrics update (optional if you already send full runtime above)
```json
{
  "type": "metrics",
  "id": "8d8a4d3c-3d50-4d87-9a7f-4df9dbf9c9f8",
  "cpu_percent": 4.2,
  "mem_bytes": 209715200
}
```

Event: daemon notice
```json
{"type":"notice","level":"info","message":"Daemon started","ts":"2026-03-01T12:34:56Z"}
```

Keepalive:
- Client may send `{ "type": "ping" }`
- Server replies `{ "type": "pong" }`

### 2) Terminal attach: `/ws/v1/term/{id}`

This WS carries **terminal output bytes** + **control JSON**.

#### Server -> Client

Binary frames:
- terminal output bytes (ANSI escape sequences preserved).
- tail payload bytes after a `tail_begin` frame.

Text frames (JSON):
- Hello:
  ```json
  {"type":"hello","daemon_version":"0.1.0"}
  ```
- Status update:
  ```json
  {"type":"status","id":"...","status":"running","pid":12345,"backend":"pty","clients_attached":1}
  ```
- Error:
  ```json
  {"type":"error","code":"not_running","message":"instance is not running"}
  ```
- Warning:
  ```json
  {"type":"warning","code":"output_lagged","message":"output lagged; dropped 3 frame(s)"}
  ```
- Tail response header (server sends binary tail bytes immediately after this frame when available):
  ```json
  {"type":"tail_begin","requested":8192,"bytes":4096,"truncated":true}
  ```

#### Client -> Server

Binary frames:
- stdin bytes (from xterm `onData`), encoded as UTF-8.

Text frames (JSON):
- Hello:
  ```json
  {"type":"hello","client_id":"...","client_name":"web"}
  ```
- Ping:
  ```json
  {"type":"ping"}
  ```
- Resize:
  ```json
  {"type":"resize","cols":120,"rows":30}
  ```
- Tail request (on initial attach / reconnect):
  ```json
  {"type":"tail","bytes":8192}
  ```

#### Reconnect behavior (recommended)

1. Client connects WS.
2. Client sends `hello` (optional).
3. Client sends `tail` request for last N bytes.
4. Server sends the last N bytes (binary) then continues streaming live output.
