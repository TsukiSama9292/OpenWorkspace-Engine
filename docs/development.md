# Development Guide

## Prerequisites

- **Node.js** ≥ 18 + **pnpm** 9
- **Rust** (stable) + **cargo**
- **Docker** + **Docker Compose** v2
- **PostgreSQL** (via Docker — no local install needed)

## Quick Start

```bash
# 1. Create Docker network for KasmVNC containers
pnpm run init

# 2. Start infrastructure (Traefik + PostgreSQL)
pnpm run docker:dev:up

# 3. Start API (Rust) + Frontend (Vite) concurrently
pnpm run dev
```

Or simply:

```bash
pnpm run dev
```

This runs `kill-dev.sh` → `init` → `docker:dev:up` → `concurrently api + web`.

## Project Structure

```
OpenWorkspace-Engine/
├── apps/
│   ├── api/                        # Rust/Axum REST API
│   │   ├── migrations/             # SQLx auto-migrations
│   │   └── src/
│   │       ├── main.rs             # Server entrypoint
│   │       ├── routes.rs           # All HTTP handlers
│   │       ├── auth.rs             # JWT auth + cookie handling
│   │       ├── db.rs               # PostgreSQL repositories
│   │       ├── docker.rs           # Bollard Docker client
│   │       └── vnc_trafik.rs       # Traefik YAML generation
│   └── web/                        # SvelteKit frontend
│       └── src/
│           ├── lib/
│           │   ├── api.ts          # API client
│           │   ├── stores/         # Svelte stores (auth, theme)
│           │   ├── vnc-components/ # VNC viewer UI components
│           │   └── vnc/            # noVNC core (JS)
│           └── routes/             # Page routes
├── docker/
│   └── openworkspace_dev/          # Dev infrastructure
│       ├── docker-compose.yml      # Traefik + PostgreSQL
│       └── traefik/
│           ├── traefik.yml         # Static config
│           └── dynamic/            # Hot-loaded route configs
│               ├── static-routers.yml
│               ├── static-services.yml
│               └── static-transports.yml
├── scripts/
│   ├── kill-dev.sh                 # Kill stale dev processes
│   └── docker-network.sh           # Create Docker network
└── docs/                           # This documentation
```

## Commands

### Root Level

| Command | Description |
|---------|-------------|
| `pnpm run dev` | Full dev restart (kill → network → compose → concurrently) |
| `pnpm run dev:api` | Start API only (`cargo run`) |
| `pnpm run dev:web` | Start Vite dev server only |
| `pnpm run dev:stop` | Stop Docker infrastructure |
| `pnpm run docker:dev:up` | Start Traefik + PostgreSQL |
| `pnpm run docker:dev:down` | Stop Traefik + PostgreSQL |
| `pnpm run init` | Create `openworkspace-engin` Docker network |
| `pnpm run build` | Build SvelteKit for production |
| `pnpm run check` | Type-check SvelteKit |
| `pnpm run lint` | Lint SvelteKit |

### API (Rust)

```bash
cd apps/api

cargo run                   # Start API server (:3000)
cargo build                 # Compile check
cargo build --release       # Release build
RUST_LOG=debug cargo run    # Verbose logging
```

### Frontend (SvelteKit)

```bash
cd apps/web

pnpm dev        # Start Vite dev server (:5173)
pnpm build      # Build to static files
pnpm check      # Type-check
```

## Environment Variables

### API (`.env` in `apps/api/`)

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | `postgres://postgres:postgres@localhost:55432/postgres` | PostgreSQL connection string |
| `JWT_SECRET` | — | Secret for JWT signing |
| `ADMIN_PASSWORD` | `admin` | Default admin user password |
| `RUST_LOG` | `info` | Log level filter |

### Docker Compose (`docker/openworkspace_dev/.env`)

| Variable | Default | Description |
|----------|---------|-------------|
| `POSTGRES_USER` | `postgres` | PostgreSQL username |
| `POSTGRES_PASSWORD` | `postgres` | PostgreSQL password |
| `POSTGRES_DB` | `postgres` | Database name |
| `JWT_SECRET` | `change-me-in-production` | JWT secret (must match API) |

## Dev Process Flow

```mermaid
flowchart TD
    A["pnpm run dev"] --> B["kill-dev.sh<br/>Kill stale processes"]
    B --> C["docker-network.sh<br/>Create openworkspace-engin"]
    C --> D["docker compose up -d<br/>Start Traefik + PostgreSQL"]
    D --> E["concurrently"]
    E --> F["cargo run<br/>API :3000"]
    E --> G["vite dev<br/>SvelteKit :5173"]

    F --> H["Run migrations"]
    H --> I["Seed admin user"]
    I --> J["Listen on :3000"]

    G --> J2["HMR enabled"]
```

## Traefik Dynamic Config Workflow

When a VNC instance is created, the API writes route files to `traefik/dynamic/`. Traefik watches this directory and hot-reloads new routes within seconds.

```mermaid
flowchart LR
    A["API: create_instance()"] --> B["Write vnc-{token}-ws.yml"]
    A --> C["Write vnc-{token}-page.yml"]
    B --> D["traefik/dynamic/"]
    C --> D
    D -->|"inotify watch"| E["Traefik detects change"]
    E --> F["Routes active"]

    G["API: delete_instance()"] --> H["Remove vnc-{token}-*.yml"]
    H --> D
```

**Files in `traefik/dynamic/`:**

- `static-routers.yml` — Committed to git (core routing rules)
- `static-services.yml` — Committed to git (backend services)
- `static-transports.yml` — Committed to git (TLS settings)
- `vnc-*-ws.yml` — **Generated by API** (gitignored)
- `vnc-*-page.yml` — **Generated by API** (gitignored)

## Debugging

### Check Traefik Routes

```bash
# Traefik dashboard (API)
curl -s http://localhost:8080/api/http/routers | python3 -m json.tool

# List VNC routes specifically
curl -s http://localhost:8080/api/http/routers?search=vnc | python3 -m json.tool
```

### Check Container Status

```bash
# List KasmVNC containers
docker ps --filter "name=ow-kasm"

# Check container IP
docker inspect ow-kasm-1 --format '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}'

# View container logs
docker logs ow-kasm-1
```

### Check Traefik Logs

```bash
docker logs ow-traefik -f
```

### Common Issues

| Symptom | Cause | Fix |
|---------|-------|-----|
| 404 on `/vnc/{token}/websockify` | Traefik route file not loaded | Check files exist in `traefik/dynamic/`, verify `watch: true` |
| WebSocket 1006 | TLS verification failed | Ensure `kasm-insecure` transport exists in `static-transports.yml` |
| ForwardAuth 401 | JWT expired or invalid | Re-login to get fresh `ow_token` cookie |
| Container not starting | Image pull failed | Check `docker logs ow-kasm-{n}` |
| Vite HMR showing instead of VNC | Route priority conflict | The `vnc-page` route should have higher priority than `web-router` |

## Production Build

```bash
# Build SvelteKit
pnpm run build

# Build API release
cd apps/api && cargo build --release

# Start production stack
pnpm run docker:up
```

Production uses `docker/openworkspace/docker-compose.yml` which includes:
- **nginx** serving `apps/web/build/` (static files)
- **api** running the compiled Rust binary
- **Traefik** with the same file-based routing
- **PostgreSQL**

No Vite dev server; nginx replaces it as the web-service backend.
