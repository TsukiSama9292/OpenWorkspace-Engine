# VNC Authentication

## Overview

KasmVNC containers run HTTP Basic Auth on the websockify endpoint. Traefik injects the `Authorization: Basic` header server-side before proxying, so the browser never sees VNC credentials. This eliminates the second login prompt while keeping container-to-container traffic blocked.

## Architecture

```
Browser                    Traefik                     KasmVNC
  |                           |                           |
  |-- wss /vnc/{token}/ws -->|                           |
  |                           |-- inject Basic header --->|
  |                           |   kasm_user:{password}    |
  |                           |                           |
  |                           |<-- 200 OK (auth passed) --|
  |<-- WebSocket upgrade -----|                           |
  |                           |                           |
  |<===== encrypted VNC tunnel ==========================>|
```

**Key insight:** Standard JS `WebSocket` API doesn't support custom headers. Traefik solves this by injecting the header server-side — the browser only sends the standard WebSocket upgrade request.

## Password Generation

### Storage

Each instance has a `vnc_password` column in PostgreSQL (`workspace_instances.vnc_password VARCHAR(128)`). This is the source of truth.

### Generation

```rust
// apps/api/src/db.rs::generate_access_password
pub fn generate_access_password() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let len = 127;
    let pool: Vec<u8> = (b'a'..=b'z')
        .chain(b'A'..=b'Z')
        .chain(b'0'..=b'9')
        .collect();
    // 62 alphanumeric characters (a-z, A-Z, 0-9)
    // 127 chars / 62^127 ≈ 756 bits of entropy
    (0..len)
        .map(|_| {
            let idx = rng.gen_range(0..pool.len());
            pool[idx] as char
        })
        .collect()
}
```

- Fixed length: **127 characters** (KasmVNC struct limit: 128)
- Charset: 62 alphanumeric chars (`a-z`, `A-Z`, `0-9`)
- Entropy: `127 × log₂(62)` ≈ **756 bits** (equivalent to a strong random key)

### Injection into KasmVNC

When creating a container, the API sets `VNC_PW=<password>` as an environment variable. KasmVNC reads this and uses it as the websockify password. No `.kasmpasswd` file manipulation needed.

## Traefik Header Injection

### Per-Token Middleware

Each VNC instance gets its own Traefik middleware in `vnc-{token}-ws.yml`:

```yaml
http:
  middlewares:
    vnc-{token}-auth:
      headers:
        customRequestHeaders:
          Authorization: "Basic a2FzbV91c2VyOnBhc3N3b3Jk"
```

The `Authorization` value is `base64(kasm_user:{password})`.

### Why `kasm_user`?

KasmVNC hardcodes the username as `kasm_user` in its startup code. The `.kasmpasswd` file contains entries:
- `kasm_user:$5$kasm$...:wo` (full access)
- `kasm_viewer:$5$kasm$...:r` (read-only)

Empty username `:password` returns 401. The correct format is `kasm_user:password`.

### Why Not ForwardAuth?

The original ForwardAuth approach called `/api/vnc/verify` before proxying, but:
1. **WebSocket headers can't be set by the browser** — JS `WebSocket` doesn't support custom headers
2. **ForwardAuth can only validate, not inject** — it can't add `Authorization` to the proxied request
3. **Per-token middleware** solves both: validates via ForwardAuth (JWT check) AND injects Basic auth header

### Container Isolation

Each container has a unique password. Even if an attacker knows one container's IP, they can't connect without the correct password — Traefik only injects headers for routed requests, not raw container-to-container traffic.

## Frontend Flow

### Launching a VNC Session

1. User clicks "Launch" on dashboard
2. `POST /api/instances` creates instance + generates `vnc_password`
3. Redirect to `/vnc/{token}/`

### Connecting to VNC

1. VNC page mounts, immediately renders `<VncSession>` (no gating)
2. `VncViewer` mounts, connects with placeholder `'password'`
3. `onMount` fetches `GET /api/instances`, matches by `vnc_token`
4. API response includes `vnc_password` field
5. `$effect` detects password change → triggers `connect()` with real password
6. Traefik injects correct `Authorization: Basic` header
7. KasmVNC authenticates → WebSocket established

### URL Structure

```
/vnc/{token}/              → VNC viewer HTML (SvelteKit)
/vnc/{token}/websockify    → WebSocket proxy (Traefik → KasmVNC)
```

No `?pw=` query parameter — password never visible in URL bar.

## Security Model

| Layer | Mechanism | What it protects |
|-------|-----------|------------------|
| Traefik ForwardAuth | JWT cookie validation | Only authenticated users can reach VNC routes |
| Traefik Header Injection | Per-token Basic auth | Only Traefik-injected traffic reaches KasmVNC |
| KasmVNC Basic Auth | HTTP Basic on websockify | Container rejects unauthenticated connections |
| Docker Network | Instances on default `bridge` (not `ow-network`) | Containers can't reach each other directly; only published ports are exposed |

### Attack Scenarios

**Attacker knows container IP (e.g., 172.16.0.4):**
- Can't connect — no Traefik header injection = 401 from KasmVNC
- Container-to-container traffic blocked

**Attacker steals JWT cookie:**
- Can access VNC routes via Traefik
- But Traefik injects correct Basic auth header
- Attacker gets same access as legitimate user (intended behavior)

**Attacker reads `vnc_password` from DB:**
- Can generate correct Basic auth header
- But still needs Traefik routing (can't bypass to container directly)
- DB access = full compromise anyway

## Configuration

### Traefik Static Config

`docker/openworkspace_dev/traefik/traefik.yml`:
```yaml
providers:
  file:
    directory: "/etc/traefik/dynamic"
    watch: true
```

### KasmVNC Config

`kasmvnc.yaml` (injected at container startup):
```yaml
network:
  ssl:
    pem_certificate: /etc/kasmvnc/self.pem
    pem_key: /etc/kasmvnc/self.pem
  ssl_only: false  # Traefik handles external TLS
require_ssl: false
```

### Environment Variables

| Variable | Value | Purpose |
|----------|-------|---------|
| `VNC_PW` | `<127-char random>` | Enables HTTP Basic Auth |
| `KASM_VNC_PORT` | `6901` | Websockify listen port |
| `DISPLAY` | `:1` | X11 display |

## Testing

### Verify Header Injection

```bash
# Get container password from DB
docker exec -i ow-postgres psql -U postgres -d openworkspace \
  -c "SELECT vnc_password FROM workspace_instances LIMIT 1;"

# Test Basic auth directly (should return 404 = auth passed, curl can't do WS)
curl -u "kasm_user:{password}" https://172.16.0.4:6901/websockify -k
# Expected: 404 (auth succeeded, but curl can't complete WebSocket handshake)

# Test without auth (should return 401)
curl https://172.16.0.4:6901/websockify -k
# Expected: 401 Unauthorized
```

### Cleanup Script

`scripts/cleanup.sh traefik` (also part of the default full cleanup) removes all per-instance route files from the dynamic dir, preserving static config files.

## Migration History

| Migration | Change |
|-----------|--------|
| `m20260723_000006_add_vnc_password.rs` | Add `vnc_password VARCHAR(64)` column |
| `m20260723_000007_expand_vnc_password.rs` | Expand to `VARCHAR(128)` for 127-char passwords |
