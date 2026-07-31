# [OpenWorkspace Engine](https://github.com/TsukiSama9292/OpenWorkspace-Engine)

**Multi-tenant container orchestration platform** — provisions isolated containers (desktop, Jupyter Lab, terminal) on demand, exposed through Traefik reverse proxy with JWT authentication.

> Lightweight container orchestration with browser-based access — turn any server into a shared dev environment.

---

## Product Vision

### Core Pain Points

| Pain Point | Impact |
|---|---|
| **Hardware Inflation** | DRAM/GPU prices outpace budgets; labs and SMBs can't refresh hardware |
| **Resource Waste** | 2-5 year old servers with multi-core CPU + GB RAM sit idle (<10% utilization) |
| **Environment Chaos** | Direct host installation of CUDA/Python causes driver conflicts and system corruption |
| **Resource Monopoly** | One person occupies a whole machine; 90%+ CPU/RAM idle during off-hours |

### Value Pillars

1. **Hyper-Efficiency** — Docker containers eliminate 15-20% VM overhead; 8GB RAM hosts run multiple isolated environments
2. **Dynamic Allocation** — Create on demand, auto-stop when idle; hardware never sits unused
3. **Browser-as-Entry** — Zero client setup, no VPN; full GUI desktop in browser
4. **Zero-Trust Isolation** — Container isolation + JWT auth + Traefik ForwardAuth + cgroups limits

---

## Terminology

| Concept | Description |
|---|---|
| **Template** | Pre-configured settings bundle (image, resources, env vars) to launch instances from |
| **Instance** | A running VNC container launched from a template |
| **User** | Person with an account (admin / manager / user roles) |

See [docs/terminology.md](docs/terminology.md) for the full mapping from old names.

---

## Architecture

```
Browser ──> Traefik :80 ──> Rust API :3000 (Axum)
                        ──> SvelteKit SPA (nginx :80)
                        ──> KasmVNC containers :6901 (WebSocket)
```

**Key mechanism:** The API generates per-instance Traefik route YAML files into a watched directory. Traefik hot-reloads them via inotify — new VNC instances are immediately accessible without proxy restart.

See [docs/architecture.md](docs/architecture.md) for routing flows, container lifecycle, network topology, and DB schema.

---

## Tech Stack

| Layer | Technology | Rationale |
|---|---|---|
| **Control Plane API** | Rust + Axum 0.8 | Zero-cost abstraction, <20MB RAM, high-concurrency non-blocking I/O |
| **Frontend** | SvelteKit 2 + Svelte 5 (static SPA) | Runes reactivity, zero SSR CPU cost on host |
| **Remote Desktop** | KasmVNC | Browser-native HTML5 Canvas, WebSocket transport |
| **Reverse Proxy** | Traefik v3 | Docker Provider + file-based hot-reload routing |
| **Container Orchestration** | bollard 0.18 (Rust Docker API) | Async container lifecycle control |
| **Database** | PostgreSQL 18 + DashMap cache | Persistent storage + O(1) in-memory token verification |

---

## Project Structure

```
OpenWorkspace-Engine/
├── apps/
│   ├── api/                    # Rust/Axum REST API
│   │   ├── migrations/         # SQLx auto-migrations
│   │   └── src/
│   │       ├── main.rs         # Server entrypoint
│   │       ├── routes.rs       # HTTP handlers
│   │       ├── auth.rs         # JWT + cookie + RBAC
│   │       ├── db.rs           # PostgreSQL repositories
│   │       ├── docker.rs       # bollard Docker client
│   │       └── vnc_trafik.rs   # Traefik YAML generation
│   └── web/                    # SvelteKit frontend
│       └── src/
│           ├── lib/            # API client, stores, types, VNC components
│           └── routes/         # Pages (dashboard, login, instances, VNC, users)
├── docker/
│   └── openworkspace_dev/      # Dev infrastructure (Traefik + PostgreSQL)
├── scripts/                    # Kill-dev, network creation, cleanup
└── docs/                       # Documentation
```

---

## Quick Start

```bash
# Prerequisites: Node.js ≥18, pnpm 9, Rust (stable), Docker + Compose v2

pnpm run dev
```

This starts Traefik + PostgreSQL via Docker Compose, then runs the Rust API and Vite dev server concurrently.

> **Dev runs on plain HTTP** — open `http://localhost`. No certificates needed.

### HTTPS in Production

This stack serves everything (frontend, `/api`, VNC WebSocket) from one Traefik origin over plain HTTP. For HTTPS, do **not** enable TLS inside Traefik — put a TLS-terminating proxy in front instead:

- **Cloudflare** — enable **Proxied** on your DNS record; TLS is terminated at Cloudflare's edge and forwarded to `:80`. No cert management on your side.
- **Let's Encrypt** — run a front reverse proxy (Traefik ACME, Caddy, or nginx) that obtains certs automatically and proxies to `:80`.

Self-signed/local CA certs are not suitable: Chromium never bypasses certificate errors for `fetch()` subresource calls, so `/api` requests will fail with `ERR_CERT_AUTHORITY_INVALID`. See [docs/development.md](docs/development.md) for details.

See [docs/development.md](docs/development.md) for environment variables, debugging, and production build.

---

## Key Features

### Supported Interfaces
- **Desktop (KasmVNC)** — Full GUI Linux desktop in browser via HTML5 Canvas + WebSocket
- **Jupyter Lab** — Python data science environment with pre-installed kernels
- **Terminal (ttyd)** — Lightweight browser-based terminal for quick CLI access

### Security & Isolation
- **Cross-tenant isolation** — All instance traffic over SSL/TLS; each instance protected by a per-instance access token (94<sup>127</sup> combinations). Prevents tenants from directly accessing another tenant's instance via the container network — every request must go through the proxy with a valid token
- **gVisor sandboxing** — Template-level `Container Runtime` option to select `runsc(gVisor)`, intercepting high-risk syscalls for host protection
- **JWT Cookie Auth** — `ow_token` cookie + Traefik ForwardAuth for WebSocket upgrade validation
- **Headless Instance Auth** — Proxy injects credentials server-side for KasmVNC, Jupyter Lab, and ttyd; browser never sees secrets, users never manually auth to instances
- **Per-Instance RBAC** — Admin / Manager / User tiers with ownership verification on mutation endpoints

### User Package Management
- **Nix + User Namespaces** — Fully isolated package management without affecting the host
- **gVisor + sudo** — Alternative mode: run under `runsc(gVisor)` with sudo capability for traditional workflows

### Instance & Account Management
- **Lifecycle control** — Admins start, pause, and remove instances from the dashboard
- **Account administration** — Admins create accounts with role assignment (admin / manager / user)

### UI/UX
- **Single-page Dashboard** — All management in one view; no page-switching latency
- **Startup wait page** — Auto-detects instance readiness, then navigates directly into the instance interface

### Under the Hood
- **Dynamic VNC Routing** — Traefik file provider with directory watching; new instances hot-reload in seconds
- **DashMap Cache** — O(1) VNC token lookup skips DB round-trip on every WebSocket handshake
- **cgroups Resource Limits** — CPU cores + memory hard limits injected at container creation

| Documentation | Contents |
|---|---|
| [docs/architecture.md](docs/architecture.md) | System architecture, routing, lifecycle, DB schema |
| [docs/frontend.md](docs/frontend.md) | SvelteKit structure, components, CSS strategy |
| [docs/rbac.md](docs/rbac.md) | Role permissions matrix, implementation |
| [docs/vnc-auth.md](docs/vnc-auth.md) | VNC password flow, Traefik header injection, security model |
| [docs/caching-strategy.md](docs/caching-strategy.md) | DashMap vs Redis/Valkey decision guide |
| [docs/api-reference.md](docs/api-reference.md) | Complete REST API reference |
| [docs/development.md](docs/development.md) | Setup, commands, debugging, production |
| [docs/terminology.md](docs/terminology.md) | Canonical naming (Template / Instance / User) |

---

## Roadmap

| Phase | Focus | Status |
|---|---|---|
| **1** | Core infrastructure — dynamic routing, auth, container lifecycle, DB | ✅ Complete |
| **2** | Jupyter Lab, ttyd terminal, auto-sleep | 🔜 Partially complete |
| **3** | Cluster monitor, audit logging, Tailscale mesh, multi-host orchestration | 📋 Planned |

---

## Beliefs

1. **Targeted Realism** — Revive old hardware; solve the hardware anxiety of academic and small teams
2. **Pragmatic Sustainability** — Maximize utilization through software optimization, not endless hardware stacking
3. **Uncompromised DX** — The underlying hardware may be old, but the developer experience must be modern, smooth, and out-of-the-box
