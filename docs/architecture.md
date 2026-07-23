# System Architecture

## Overview

OpenWorkspace Engine is a multi-tenant virtual desktop platform. It provisions isolated KasmVNC Docker containers on demand and exposes them as browser-accessible VNC sessions through Traefik reverse proxy.

**Core design principle:** Traefik's file-based provider with directory watching enables hot-reloadable routing. The API dynamically generates per-instance Traefik route YAML files, making new VNC instances immediately accessible without proxy restart.

## High-Level Architecture

```mermaid
graph TB
    subgraph Browser
        UI[Dashboard UI<br/>SvelteKit SPA]
        VNC[VNC Viewer<br/>noVNC in browser]
    end

    subgraph "Host Machine"
        subgraph Docker Network: openworkspace-engin
            TRAEFIK[Traefik<br/>:80]
            KASM1["KasmVNC Instance #1<br/>:6901"]
            KASM2["KasmVNC Instance #2<br/>:6901"]
            KASMN["KasmVNC Instance #N<br/>:6901"]
        end

        API[Rust API<br/>Axum :3000]
        VITE[SvelteKit Dev<br/>Vite :5173]
        PG[(PostgreSQL)]

        subgraph "Filesystem"
            DYNAMIC["traefik/dynamic/<br/>Hot-loaded YAML"]
        end
    end

    UI -->|"GET /"| TRAEFIK
    UI -->|"POST /api/*"| TRAEFIK
    VNC -->|"wss /vnc/{token}/websockify"| TRAEFIK

    TRAEFIK -->|"api-router"| API
    TRAEFIK -->|"web-router"| VITE
    TRAEFIK -->|"vnc-auth middleware"| API
    TRAEFIK -->|"vnc-ws-router"| KASM1
    TRAEFIK -->|"vnc-ws-router"| KASM2
    TRAEFIK -->|"vnc-ws-router"| KASMN

    API --> PG
    API -->|"bollard (Docker API)"| KASM1
    API -->|"bollard"| KASM2
    API -->|"bollard"| KASMN
    API -->|"write YAML"| DYNAMIC

    TRAEFIK -.->|"watch: true"| DYNAMIC
```

## Request Routing

Traefik routes all incoming traffic on port 80. Four route types are evaluated by rule specificity (longer PathPrefix = higher priority):

```mermaid
flowchart LR
    REQ["Request:80"] --> CHECK{Rule Match}

    CHECK -->|"PathPrefix(/api)"| API["api-service<br/>→ host.docker.internal:3000"]
    CHECK -->|"PathPrefix(/vnc/{token}/websockify)"| VNCWS["vnc-{token}<br/>→ https://172.x.x.x:6901<br/>🔒 ForwardAuth"]
    CHECK -->|"PathPrefix(/vnc/{token})"| VNCPAGE["web-service<br/>→ SvelteKit :5173"]
    CHECK -->|"PathPrefix(/)"| WEB["web-service<br/>→ SvelteKit :5173"]

    style VNCWS fill:#1a3a2a,stroke:#4ecca3
    style API fill:#1a2a3a,stroke:#4ecca3
```

| Route | Service | Auth | Purpose |
|-------|---------|------|---------|
| `/api/*` | Rust API (3000) | JWT cookie | REST API |
| `/vnc/{token}/websockify` | KasmVNC container | ForwardAuth → `/api/vnc/verify` | WebSocket proxy |
| `/vnc/{token}` | SvelteKit (5173) | None | VNC viewer HTML |
| `/` | SvelteKit (5173) | None | Dashboard SPA |

## Dynamic VNC Routing

The key mechanism enabling scalable instance provisioning:

```mermaid
sequenceDiagram
    participant U as User
    participant A as API (Rust)
    participant FS as Filesystem
    participant T as Traefik
    participant K as KasmVNC Container

    U->>A: POST /api/instances {name: "dev-1"}
    A->>A: Create DB record, generate vnc_token
    A->>K: bollard: create_kasm_container()
    K-->>A: container_id, IP: 172.16.0.4
    A->>FS: Write vnc-{token}-ws.yml
    Note over FS: Rule: /vnc/{token}/websockify<br/>Service: https://172.16.0.4:6901<br/>Middleware: vnc-auth
    A->>FS: Write vnc-{token}-page.yml
    Note over FS: Rule: /vnc/{token}<br/>Service: web-service
    T->>FS: watch detects new .yml
    T->>T: Hot-reload: new routes active

    U->>T: GET /vnc/{token}/
    T->>A: ForwardAuth → /api/vnc/verify
    A-->>T: ✅ 200 + X-Forwarded-User
    T->>A: GET /vnc/{token}/ (SvelteKit)
    A-->>U: HTML page

    U->>T: wss /vnc/{token}/websockify
    T->>A: ForwardAuth → /api/vnc/verify
    A-->>T: ✅ 200
    T->>K: wss://172.16.0.4:6901/websockify
    K-->>U: WebSocket tunnel
```

### Why File Provider?

Traefik supports multiple providers (Docker, file, etcd, etc.). We chose **file provider with directory watching** because:

1. **Hot reload** — New YAML files are detected and loaded without restarting Traefik
2. **Full control** — The API generates exactly the routing rules needed per instance
3. **No label pollution** — KasmVNC containers don't need Traefik Docker labels
4. **Decoupled lifecycle** — Traefik and KasmVNC are independent; only the API orchestrates both

### Route File Format

When the API creates an instance with token `abc123`, two files are written to `traefik/dynamic/`:

**`vnc-abc123-ws.yml`** — WebSocket route:
```yaml
http:
  routers:
    vnc-abc123-ws:
      rule: "PathPrefix(`/vnc/abc123/websockify`)"
      service: "vnc-abc123"
      entryPoints:
        - web
      middlewares:
        - "vnc-auth"
  services:
    vnc-abc123:
      loadBalancer:
        serversTransport: "kasm-insecure"
        servers:
          - url: "https://172.16.0.4:6901"
```

**`vnc-abc123-page.yml`** — VNC viewer page:
```yaml
http:
  routers:
    vnc-abc123-page:
      rule: "PathPrefix(`/vnc/abc123`)"
      service: "web-service"
      entryPoints:
        - web
```

## Authentication Flow

```mermaid
sequenceDiagram
    participant U as Browser
    participant T as Traefik
    participant A as API

    Note over U: Login: POST /api/auth/login
    U->>A: {username, password}
    A->>A: bcrypt verify
    A-->>U: Set-Cookie: ow_token=JWT

    Note over U: Open VNC session
    U->>T: GET /vnc/{token}/websockify
    T->>T: Match vnc-ws-router
    T->>A: ForwardAuth: GET /api/vnc/verify<br/>Cookie: ow_token=JWT<br/>X-Forwarded-Uri: /vnc/{token}/websockify
    A->>A: Decode JWT → user_id, role
    A->>A: Extract VNC token from URI
    A->>A: Cache lookup (DashMap, O(1))
    alt Cache hit
        A-->>T: 200 OK + X-Forwarded-User
    else Cache miss
        A->>A: DB lookup: instance exists + status=running
        A->>A: Populate cache
        A-->>T: 200 OK + X-Forwarded-User
    end
    T->>K: Proxy to KasmVNC
```

### JWT Claims

```json
{
  "sub": "91c5b69a-6955-4256-8182-fe3002059630",
  "role": "admin",
  "exp": 1784870936
}
```

- **Cookie:** `ow_token` (HttpOnly, SameSite=Lax, Path=/, Max-Age=86400)
- **Secret:** `JWT_SECRET` environment variable
- **ForwardAuth** uses a separate code path (`vnc_verify`) that reads cookies from headers rather than Axum's cookie jar

## KasmVNC Container Lifecycle

```mermaid
stateDiagram-v2
    [*] --> Creating: POST /api/instances
    Creating --> Running: Container started<br/>+ Traefik route written
    Creating --> Error: Container create failed

    Running --> Stopped: POST /api/instances/{id}/stop
    Stopped --> Running: POST /api/instances/{id}/start

    Running --> Deleted: DELETE /api/instances/{id}
    Stopped --> Deleted: DELETE /api/instances/{id}
    Error --> Deleted: DELETE /api/instances/{id]

    Deleted --> [*]
```

### Container Creation Steps

1. **Pull image** — `kasmweb/desktop:1.19.0-rolling-daily`
2. **Create container** with env vars:
   - `VNCOPTIONS=-disableBasicAuth` (disable HTTP Basic Auth on websockify)
   - `KASM_VNC_PORT=6901`
   - `DISPLAY=:1`
3. **Connect to network** — `openworkspace-engin` (Docker bridge)
4. **Inject config** — Upload `kasmvnc.yaml` to `/etc/kasmvnc/kasmvnc.yaml` via tar stream
5. **Start container**
6. **Get IP** — Query Docker API for container IP on the bridge network
7. **Write Traefik routes** — Generate YAML files in `traefik/dynamic/`

### kasmvnc.yaml Configuration

The API injects a custom `kasmvnc.yaml` that:
- Enables SSL (KasmVNC requires it with `-sslOnly` hardcoded in entrypoint)
- Disables `require_ssl` (allows Traefik to proxy without presenting a client cert)
- Allows environment variable overrides

## Traefik Configuration

### Static Config (`traefik.yml`)

```yaml
entryPoints:
  web:
    address: ":80"
  websecure:
    address: ":443"

providers:
  file:
    directory: "/etc/traefik/dynamic"
    watch: true          # Key: auto-reload on file changes

api:
  dashboard: true        # Debug UI on :8080
  insecure: true
```

### Dynamic Config Files

| File | Purpose | Managed by |
|------|---------|------------|
| `static-routers.yml` | api-router, web-router, vnc-auth middleware | Manual |
| `static-services.yml` | api-service, web-service | Manual |
| `static-transports.yml` | `kasm-insecure` (skip TLS verify) | Manual |
| `vnc-{token}-ws.yml` | Per-instance WebSocket route | API |
| `vnc-{token}-page.yml` | Per-instance page route | API |

### ForwardAuth Middleware

```yaml
vnc-auth:
  forwardAuth:
    address: "http://host.docker.internal:3000/api/vnc/verify"
    trustForwardHeader: true
    authResponseHeaders:
      - "X-Forwarded-User"
      - "X-Forwarded-Role"
```

Traefik calls `/api/vnc/verify` before proxying any WebSocket request to a KasmVNC instance. The API validates the JWT cookie and instance ownership via an in-memory cache (`DashMap`), falling back to PostgreSQL on cache miss.

### TLS Handling

KasmVNC enforces `-sslOnly` (hardcoded in container entrypoint). All KasmVNC backends use self-signed certificates. The `kasm-insecure` serversTransport tells Traefik to skip certificate verification:

```yaml
http:
  serversTransports:
    kasm-insecure:
      insecureSkipVerify: true
```

## Network Topology

```mermaid
graph LR
    subgraph "Host"
        API_PORT["API :3000"]
        VITE_PORT["Vite :5173"]
        TRAEFIK_PORT["Traefik :80"]
        TRAEFIK_DASH["Traefik :8080"]
    end

    subgraph "openworkspace-engin (Docker Bridge)"
        TRAEFIK_N["Traefik"]
        KASM1_N["KasmVNC #1"]
        KASM2_N["KasmVNC #2"]
        KASMN_N["KasmVNC #N"]
    end

    TRAEFIK_PORT --> TRAEFIK_N
    TRAEFIK_DASH --> TRAEFIK_N
    TRAEFIK_N -->|"host.docker.internal"| API_PORT
    TRAEFIK_N -->|"host.docker.internal"| VITE_PORT
    TRAEFIK_N -->|"direct IP"| KASM1_N
    TRAEFIK_N -->|"direct IP"| KASM2_N
    TRAEFIK_N -->|"direct IP"| KASMN_N
```

- **Traefik ↔ API/Vite:** via `host.docker.internal` (host.docker.internal extra_hosts in compose)
- **Traefik ↔ KasmVNC:** direct container IP on `openworkspace-engin` bridge network
- **API ↔ Docker daemon:** Unix socket (`/var/run/docker.sock`)

## Database Schema

```mermaid
erDiagram
    users {
        uuid id PK
        varchar username UK
        varchar password_hash
        varchar role
        timestamptz created_at
        timestamptz updated_at
    }

    instances {
        uuid id PK
        varchar name
        serial instance_number UK
        varchar container_id
        varchar status
        uuid owner_id FK
        varchar vnc_token UK
        timestamptz created_at
        timestamptz updated_at
    }

    users ||--o{ instances : "owns"
```

### Instance Status Values

| Status | Meaning |
|--------|---------|
| `running` | Container running, Traefik route active |
| `stopped` | Container stopped, Traefik route removed |
| `error` | Container creation failed |

## Scalability Considerations

**Traefik file provider scales horizontally by design:**
- Each VNC instance = 2 small YAML files (~200 bytes each)
- Traefik watches for filesystem changes via inotify — O(1) detection
- Route matching is O(rule_length) per request, independent of total instance count
- No state stored in Traefik; all state lives in PostgreSQL

**Container network:**
- Docker bridge network supports ~65k containers (172.x.0.0/16 subnet)
- Each KasmVNC container gets its own IP; no port conflicts (no host port mapping)
- Traefik proxies to container IPs directly, no port allocation needed

**In-memory VNC cache (`DashMap`):**
- `vnc_token → { status, owner_id }` mapping stored in a lock-free concurrent HashMap
- Populated on API startup from DB (all running instances)
- Synchronized on instance create/start/stop/delete events
- `vnc_verify` reads cache first (O(1) hash lookup, no async); falls back to DB on cache miss
- Eliminates PostgreSQL round-trip on every WebSocket handshake
- Cache is per-process; sufficient for single-API deployment
