# VNC Authentication

## Overview

KasmVNC containers run HTTP Basic Auth on their websockify endpoint. Traefik injects the `Authorization: Basic` header server-side before proxying, so the browser never sees VNC credentials. This eliminates the second login prompt while keeping container-to-container traffic blocked.

The mechanism is per-instance: every instance has a unique `access_token` (used in the URL path) and `access_password` (used for Basic auth). These columns were renamed from `vnc_token` / `vnc_password` by migration `000008` when ttyd/Jupyter support landed.

## Architecture

```
Browser                    Traefik                        KasmVNC container
  |                           |                                |
  |-- wss /kasmvnc/{token}/websockify -->|                      |
  |                           |-- inject Basic header --------->|
  |                           |   kasm_user:{access_password}   |
  |                           |                                |
  |                           |<-- 101 (auth passed) ----------|
  |<-- WebSocket upgrade -----|                                |
  |                           |                                |
  |<============ VNC tunnel (via host-published port) ========>|
```

**Key insight:** the standard JS `WebSocket` API doesn't support custom headers. Traefik solves this by injecting the header server-side — the browser only sends the standard WebSocket upgrade request.

The KasmVNC container is reached via its **host-published port**: the route service points at `https://host.docker.internal:{host_port}/websockify` (transport `kasm-insecure`, which skips certificate verification because KasmVNC forces TLS).

## Credential Generation

### `access_token`

```rust
// apps/api/src/db.rs::generate_access_token
Uuid::new_v4().as_simple().to_string()   // 32 hex chars, no dashes
```

The token appears in the session URL (`/kasmvnc/{token}/websockify`) — a high-entropy unguessable path secret.

### `access_password`

```rust
// apps/api/src/db.rs::generate_access_password
pub fn generate_access_password() -> String {
    let mut rng = rand::thread_rng();
    let len = 127;
    let pool: Vec<u8> = (b'a'..=b'z').chain(b'A'..=b'Z').chain(b'0'..=b'9').collect();
    (0..len).map(|_| { let idx = rng.gen_range(0..pool.len()); pool[idx] as char }).collect()
}
```

- **127 characters** from a 62-char alphanumeric set → ≈ 756 bits of entropy
- Never exposed in the URL; it only travels as `base64(kasm_user:{password})` inside the Traefik-injected header
- Stored in `workspace_instances.access_password` (the API returns it to the frontend page, which hands it to `VncSession` → `VncViewer`)

### Injection into KasmVNC

When creating a KasmVNC container, the API sets these env vars:

| Variable | Value | Purpose |
|----------|-------|---------|
| `KASM_VNC_PORT` | `6901` | Websockify listen port |
| `DISPLAY` | `:1` | X11 display |
| `VNC_PW` | `<127-char access_password>` | Websockify Basic-auth password (user `kasm_user`) |

It also injects `/etc/kasmvnc/kasmvnc.yaml` (tar-stream upload):

```yaml
network:
  ssl:
    pem_certificate: ${HOME}/.vnc/self.pem
    pem_key: ${HOME}/.vnc/self.pem
    require_ssl: false
  udp:
    public_ip: 127.0.0.1
runtime_configuration:
  allow_override_standard_vnc_server_settings: true
  allow_override_list:
    - pointer.enabled
server:
  allow_environment_variables_to_override_config_settings: true
```

## Traefik Header Injection

### Per-Token Middleware

Each instance gets its own route file `kasmvnc-{token}-ws.yml` (written by `apps/api/src/route_writer.rs`):

```yaml
http:
  routers:
    kasmvnc-{token}-ws:
      rule: "PathPrefix(`/kasmvnc/{token}/websockify`)"
      service: "kasmvnc-{token}"
      entryPoints:
        - web
      middlewares:
        - "kasmvnc-{token}-auth"
        - "kasmvnc-{token}-strip"
  services:
    kasmvnc-{token}:
      loadBalancer:
        serversTransport: "kasm-insecure"
        servers:
          - url: "https://host.docker.internal:{host_port}"
  middlewares:
    kasmvnc-{token}-auth:
      headers:
        customRequestHeaders:
          Authorization: "Basic base64(kasm_user:{access_password})"
    kasmvnc-{token}-strip:
      stripPrefix:
        prefixes:
          - "/kasmvnc/{token}"
```

The `Authorization` value is `base64(kasm_user:{access_password})`.

### Why `kasm_user`?

KasmVNC hardcodes the username as `kasm_user` in its startup code (`.kasmpasswd` entries: `kasm_user:...:wo` full access, `kasm_viewer:...:r` read-only). An empty username returns 401 — the correct format is `kasm_user:password`.

### Why not ForwardAuth?

The original design used Traefik **ForwardAuth** pointing at `GET /api/vnc/verify`:

1. **WebSocket headers can't be set by the browser** — JS `WebSocket` doesn't support custom headers.
2. **ForwardAuth can validate, but can't inject** — it can't add `Authorization` to the proxied request.

`/api/vnc/verify` still exists (it decodes the `ow_token` cookie, looks up the instance by the `/kasmvnc/{token}/websockify` path via the `VncCache`, and returns `X-Forwarded-User` / `X-Forwarded-Role`), and the `vnc-auth` ForwardAuth middleware is still declared in `static-routers.yml`. However, **it is currently not attached to any router** — the active per-route gate is the per-token Basic header injection above.

### Container Isolation

Each container has a unique password. Even if an attacker learns one container's IP, they can't connect without the correct password — Traefik only injects headers for routed requests, not raw container-to-container traffic.

## Frontend Flow

### Launching a Session

1. User clicks **Launch** on the dashboard → `POST /api/instances` creates the instance and generates `access_token` + `access_password`.
2. The dashboard redirects to `/instances/{id}`, which (once running) redirects to `/kasmvnc/{token}/` (VNC) or `/open/{token}/` (ttyd/Jupyter).

### Connecting to VNC

1. `/kasmvnc/{token}/` mounts and finds the instance by `access_token` via `GET /api/instances`.
2. It fetches `access_password` and passes it to `VncSession` → `VncViewer`.
3. `VncViewer` connects over `wss://<host>/kasmvnc/{token}/websockify`.
4. Traefik injects the correct `Authorization: Basic` header for that token.
5. KasmVNC authenticates → WebSocket established.

### URL Structure

```
/kasmvnc/{token}/              → VNC viewer HTML (SvelteKit)
/kasmvnc/{token}/websockify    → WebSocket proxy (Traefik → KasmVNC container)
/open/{token}/                 → ttyd / Jupyter iframe wrapper
/ttyd/{token}/                 → ttyd proxy route
/jupyter/{token}/              → Jupyter proxy route
```

No `?pw=` query parameter — the password never appears in the URL bar.

## Security Model

| Layer | Mechanism | What it protects |
|-------|-----------|------------------|
| Traefik Header Injection | Per-token Basic auth | Only Traefik-injected traffic reaches KasmVNC |
| KasmVNC Basic Auth | HTTP Basic on websockify | Container rejects unauthenticated connections |
| Path secret | High-entropy `access_token` | Session URLs are unguessable |
| Docker isolation | Dedicated `/30` networks (`ow-<instance-id>`), port published on host gateway | Instances can't reach each other; only published ports are exposed |

### Attack Scenarios

**Attacker guesses a container IP/port:**
- Can't connect — no Traefik header injection = 401 from KasmVNC.

**Attacker knows a session URL token:**
- Can open the VNC page, but the page still needs a valid `ow_token` JWT (dashboard `+layout` guard redirects to `/login`), and the instance data (incl. `access_password`) is only returned to authorized users.

**Attacker steals a JWT cookie:**
- Can open VNC pages for instances they can manage (owner/admin/manager-over-user) — intended behavior.

**Attacker reads `access_password` from DB:**
- Can generate the Basic header, but still needs Traefik routing (can't bypass to the container directly); DB access is a full compromise anyway.

## Testing

### Verify Header Injection

```bash
# Get the instance password from DB
docker exec -i ow-dev-postgres psql -U postgres -d postgres \
  -c "SELECT access_token, access_password FROM workspace_instances LIMIT 1;"

# Test Basic auth directly against the container's published port
# (auth passed but curl can't complete the WS handshake → 404):
curl -u "kasm_user:{access_password}" https://localhost:{host_port}/websockify -k
# Expected: 404

# Test without auth (should be 401):
curl https://localhost:{host_port}/websockify -k
# Expected: 401 Unauthorized
```

### Unit Tests

- `apps/api/src/route_writer.rs` — `write_route` emits `kasmvnc-{token}-ws.yml` with the `websockify` PathPrefix, the strip middleware, and the auth middleware; `delete_route` removes it.
- `apps/api/tests/vnc_verify_test.rs` — the `/api/vnc/verify` handler (cache hit/miss, non-running → 404).

### Cleanup

`scripts/cleanup.sh traefik` (also part of `all`) removes per-instance route files from the dynamic dir, preserving the static config files.

## Migration History

| Migration | Change |
|-----------|--------|
| `m20260723_000006_add_vnc_password.rs` | Add `vnc_password VARCHAR(64)` column |
| `m20260723_000007_expand_vnc_password.rs` | Expand to `VARCHAR(128)` for 127-char passwords |
| `m20260802_000008_add_remote_type.rs` | Rename `vnc_token`→`access_token`, `vnc_password`→`access_password`; add `remote_type` (kasmvnc/ttyd/jupyter) |
