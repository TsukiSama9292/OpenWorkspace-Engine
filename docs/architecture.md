# System Architecture

## Overview

OpenWorkspace Engine is a multi-tenant virtual desktop platform. It provisions isolated containers (KasmVNC desktop, ttyd terminal, Jupyter Lab) on demand and exposes them as browser-accessible sessions through a Traefik reverse proxy.

**Core design principle:** Traefik's file-based provider with directory watching enables hot-reloadable routing. The API dynamically generates per-instance Traefik route YAML files, making new instances immediately accessible without proxy restart.

## Terminology

This project uses three canonical domain concepts:

| Concept | Canonical Name | Description |
|---------|---------------|-------------|
| Template | **Template** | A pre-configured settings bundle (image, resources, env vars) that users launch instances from |
| Instance | **Instance** | A running container (KasmVNC / ttyd / Jupyter) launched from a template |
| User | **User** | A person with an account. Authorization is **group-based** (no per-user role column): users belong to groups carrying permission flags, a template whitelist, and an instance ceiling, resolved into an effective context on every request (see [RBAC](rbac.md)) |

Sidebar tabs (gated by the effective context): **Instances** (own instance cards + quick launch, everyone), **Templates** and **Sessions** (shown for any `can_create_template` / `can_manage_users` / `can_manage_group_instances` holder or admin; template management itself requires `can_create_template` and the all-instances table requires `can_manage_group_instances`), **Volumes** (orphaned persistent volumes, `can_manage_users`), **Groups** (admin), **Users** (user table with policy CRUD, `can_manage_users`), **Settings**/**Monitor**/**Logs** (admin).

These names replaced legacy terminology (`WorkspaceConfig`→**Template**, `configs`/`config_id`/`config_name`→**templates**/`template_id`/`template_name`, `workspace_configs`→`workspace_templates`, `Workspaces` tab→**Instances**, old admin "Instances" tab→**Sessions**). New templates use the `_dini` images so the in-instance Docker switch works out of the box.

## High-Level Architecture

```mermaid
graph TB
    subgraph Browser
        UI[Dashboard UI<br/>SvelteKit SPA]
        SESSION[Session Viewer<br/>noVNC / iframe in browser]
    end

    subgraph "Host Machine"
        subgraph "Control Network: ow-network"
            TRAEFIK[Traefik<br/>:80]
            API[Rust API<br/>Axum :3000]
            WEB[Web (dev: Vite :5173<br/>prod: nginx :80)]
            PG[(PostgreSQL)]
        end

        subgraph "Per-instance /30 networks: ow-{instance-id}"
            INST1["Instance #1<br/>KasmVNC :6901"]
            INST2["Instance #2<br/>ttyd :7681"]
            INSTN["Instance #N<br/>Jupyter :8888"]
        end

        subgraph "Filesystem"
            DYNAMIC["traefik/dynamic/<br/>Hot-loaded YAML"]
        end
    end

    UI -->|"GET /"| TRAEFIK
    UI -->|"POST /api/*"| TRAEFIK
    SESSION -->|"wss /kasmvnc/{token}/websockify"| TRAEFIK

    TRAEFIK -->|"api-router"| API
    TRAEFIK -->|"web-router"| WEB
    TRAEFIK -->|"kasmvnc-{token}-ws"| INST1
    TRAEFIK -->|"ttyd-{token}-ws"| INST2
    TRAEFIK -->|"jupyter-{token}-ws"| INSTN

    API --> PG
    API -->|"bollard (Docker API)"| INST1
    API -->|"bollard"| INST2
    API -->|"bollard"| INSTN
    API -->|"write YAML"| DYNAMIC

    TRAEFIK -.->|"watch: true"| DYNAMIC
```

Traefik, the API, the web app, and Postgres share the `ow-network` control plane. **Instance containers do not join `ow-network`** — each instance gets its own dedicated `/30` bridge network (`ow-<instance_id>`), and Traefik reaches it through a host-published port on the Docker bridge gateway.

## Request Routing

Traefik routes all incoming traffic on port 80. The three static routers plus per-instance routes are evaluated by rule specificity (longer PathPrefix = higher priority):

```mermaid
flowchart LR
    REQ["Request:80"] --> CHECK{Rule Match}

    CHECK -->|"PathPrefix(/api)"| API["api-service<br/>→ host.docker.internal:3000"]
    CHECK -->|"PathPrefix(/kasmvnc/{token}/websockify)"| VNCWS["kasmvnc-{token}<br/>→ https://host.docker.internal:{host_port}<br/>🔒 Basic header injection"]
    CHECK -->|"PathPrefix(/ttyd/{token}/)"| TTYDWS["ttyd-{token}<br/>→ https://host.docker.internal:{host_port}<br/>🔒 Basic header injection"]
    CHECK -->|"PathPrefix(/jupyter/{token}/)"| JUPWS["jupyter-{token}<br/>→ https://host.docker.internal:{host_port}"]
    CHECK -->|"PathPrefix(/)"| WEB["web-service<br/>→ SvelteKit"]

    style VNCWS fill:#1a3a2a,stroke:#4ecca3
    style TTYDWS fill:#1a3a2a,stroke:#4ecca3
    style API fill:#1a2a3a,stroke:#4ecca3
```

| Route | Service | Auth | Purpose |
|-------|---------|------|---------|
| `/api/*` | Rust API (3000) | JWT cookie | REST API |
| `/kasmvnc/{token}/websockify` | KasmVNC container | Basic header injection | WebSocket proxy |
| `/ttyd/{token}/` | ttyd container | Basic header injection | Terminal proxy |
| `/jupyter/{token}/` | Jupyter container | Access token in URL | Notebook proxy |
| `/kasmvnc/{token}/` , `/open/{token}/` | SvelteKit | JWT cookie | Session viewer HTML |
| `/` | SvelteKit | JWT cookie | Dashboard SPA |

## Dynamic Instance Routing

The key mechanism enabling scalable instance provisioning:

```mermaid
sequenceDiagram
    participant U as User
    participant A as API (Rust)
    participant FS as Filesystem
    participant T as Traefik
    participant K as Instance Container

    U->>A: POST /api/instances {template_id, persistence}
    A->>A: Create DB record, generate access_token + access_password
    A->>A: Allocate free host port + free /30 subnet
    A->>K: bollard: create network + container
    K-->>A: container_id
    A->>FS: Write kasmvnc-{token}-ws.yml
    Note over FS: Rule: PathPrefix(/kasmvnc/{token}/websockify)<br/>Service: https://host.docker.internal:{host_port}<br/>Middleware: auth (Basic header) + strip
    T->>FS: watch detects new .yml
    T->>T: Hot-reload: new routes active

    U->>T: GET /kasmvnc/{token}/
    T->>T: Match web-router (SvelteKit)
    T->>A: GET /kasmvnc/{token}/ (SvelteKit page)

    U->>T: wss /kasmvnc/{token}/websockify
    T->>T: Inject Authorization: Basic header
    T->>K: wss://host.docker.internal:{host_port}/websockify
    K-->>U: WebSocket tunnel
```

### Why File Provider?

Traefik supports multiple providers (Docker, file, etcd, etc.). We chose **file provider with directory watching** because:

1. **Hot reload** — New YAML files are detected and loaded without restarting Traefik
2. **Full control** — The API generates exactly the routing rules needed per instance
3. **No label pollution** — Instance containers don't need Traefik Docker labels
4. **Decoupled lifecycle** — Traefik and instances are independent; only the API orchestrates both

### Route File Format

When the API creates a KasmVNC instance with token `abc123`, one file is written to `traefik/dynamic/`:

**`kasmvnc-abc123-ws.yml`** — WebSocket route + Basic auth injection + prefix strip:
```yaml
http:
  routers:
    kasmvnc-abc123-ws:
      rule: "PathPrefix(`/kasmvnc/abc123/websockify`)"
      service: "kasmvnc-abc123"
      entryPoints:
        - web
      middlewares:
        - "kasmvnc-abc123-auth"
        - "kasmvnc-abc123-strip"
  services:
    kasmvnc-abc123:
      loadBalancer:
        serversTransport: "kasm-insecure"
        servers:
          - url: "https://host.docker.internal:10000"
  middlewares:
    kasmvnc-abc123-auth:
      headers:
        customRequestHeaders:
          Authorization: "Basic a2FzbV91c2VyOnh4eHh4eHh4eHg="
    kasmvnc-abc123-strip:
      stripPrefix:
        prefixes:
          - "/kasmvnc/abc123"
```

ttyd and Jupyter get equivalent files (`ttyd-{token}-ws.yml`, `jupyter-{token}-ws.yml`); Jupyter has no auth middleware (it authenticates via `?token=` in the URL), and only KasmVNC/ttyd strip their prefix.

## Authentication Flow

```mermaid
sequenceDiagram
    participant U as Browser
    participant T as Traefik
    participant A as API
    participant K as Instance Container

    Note over U: Login: POST /api/auth/login
    U->>A: {username, password}
    A->>A: bcrypt verify
    A-->>U: Set-Cookie: ow_token=JWT (7 days)

    Note over U: Open session
    U->>T: wss /kasmvnc/{token}/websockify
    T->>T: Match kasmvnc-{token}-ws router
    T->>T: Inject Authorization: Basic header<br/>(from kasmvnc-{token}-auth middleware)
    T->>K: wss://host.docker.internal:{host_port}/websockify<br/>Authorization: Basic base64(kasm_user:{password})
    K->>K: Verify Basic auth
    K-->>T: 101 Switching Protocols
    T-->>U: WebSocket upgrade complete
```

### JWT Claims

The JWT is **identity-only** — user id and expiry, nothing else:

```json
{
  "sub": "91c5b69a-6955-4256-8182-fe3002059630",
  "exp": 1784870936
}
```

- **Cookie:** `ow_token` (HttpOnly, Secure, SameSite=Lax, Path=/, Max-Age=604800 = 7 days)
- **Secret:** `JWT_SECRET` environment variable
- Permissions are **never** carried in the token: every request resolves the user's effective context (flags, template whitelist, ceiling, tier) from the database, so a permission change takes effect on the next request — a stale token cannot outlive a policy edit. See [RBAC](rbac.md).
- **VNC cache:** the `vnc_verify` path (`/api/vnc/verify`) decodes the JWT directly from the `Cookie` header (not Axum's cookie jar) and validates the instance via the `VncCache` DashMap, falling back to the DB on cache miss. See [Caching Strategy](caching-strategy.md) and [VNC Authentication](vnc-auth.md).

## Instance Lifecycle

```mermaid
stateDiagram-v2
    [*] --> stopped: POST /api/instances (DB record created)
    stopped --> starting: POST /api/instances/{id}/start
    starting --> running: Health probe passes
    starting --> error: Health probe fails (120s)
    running --> paused: POST /api/instances/{id}/pause
    paused --> running: POST /api/instances/{id}/unpause
    running --> stopped: POST /api/instances/{id}/stop
    stopped --> running: POST /api/instances/{id}/start
    running --> stopped: timeout_action (auto-sleep / keep-time)
    running --> paused: timeout_action (auto-sleep / keep-time)
    running --> removed: timeout_action (auto-sleep / keep-time)
    stopped --> removed: DELETE /api/instances/{id}
    running --> removed: DELETE /api/instances/{id}
    error --> removed: DELETE /api/instances/{id}

    removed --> [*]
```

### Instance Status Values

| Status | Meaning |
|--------|---------|
| `running` | Container running, Traefik route active |
| `starting` | Container started, waiting on health probe |
| `paused` | Container paused (Docker pause), route removed |
| `stopped` | Container stopped, Traefik route removed |
| `error` | Container create/start failed or probe timed out |

For persistent instances, delete keeps the user's data (host dir + volume) so it can be reused by a later launch; only a reset wipes it. See [Persistent Storage](persistent-storage.md).

### Background worker (`health_worker.rs`)

Every 3 seconds the API runs three checks:

- **Health probe** — `starting` instances are probed at `https://{host_gateway_ip}:{host_port}/`; a successful response moves them to `running` (and backfills `started_at`/`last_seen_at`), 120s without success moves them to `error`.
- **Auto-sleep** — instances running past their template's `max_run_seconds` get the configured `timeout_action` (`remove`/`stop`/`pause`).
- **Keep-time** — instances idle past their template's `keep_time_seconds` get the configured `keep_time_action`. A live container session connection (checked via `has_session_connection`) resets the idle clock, so in-use sessions survive even if the browser focus heartbeat is missed.

### Container Creation Steps

1. **Allocate resources** — a free host port in the pool (`OW_HOST_PORT_START`–`OW_HOST_PORT_END`, default 10000–20000) and a free `/30` subnet from the instance net base (`OW_INSTANCE_NET_BASE`, default `10.200.0.0/16`).
2. **Create the dedicated network** — `ow-<instance_id>` bridge with `{subnet}/30`; the instance gets the `.2` address (gateway `.1`).
3. **Create container** with:
   - env vars: `VNC_PW=<127-char random>`, `KASM_VNC_PORT`/`DISPLAY` (KasmVNC), `OW_DNS` (DNS resolvers; the image entrypoint rewrites `/etc/resolv.conf` — user-defined bridges break Docker's embedded resolver under `runsc`), and `OW_DOCKER_IN_INSTANCE=true` when the template enables in-instance Docker.
   - runtime `runsc` (gVisor) when the template sets it; `docker`/`runc` otherwise.
   - `docker_in_instance` security profile: `--privileged` with no capability drops plus a `tmpfs` at `/var/lib/docker`; otherwise `NET_RAW`/`NET_ADMIN` are dropped.
   - port binding: the service port (KasmVNC `6901`, ttyd `7681`, Jupyter `8888`) published to `<host_gateway_ip>:<host_port>`.
4. **Inject config** — upload `kasmvnc.yaml` to `/etc/kasmvnc/kasmvnc.yaml` via tar stream (KasmVNC).
5. **Start container**, then apply tc/HTB bandwidth limits (see below).
6. **Write Traefik route** — generate the per-remote-type YAML in `traefik/dynamic/` targeting `https://host.docker.internal:<host_port>`.

### Network Bandwidth Limiting

Per-template `network_bandwidth_up_mbps` / `network_bandwidth_down_mbps` (0 = unlimited) are enforced with `tc`/HTB at the kernel on the instance's veth pair: upload shapes egress on `eth0` inside the container netns; download shapes egress on the host-side veth (located by reading the container's `eth0@ifN` via `nsenter`). The API container runs as root with `pid: host`, `cap_add: [SYS_ADMIN, NET_ADMIN]`, and `apparmor=unconfined`. Shaping is applied on every container start (Docker recreates the veth pair) and is **fail-open** — errors log and never kill a session.

## Traefik Configuration

### Static Config (`traefik.yml`)

```yaml
entryPoints:
  web:
    address: ":80"

providers:
  file:
    directory: "/etc/traefik/dynamic"
    watch: true          # Key: auto-reload on file changes

api:
  dashboard: true        # Debug UI on :8080
  insecure: true
```

The dev stack is **HTTP-only** by design (browsers treat `http://localhost` as a secure context). For HTTPS in production, terminate TLS in front of this stack — e.g. **Cloudflare** (proxied DNS) or **Let's Encrypt** via a front reverse proxy (Traefik ACME / Caddy / nginx) forwarding to `:80`.

### Dynamic Config Files

| File | Purpose | Managed by |
|------|---------|------------|
| `static-routers.yml` | api-router, web-router, vnc-auth forwardAuth middleware | Manual (committed) |
| `static-services.yml` | api-service, web-service | Manual (committed) |
| `static-transports.yml` | `kasm-insecure` (skip TLS verify) | Manual (committed) |
| `kasmvnc-{token}-ws.yml` | Per-instance KasmVNC route + Basic auth + strip | API (gitignored) |
| `ttyd-{token}-ws.yml` | Per-instance ttyd route + Basic auth + strip | API (gitignored) |
| `jupyter-{token}-ws.yml` | Per-instance Jupyter route | API (gitignored) |

### Per-Token Basic Auth Middleware

Instead of a browser-supplied header (which JS `WebSocket` can't set), Traefik injects `Authorization: Basic` per-token via a `headers` middleware:

```yaml
http:
  middlewares:
    kasmvnc-{token}-auth:
      headers:
        customRequestHeaders:
          Authorization: "Basic base64(kasm_user:{password})"
```

Each instance route YAML includes its own middleware with the correct credentials. The browser never sees these credentials — they are injected server-side by Traefik before proxying to the container.

See [VNC Authentication](vnc-auth.md) for full details.

### TLS Handling

KasmVNC enforces `-sslOnly` (hardcoded in container entrypoint). All instance backends use self-signed certificates. The `kasm-insecure` serversTransport tells Traefik to skip certificate verification:

```yaml
http:
  serversTransports:
    kasm-insecure:
      insecureSkipVerify: true
```

## Network Topology

```mermaid
graph LR
    subgraph "Control Plane (ow-network)"
        TRAEFIK_N["Traefik :80"]
        API_N["API :3000"]
        WEB_N["Web"]
        DB_N["Postgres"]
    end

    subgraph "Per-instance /30 (ow-{id})"
        KASM1_N["Instance #1<br/>10.200.0.2/30"]
        KASM2_N["Instance #2<br/>10.200.0.6/30"]
        KASMN_N["Instance #N<br/>10.200.0.10/30"]
    end

    TRAEFIK_N -->|"host.docker.internal:3000"| API_N
    TRAEFIK_N -->|"host.docker.internal:5173 / web-service"| WEB_N
    TRAEFIK_N -->|"host.docker.internal:{host_port}"| KASM1_N
    TRAEFIK_N -->|"host.docker.internal:{host_port}"| KASM2_N
    TRAEFIK_N -->|"host.docker.internal:{host_port}"| KASMN_N
```

- **Control plane:** Traefik, API, web, Postgres all live on `ow-network`. Dev routes reach the host-run API/Vite via `host.docker.internal` (`host-gateway` extra_host); production uses the `ow-service-network` service names (`ow-api`, `ow-web`).
- **Instances:** each instance has its own `/30` bridge network `ow-<instance_id>`. Its single service port is published to `<host_gateway_ip>:<host_port>` (default gateway `172.17.0.1`), and Traefik reaches it via `https://host.docker.internal:<host_port>` — never a container IP.
- **API ↔ Docker daemon:** Unix socket (`/var/run/docker.sock`, rw in the API container).

## Database Schema

```mermaid
erDiagram
    users {
        uuid id PK
        varchar username UK
        varchar password_hash
        integer direct_max_instances
        timestamptz created_at
        timestamptz updated_at
    }

    groups {
        uuid id PK
        varchar name UK
        text description
        varchar kind
        boolean can_create_template
        boolean can_manage_users
        boolean can_manage_group_instances
        boolean can_manage_docker
        boolean can_manage_registry
        integer max_instances
        timestamptz created_at
        timestamptz updated_at
    }

    user_groups {
        uuid user_id FK
        uuid group_id FK
    }

    group_templates {
        uuid group_id FK
        uuid template_id FK
    }

    persistent_volumes {
        uuid id PK
        uuid owner_id FK
        varchar host_path
        varchar status
        timestamptz created_at
    }

    workspace_templates {
        uuid id PK
        varchar name
        uuid owner_id FK
        varchar image
        integer cores
        bigint memory
        integer gpu_count
        varchar docker_registry
        varchar remote_type
        varchar container_runtime
        varchar persistent_storage_path
        bigint max_run_seconds
        varchar timeout_action
        bigint keep_time_seconds
        varchar keep_time_action
        integer network_bandwidth_up_mbps
        integer network_bandwidth_down_mbps
        boolean docker_in_instance
        varchar visibility
        json run_config
        json exec_config
        json volume_mappings
    }

    workspace_instances {
        uuid id PK
        varchar name
        uuid template_id FK
        uuid owner_id FK
        integer instance_number
        varchar container_id
        varchar status
        varchar access_token UK
        varchar access_password
        boolean mount_persistent
        varchar resolved_volume_host_path
        integer host_port
        timestamptz started_at
        timestamptz last_seen_at
        timestamptz created_at
        timestamptz updated_at
    }

    registry_config {
        varchar url
    }

    registry_cache {
        json data
        timestamptz updated_at
    }

    users ||--o{ user_groups : "member of"
    groups ||--o{ user_groups : "has members"
    groups ||--o{ group_templates : "whitelists"
    workspace_templates ||--o{ group_templates : "is whitelisted on"
    users ||--o{ workspace_templates : "owns"
    users ||--o{ workspace_instances : "owns"
    workspace_templates ||--o{ workspace_instances : "launches"
    users ||--o{ persistent_volumes : "owner"
```

Authorization lives in the group tables: `users.role`, `users.is_system_admin`, and `user_templates` were dropped (migrations `000018`–`000020`); admin status is **Admin-group membership**, the template whitelist is the union of each member group's `group_templates` rows, and `groups.max_instances`/`users.direct_max_instances` feed the effective instance ceiling. See [RBAC](rbac.md).

Access credentials are `access_token` / `access_password` (renamed from `vnc_token` / `vnc_password` by migration `000008` when ttyd/Jupyter support landed).

## Scalability Considerations

**Traefik file provider scales horizontally by design:**
- Each instance = 1 small YAML file (~250 bytes)
- Traefik watches for filesystem changes via inotify — O(1) detection
- Route matching is O(rule_length) per request, independent of total instance count
- No state stored in Traefik; all state lives in PostgreSQL

**Instance networks:**
- Each instance consumes a `/30` (4 IPs) from `OW_INSTANCE_NET_BASE`. Default `10.200.0.0/16` provides 16,384 instance subnets.
- Host ports are drawn from a configurable pool (default 10000–20000) and instance `/30` subnets from `OW_INSTANCE_NET_BASE`; both allocators (`host_port.rs`, `instance_net.rs`) arbitrate across processes with non-blocking `flock` lockfiles in a shared per-UID lock directory, with a token-derived spread on retry.

**In-memory VNC cache (`DashMap`):**
- `access_token → { status }` mapping stored in a lock-free concurrent HashMap
- Populated on API startup from DB (all instances), synchronized on create/start/stop/delete events
- `vnc_verify` reads cache first (O(1) hash lookup); falls back to DB on cache miss
- Eliminates PostgreSQL round-trip on every WebSocket handshake
- Cache is per-process; sufficient for single-API deployment (see [Caching Strategy](caching-strategy.md))

## VNC Authentication

See [VNC Authentication](vnc-auth.md) for full details on password generation, Traefik header injection, and the security model.
