# [OpenWorkspace Engine](https://github.com/TsukiSama9292/OpenWorkspace-Engine)

**Multi-tenant container orchestration platform** — provisions isolated containers (desktop, Jupyter Lab, terminal) on demand, exposed through Traefik reverse proxy with JWT authentication, auto-sleep, idle-based keep-time reclamation, per-template network bandwidth caps, and persistent user data that survives restarts.

> Lightweight container orchestration with browser-based access — turn any server into a shared dev environment.

---

## Design Philosophy: Security · Stability · Performance

This project is engineered to reach a **balanced optimum of `Security`, `Stability`, and `Performance`**. Every layer is chosen for a deliberate trade-off — no single property is maximized at the expense of the others:

| Layer | Technology | What it buys us |
|---|---|---|
| **Control Plane API** | Rust | **Security + Performance** — memory safety with zero-cost abstractions; <35MB RAM, high-concurrency non-blocking I/O |
| **Frontend** | SvelteKit | **Performance + DX** — ships a lightweight static SPA (small bundle, fast load) without abandoning development convenience (runes reactivity, batteries-included tooling) |
| **Reverse Proxy** | Traefik | **Stability + Performance** — efficient reverse proxying with **zero-downtime config** (file provider + inotify hot-reload; a bad/added route never requires a restart) |
| **Static Asset Serving** | Nginx | **Performance** — HTTP caching eliminates the I/O bottleneck of repeated asset requests |
| **Container Runtime** | Docker OCI + **runC** | **Performance** — fast instance creation with the standard OCI runtime |
| **Container Runtime (hardened)** | **gVisor (runsc)** | **Security** — a user-space kernel intercepts syscalls, drastically reducing container-escape risk; selectable per template as an alternative to runC |
| **Instance Networking** | Per-instance `/30` + host-published ports | **Security (network segmentation)** — see below |

### Why per-instance `/30` networks instead of one shared subnet

Managing every instance on a single flat virtual subnet (e.g. one `/16` or `/24`) is convenient, but it is also a **lateral-movement attack surface**: a compromised user could scan the shared segment and attack other instances from inside the network.

To reduce the attack surface at the network layer *before* it becomes a problem:

- The instance's **remote service port is published directly to a host port on the Docker bridge gateway** (`<host_gateway_ip>:<host_port>`) — Traefik reaches it via `host.docker.internal:<host_port>`, never a container IP.
- **Outbound internet access uses a dedicated `/30` subnet per instance**: one IP for the gateway, one IP for the instance. With only 4 addresses (2 usable), each instance is isolated into its own L2 segment — no peer instances exist on the same broadcast domain, so **east-west attacks between instances are structurally impossible**.

This is network-level isolation taken to its extreme: each container lives in its own bubble, and the only entrance to it is the single Traefik-controlled published port.

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

See [docs/architecture.md](docs/architecture.md) for the canonical naming and lifecycle details.

---

## Architecture

```
Browser ──> Traefik :80 ──> Rust API :3000 (Axum)
                        ──> SvelteKit SPA (nginx :80)
                        ──> KasmVNC / ttyd / Jupyter Lab containers
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
| **Reverse Proxy** | Traefik v3 | File Provider + inotify hot-reload routing (no Docker socket) |
| **Container Orchestration** | bollard 0.18 (Rust Docker API) | Async container lifecycle control |
| **Network QoS** | Linux `tc`/HTB | Kernel-level per-instance upload/download bandwidth caps |
| **Database** | PostgreSQL 18 + DashMap cache | Persistent storage + O(1) in-memory token verification |

---

## Project Structure

```
OpenWorkspace-Engine/
├── apps/
│   ├── api/                    # Rust/Axum REST API
│   │   ├── migration/          # SQLx auto-migrations
│   │   └── src/
│   │       ├── main.rs         # Server entrypoint
│   │       ├── routes/         # HTTP handlers (auth, templates, instances)
│   │       ├── health_worker.rs# Background worker (auto-sleep, keep-time)
│   │       ├── network_qos.rs  # tc/HTB bandwidth shaping logic
│   │       ├── docker.rs       # bollard Docker client
│   │       ├── db.rs           # PostgreSQL repositories
│   │       └── route_writer.rs # Traefik YAML generation
│   └── web/                    # SvelteKit frontend
│       └── src/
│           ├── lib/            # API client, stores, types, VNC components
│           └── routes/         # Pages (dashboard, login, instances, VNC, users)
├── docker/
│   ├── template_images/        # Dockerfiles for instance templates
│   │   ├── Dockerfile.jupyterlab_ubuntu
│   │   ├── Dockerfile.ttyd_ubuntu
│   │   └── Dockerfile.kasmvnc_ubuntu
│   ├── openworkspace/          # Production stack (Traefik + PostgreSQL + web + api)
│   └── openworkspace_dev/      # Dev infrastructure (Traefik + PostgreSQL)
├── scripts/                    # Kill-dev, network creation, cleanup
└── docs/                       # Documentation
```

---

## Quick Start

### Development

```bash
# Prerequisites: Node.js ≥18, pnpm 9, Rust (stable), Docker + Compose v2

pnpm run dev
```

This runs `kill-dev.sh` → creates the `ow-network` Docker network → starts Traefik + PostgreSQL via Docker Compose → runs the Rust API and Vite dev server concurrently.

> **Dev runs on plain HTTP** — open `http://localhost`. No certificates needed.

### Production

1. **Build (or pull) the template images.** From the repo root:

   ```bash
   # Option A — build locally (recommended; keep images in sync with this repo)
   # Builds the three regular images plus their *_dini (in-instance Docker) variants.
   pnpm run build:template-images
   ```

   ```bash
   # Option B — pull prebuilt images from Docker Hub
   docker pull tsukisama9292/ow-jupyter-ubuntu:jammy
   docker pull tsukisama9292/ow-ttyd-ubuntu:jammy
   docker pull tsukisama9292/ow-kasmvnc-ubuntu:jammy
   docker pull tsukisama9292/ow-jupyter-ubuntu-dini:jammy
   docker pull tsukisama9292/ow-ttyd-ubuntu-dini:jammy
   docker pull tsukisama9292/ow-kasmvnc-ubuntu-dini:jammy
   ```

   > New templates default to the `_dini` images so the `docker_in_instance`
   > switch works out of the box. The regular images remain available for
   > templates that never need an in-instance daemon.

2. **Start the production stack** (Traefik + PostgreSQL + nginx web + Rust API):

   ```bash
   cd /path/to/OpenWorkspace-Engine
   docker compose -f docker/openworkspace/docker-compose.yml up -d
   ```

   The compose file builds `ow-web:latest` and `ow-api:latest` from source, so no separate image build is needed for the platform itself. Instance containers are then launched on demand from the template images above.

   Set secrets via environment variables or a `.env` file next to the compose file: `JWT_SECRET` (change from default), `ADMIN_PASSWORD`, `POSTGRES_USER`/`POSTGRES_PASSWORD`/`POSTGRES_DB`.

> **Note on tc/HTB bandwidth shaping:** applying per-instance bandwidth caps requires `nsenter`/`tc` capabilities on the host. Run `pnpm run network:allow` once to grant them, or disable bandwidth limits on templates to skip `tc` entirely. See [docs/development.md](docs/development.md).

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
- **Cross-tenant isolation** — Every instance protected by a per-instance access token (62<sup>127</sup> combinations, 127 chars from `a-z A-Z 0-9`). Prevents tenants from directly accessing another tenant's instance via the container network — every request must go through the proxy with a valid token
- **gVisor sandboxing** — Template-level `Container Runtime` option to select `runsc(gVisor)`, intercepting high-risk syscalls for host protection
- **GPU passthrough (NVProxy)** — gVisor's `--nvproxy` proxies NVIDIA ioctls from the sandbox to the host driver. Officially supported GPUs are on **Turing / Ampere / Ada Lovelace / Hopper** (T4, A100/A10G, L4, H100); same-microarchitecture consumer cards (e.g. RTX 3090, RTX 4090) likely work — verified on Turing (GTX 1650) and Ampere (RTX 3060), Maxwell (GTX 970) fails. See [docs/gvison.md](docs/gvison.md)
- **JWT Cookie Auth** — `ow_token` cookie + Traefik ForwardAuth for WebSocket upgrade validation
- **Headless Instance Auth** — Proxy injects credentials server-side for KasmVNC, Jupyter Lab, and ttyd; browser never sees secrets, users never manually auth to instances
- **Per-Instance RBAC** — Admin / Manager / User tiers with ownership verification on mutation endpoints

### User Package Management
- **Nix + User Namespaces** — Fully isolated package management without affecting the host
- **gVisor + sudo** — Alternative mode: run under `runsc(gVisor)` with sudo capability for traditional workflows

### Resource Governance
- **Auto-Sleep (run-time limit)** — Per-template `max_run_seconds`; when an instance has been `running` past the limit, a background worker executes the configured `timeout_action` (`remove` / `stop` / `pause`). A browser countdown overlay warns the user and auto-redirects when the deadline hits.
- **Keep Time (idle reclamation)** — Per-template `keep_time_seconds` + `keep_time_action`. An instance stays alive only while its browser tab is **open, visible and focused** — the frontend sends a heartbeat every 10s while focused, and the worker reclaims (`pause` / `stop` / `remove`) the instance after it has been idle (tab hidden, unfocused, or never opened) for longer than `keep_time_seconds`.
- **Network Bandwidth Limiting** — Per-template `network_bandwidth_up_mbps` / `network_bandwidth_down_mbps` (0 = unlimited), enforced in the kernel with `tc`/HTB on the instance's veth pair. A single tenant can no longer saturate the host's link.

### Persistent User Data
- **Whole-home persistence** — A user's entire home directory survives stop, restart, and delete. Backed by a Docker **Local Bind-mounted Named Volume** at a fixed host path; the first (empty) mount auto-populates the image's built-in home files (`.bashrc`, X11/VNC/Jupyter configs), so environments start intact instead of masking to a blank screen.
- **Server-resolved paths** — The host path is resolved and validated **by the API** as `{root}/{template_name}/{user_id}` (absolute, no `..`, no injection). Clients never supply a path, so no tenant can mount an arbitrary host directory.
- **Three launch modes** — Per launch: **Use** persistent storage (reuse existing data), **No** persistent storage (ephemeral, unlimited), or **Reset** persistent storage (wipe the data and start fresh, with a frontend confirm warning).
- **One persistent instance per (Template, owner)** — A second persistent launch for the same template+user is rejected with 409 until the old one is removed.
- **Delete keeps data** — Removing an instance only removes its container, route, and DB record; the data stays on disk so a later `use_persistent` launch picks up exactly where you left off. Only an explicit reset wipes it.
- **Restart resilience** — Restarting re-declares a lost volume declaration and backfills the resolved path for legacy instances, never overwriting user data.

See [docs/persistent-storage.md](docs/persistent-storage.md) for the full design and lifecycle.

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
- **tc/HTB Bandwidth Shaping** — upload shaped on the container's egress `eth0`, download on the host-side veth; re-applied on every container start (Docker recreates the veth pair). Fail-open: shaping errors log and never kill a session
- **Background Lifecycle Worker** — 3-second scan enforcing auto-sleep deadlines and keep-time idle reclamation; heartbeat endpoint resets the idle clock while a tab is focused

| Documentation | Contents |
|---|---|
| [docs/architecture.md](docs/architecture.md) | System architecture, routing, lifecycle, DB schema |
| [docs/persistent-storage.md](docs/persistent-storage.md) | Persistent user data: paths, volume lifecycle, launch modes |
| [docs/gvison.md](docs/gvison.md) | gVisor/runsc sandboxing, NVProxy GPU passthrough, driver setup |
| [docs/frontend.md](docs/frontend.md) | SvelteKit structure, components, CSS strategy |
| [docs/rbac.md](docs/rbac.md) | Role permissions matrix, implementation |
| [docs/vnc-auth.md](docs/vnc-auth.md) | VNC password flow, Traefik header injection, security model |
| [docs/caching-strategy.md](docs/caching-strategy.md) | DashMap vs Redis/Valkey decision guide |
| [docs/api-reference.md](docs/api-reference.md) | Complete REST API reference |
| [docs/development.md](docs/development.md) | Setup, commands, debugging, production |

---

## Roadmap

| Phase | Focus | Status |
|---|---|---|
| **1** | Core infrastructure — dynamic routing, auth, container lifecycle, DB | ✅ Complete |
| **2** | Jupyter Lab, ttyd terminal, auto-sleep, keep-time idle reclamation, network bandwidth limits, persistent user data | ✅ Complete |
| **3** | Cluster monitor, audit logging, Tailscale mesh, multi-host orchestration | 📋 Planned |

---

## Beliefs

1. **Targeted Realism** — Revive old hardware; solve the hardware anxiety of academic and small teams
2. **Pragmatic Sustainability** — Maximize utilization through software optimization, not endless hardware stacking
3. **Uncompromised DX** — The underlying hardware may be old, but the developer experience must be modern, smooth, and out-of-the-box
