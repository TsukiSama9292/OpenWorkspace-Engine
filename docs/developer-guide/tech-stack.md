# OpenWorkspace Engine — Tech Stack & Deployment

> This document is the project's "constitution", article two: it **defines the
> technology decisions** and the **deployment/update flows**. Any technology
> change should record its rationale here before work begins.

---

## Technology Decision Overview

| Layer | Technology | Version | Rationale |
|---|---|---|---|
| **Control-plane API** | Rust + Axum | stable / axum 0.8 | Memory safety, zero-cost abstractions, <35MB RAM, high-concurrency non-blocking I/O |
| **Frontend** | SvelteKit 2 + Svelte 5 (static SPA) | 2.x / 5.x | Runes reactivity, adapter-static fully static, zero SSR CPU cost on the host |
| **CSS / UI** | Tailwind CSS v4 + Skeleton | v4 | Utility-class speed, existing component library |
| **Reverse proxy** | Traefik | v3.7.4 | File Provider + inotify hot-reloaded routes; **no Docker socket** |
| **Static assets** | Nginx | latest | HTTP caching, static SPA hosting |
| **Container orchestration** | bollard (Rust Docker API) | 0.18 | Async container/network lifecycle control |
| **Container runtime** | Docker OCI (runc) | ≥ 24 | Standard, fast instance creation |
| **Container runtime (hardened)** | gVisor (runsc) | latest | User-space kernel intercepts syscalls; optional per template |
| **Database** | PostgreSQL | 18-alpine | Single source of truth; sqlx compile-time checks + automatic migrations |
| **In-memory cache** | DashMap | — | O(1) VNC token lookup; avoids a DB round-trip per WebSocket handshake |
| **Network QoS** | Linux `tc`/HTB + `nsenter` | — | Kernel-level per-instance up/down bandwidth caps |
| **Package management** | pnpm + Turborepo | pnpm 9 / turbo 2 | Monorepo workspace dependencies + task orchestration and caching |
| **Instance images** | KasmVNC / Jupyter Lab / ttyd (self-built + `_dini` variants) | — | Prebuilt images can also be pulled from Docker Hub |

---

## Layer-by-layer Decisions and Rationale (ADR-style)

### 1. Control plane in Rust (not Go / Node / Python)

- **Safety**: the ownership model eliminates memory bugs (use-after-free, data
  races) at compile time — this is the process that drives the host's Docker
  socket and network; it cannot rely on a GC or type compromises.
- **Performance**: zero-cost abstractions, non-blocking I/O (tokio); very low
  RAM and CPU footprint, consistent with the "revive old hardware" mission.
- **Axum 0.8**: type-safe routes, extractors, and good tokio ecosystem
  integration.
- **bollard**: the most mature Rust Docker API client, async container/network
  control.

### 2. Frontend as a SvelteKit static site (not SSR, not React)

- **adapter-static + `ssr = false`**: the build output is pure static files
  served by Nginx — **zero SSR CPU cost on the host**, critical for a shared
  host.
- **Svelte 5 runes**: fine-grained reactivity, small bundle, fast load.
- **Multi-instance SPA**: the catch-all route (`[...path]`) detects
  `/kasmvnc/{token}/` and similar paths from `window.location.pathname`; one
  build serves the whole platform.
- **Tailwind v4 + Skeleton**: unified design language, fast admin-UI iteration.

### 3. Reverse proxy via Traefik File Provider (not Docker Provider, not dynamic nginx)

- **Hot-reloaded routes**: the API writes per-instance route YAML into a watched
  directory; Traefik picks it up via inotify and applies it **immediately —
  zero restart, zero downtime** — a new instance is reachable within seconds.
- **No Docker socket**: Traefik never mounts `/var/run/docker.sock`, shrinking
  the attack surface; routing is decided solely by the API (the only actor
  allowed to control containers).
- **Server-side Basic injection per token**: JS `WebSocket` cannot set custom
  headers, so a Traefik middleware injects `Authorization: Basic` server-side —
  the browser never sees the instance credentials.
- **Scaling characteristics**: one instance = one ~250-byte YAML; route
  matching cost is independent of instance count. Traefik is stateless; all
  state lives in PostgreSQL.

### 4. Dual container runtime: runC + gVisor (hardening)

- **runC (runc)**: the standard OCI runtime, best performance, GPU-compatible;
  the actual default — the API-level `container_runtime` defaults to `runc`
  and the server-level `OW_CONTAINER_RUNTIME` defaults to `runc`; templates
  with no runtime fall back to the server value.
- **runsc (gVisor)**: a user-space kernel intercepts syscalls, sharply reducing
  the container-escape risk; an optional hardening choice (slower), selected
  explicitly per template or via `OW_CONTAINER_RUNTIME`.
- **Resource reporting difference**: runsc virtualizes `/proc` (`free`/`htop`
  inside the container show its own CPU/RAM limits); runC shows the host's
  totals (limits are still enforced via cgroups). See
  [container-runtime.md](container-runtime.md).
- **NVProxy GPU passthrough**: gVisor `--nvproxy` proxies NVIDIA ioctls,
  supporting Turing/Ampere/Ada/Hopper (T4, A100/A10G, L4, H100).
- **Verified on our hardware**: Turing (GTX 1650) and Ampere (RTX 3060) work;
  Maxwell (GTX 970) fails.

### 5. PostgreSQL + tiered in-memory caching

- **PostgreSQL is the single source of truth**: users, groups, templates,
  instances, and persistent paths all live here; sqlx checks SQL at compile
  time and migrations run automatically on API startup.
- **DashMap cache**: a lock-free concurrent HashMap for `access_token → {status}`,
  making each WebSocket handshake token check an O(1) in-memory lookup (only a
  miss hits the DB).
- **Why not Redis/Valkey**: a single API process provides full cache
  consistency (cache and state in the same process), saving one stateful
  service. If the platform grows to multiple processes/hosts, a distributed
  cache can be revisited (see [caching-strategy.md](caching-strategy.md)).

### 6. JWT (identity-only) + permissions recomputed per request

- **JWT carries identity only** (`sub`, `exp`) — **no permission data**.
- Every request re-resolves the user's effective context from the DB (group
  flags, template whitelist, instance ceiling) — a permission change takes
  effect **on the next request**; a stale token can never carry stale rights.
- The token lives in an **HttpOnly cookie** (`ow_token`), keeping XSS from
  reading it; `SameSite=Lax`, `Secure` (in HTTPS environments).

### 7. Instance networking: per-instance `/30` + host-port publishing

- Each instance owns a `/30` subnet (2 usable IPs) as its own L2 segment —
  **east-west attacks are structurally impossible**.
- The service port is published to the host bridge gateway
  (`<host_gateway_ip>:<host_port>`); Traefik reaches it via
  `host.docker.internal` and never uses container IPs.
- Host ports and `/30` subnets are finite pools arbitrated across processes via
  **flock lockfiles** (`host_port.rs` / `instance_net.rs`), with TCP probes and
  bounded retries absorbing concurrent-snapshot races. See
  [lock-registry.md](lock-registry.md).

### 8. Persistence via "Local Bind-mounted Named Volume"

- The user's whole home directory maps to a fixed host path
  `{root}/{template_name}/{user_id}` (resolved by the API, absolute, `..`
  traversal-proof).
- A first (empty) mount auto-populates the image's built-in home settings, so
  the environment works out of the box.
- Deleting an instance **keeps the data**; only "reset" wipes it; restart
  re-declares any lost volume.

### 9. Package management: pnpm + Turborepo monorepo

- Two active apps: `apps/web` (SvelteKit frontend) and `apps/api` (Rust API).
- pnpm workspace + Turborepo task orchestration and caching; unified entry
  points `pnpm run dev` / `pnpm run build` / `pnpm test`.

### 10. Dependency version pinning

- **pako@1 pinned** — v3 broke import paths used internally by noVNC.
- **pnpm-lock.yaml is committed**, guaranteeing reproducible installs.
- Upgrading any dependency requires passing the full test suite (below) and a
  note of rationale in this document.

### 11. Host + per-instance monitoring: in-memory sampler, no DB, no chart library

- **Sampling cadence**: the 3-second `health_worker` tick keeps its duties; every
  5th tick (15 s) a `MetricsSampler` reads host metrics from `/proc` (stat /
  meminfo / mounts) and calls one-shot `docker stats` per active instance via
  the new `DockerService::container_stats()` seam. Failed reads log and skip
  (fail-open) — one dead container never stops the pass.
- **CPU % needs deltas**: cumulative CPU vs system time is read twice, and the
  first read returns no CPU % until a second sample exists (exactly how
  `docker stats` computes it); the client caches per-container counters so
  restarts never reuse stale deltas.
- **Two-tier in-memory store** (`MetricsStore`, pure module): Tier 1 holds 240
  samples at 15 s (1 hour); Tier 2 holds 288 five-minute mean+peak aggregates
  (24 hours) folded from Tier 1. Nothing is persisted — a 100-instance box
  costs ~2 MB, inside the < 35 MB API budget by two orders of magnitude.
- **Why no Redis/Postgres/Influx**: single-process consistency (same rationale
  as §5); monitoring data is ephemeral by design and not worth DB write
  volume. If multi-host arrives, this store becomes a candidate for a shared
  cache (see [caching-strategy.md](caching-strategy.md)).
- **Frontend**: charts are hand-rolled SVG (no chart library). The snapshot
  returns **timestamped two-tier points** per metric (`*_fine` 15 s + `*_coarse`
  5 min). A pure math module, `apps/web/src/lib/chart/`, owns the time↔x
  mapping, fine/coarse merging by visible window, nearest-point lookup, drag
  selection → zoom, zoom clamping, and follow-state transitions — DOM-free and
  unit-tested. A reusable `TimeSeriesChart` component renders hover crosshair +
  readout, click-to-pin, drag-select with live stats + auto-zoom, follow /
  "back to now", and the 1 h fine-data boundary marker; the small row
  sparklines use a lighter `Sparkline` (tooltip + pin). The Monitor tab polls a
  snapshot endpoint every 5 s while open.
- Access is gated by the group flag `can_view_monitoring` (Admin/Manager on by
  default, User off), a sixth flat-RBAC flag — see
  [../user-guide/rbac.md](../user-guide/rbac.md).

---

## Quality Gates

Hard rules of the development flow (cannot be bypassed):

1. **Rust zero-warning policy** — `cd apps/api && bash scripts/check.sh` twice
   (default + `docker` feature) **must produce no output**; no `#[allow(…)]`
   suppression attributes.
2. **Test suites** — `apps/api/scripts/run_tests.sh` (cargo nextest, needs
   Docker): 228 unit tests + 412 integration tests; `cd apps/web && pnpm test`:
   25 files / 310 tests.
3. **Typecheck + lint** — `cd apps/web && pnpm check`
   (svelte-kit sync + svelte-check + eslint, a hard gate).
4. Root `pnpm lint` acts on web only (eslint).

---

## Development Environment Flow (Dev)

### One-shot startup

```bash
pnpm install          # install workspace dependencies
pnpm run dev          # full dev stack
```

`pnpm run dev` runs, in order:
1. `kill-dev.sh` — frees ports 3000/5173 from leftover dev servers.
2. `pnpm run init` — creates `ow-network` (auto-picks a free `172.16-31.0.0/16`
   block) + registers the gVisor `runsc` runtime in `/etc/docker/daemon.json`.
3. `pnpm run docker:dev:up` — starts the dev Traefik + Postgres
   (`docker/openworkspace_dev/`).
4. `pnpm run network:allow` — grants `nsenter`/`tc` capabilities so the
   host-run API can shape bandwidth.
5. Runs the Rust API (`:3000`) and the Vite dev server (`:5173`) via
   `concurrently`.

Stop: `pnpm run dev:stop`; full teardown (including volumes):
`pnpm run dev:remove`.

### Dev-environment characteristics
- **Plain HTTP** (`http://localhost`) — browsers treat localhost as a secure
  context, so no certificates are needed.
- The dev Traefik proxies to dev servers **running on the host**
  (`host.docker.internal:5173` / `:3000`).
- The host-run API writes route YAML to
  `docker/openworkspace_dev/traefik/dynamic` (compile-time default when
  `TRAEFIK_DYNAMIC_DIR` is unset).
- Instances are still created by the host-run API via the Docker socket; the
  dev Postgres port is `55432:5432`.

---

## Production Deployment Flow (Prod)

### Initial deployment (first time)

```bash
pnpm run init                                   # ow-network + runsc runtime
pnpm run build:template-images                  # build the three instance images (incl. _dini variants)
docker compose -f docker/openworkspace/docker-compose.yml up -d --build
```

> The platform itself (`ow-web:latest` / `ow-api:latest`) is built by compose
> straight from source; no separate push needed.
> Instance images may alternatively be pulled from Docker Hub
> (`tsukisama9292/ow-*-ubuntu*`).

### Production architecture

| Service | Image | Port | Notes |
|---|---|---|---|
| `traefik` | traefik:v3.7.4 | `80`, `127.0.0.1:8080` | **file provider only** (no Docker socket); dynamic dir mounted read-only |
| `api` | ow-api (self-built) | — | **runs as root**, `pid: host`, `cap_add: [SYS_ADMIN, NET_ADMIN, SYS_PTRACE]`, `apparmor=unconfined`, rw Docker socket |
| `web` | ow-web (self-built) | — | SvelteKit static build served by nginx |
| `postgresql` | postgres:18-alpine | — | Volume `server-pgdata` |

- Prod Traefik routes by compose service name (`ow-api:3000`, `ow-web:80`),
  not `host.docker.internal`.
- The API container writes routes into `./traefik/dynamic` (its
  `TRAEFIK_DYNAMIC_DIR`, mounted rw into the API, ro into traefik at
  `/etc/traefik/dynamic`).
- Per-instance route files (`*-ws.yml`) are gitignored.

### HTTPS (TLS termination before Traefik)

This stack serves everything over plain HTTP (frontend, `/api`, VNC WebSocket).
**Do not enable TLS inside Traefik** — put a TLS-terminating proxy in front:

- **Cloudflare** — set the DNS record to **Proxied**; TLS terminates at the
  Cloudflare edge and forwards to `:80`; no certificate management needed.
- **Let's Encrypt** — front with a reverse proxy that auto-issues certs
  (Traefik ACME / Caddy / nginx) forwarding to `:80`.

> Self-signed/self-hosted CA certificates do not work: Chromium never ignores
> certificate errors on `fetch()` subresources, so `/api` requests fail with
> `ERR_CERT_AUTHORITY_INVALID`.

---

## Update Flow

### Code updates

```bash
git pull                                    # fetch latest code
docker compose -f docker/openworkspace/docker-compose.yml up -d --build
```

- `--build` rebuilds the `ow-web` / `ow-api` images.
- **DB migrations run automatically**: sqlx applies any pending migration on API
  startup (currently through `000023`) — no manual step.
- **Zero-downtime routes**: Traefik hot-reloads the dynamic directory; new
  routes are live within seconds; no traefik restart during updates.

### Instance image updates

```bash
pnpm run build:template-images              # rebuild local images, or
docker pull tsukisama9292/ow-*-ubuntu*       # pull the latest from the Hub
```

### Environment variable changes

Edit `.env` (next to the compose file) and run `docker compose up -d`. Note:
changing `JWT_SECRET` forces every logged-in user to re-login; changing
`POSTGRES_*` must match the credentials already in the `server-pgdata` volume.

### Version-control conventions

- All config, compose, and docs live in the repo; `*-ws.yml` (dynamic routes)
  and build artifacts do not.
- Upgrading any dependency (especially pako, Traefik, and Postgres major
  versions) must pass the full test suite before deploying.

---

## API Environment Variables

| Variable | Default | Description |
|---|---|---|
| `DATABASE_URL` | *(required)* | Postgres connection string |
| `JWT_SECRET` | *(required; must change in prod)* | Signing key for the `ow_token` JWT |
| `ADMIN_PASSWORD` | `admin` | Startup password for the seeded admin |
| `SERVER_HOST` / `SERVER_PORT` | `0.0.0.0` / `3000` | API bind address |
| `DB_MAX_CONNECTIONS` | `5` | sqlx connection-pool size |
| `OW_CONTAINER_RUNTIME` | `runc` | Server-level default container runtime (`runsc`, `runc`, …); applies when a template has none |
| `OW_HOST_GATEWAY_IP` | `172.17.0.1` | Host IP instance published ports bind to |
| `OW_HOST_PORT_START` / `OW_HOST_PORT_END` | `10000` / `20000` | Host-port pool |
| `OW_INSTANCE_NET_BASE` | `10.200.0.0/16` | CIDR base for per-instance `/30` subnets (must stay subnet-aligned) |
| `OW_INSTANCE_DNS` | `8.8.8.8,1.1.1.1` | DNS resolvers injected as `OW_DNS` (image entrypoint rewrites `/etc/resolv.conf`) |
| `TRAEFIK_DYNAMIC_DIR` | dev default | Directory where per-instance route YAML is written |

---

## Deprecated / Reference Items

| Item | Status | Notes |
|---|---|---|
| `references_repo/KasmVNC` | reference | Upstream KasmVNC source (`kasmweb/`) |
| `references_repo/gvisor` | reference | Upstream gVisor (shallow clone, `g3doc/` only) |
| `references_repo/docker-docs` | reference | Upstream Docker docs (`content/` only) |
| `users.role` / `is_system_admin` / `user_templates` | removed | Migrations `000018`–`000020` removed them; group-based RBAC instead |

---

## Known Limitations

- **Single API process** — DashMap cache and resource allocation are
  per-process consistent; multi-process contention for ports/subnets is handled
  by flock, but the cache remains per-process.
- **No CI** — no `.github` CI yet; quality gates rely on local scripts
  (`check.sh` / `run_tests.sh`).
- **Platform support: AMD64 + ARM64 only** — the Rust API and the published
  instance images (`tsukisama9292/ow-*-ubuntu*:jammy`, multi-arch) are built
  for AMD64 (x86_64) and ARM64 (aarch64); other platforms are untested. gVisor
  (`runsc`) is released only for these two architectures, so `runsc`-pinned
  templates cannot launch anywhere else. The platform's support envelope is
  bounded mainly by the Rust API server — other platforms may work, but are
  not verified.
- **GPU is NVIDIA-only + specific architectures** — NVProxy supports
  Turing/Ampere/Ada/Hopper.
- **tc/HTB needs root capabilities** — the host-run dev API requires
  `network:allow` (`sudo setcap` on `nsenter`/`tc`); the production container
  gets them via `cap_add` (`SYS_ADMIN`/`NET_ADMIN`/`SYS_PTRACE`) + `pid: host`,
  so no host-side grant is needed there. Failures are fail-open (logged, session
  not killed).

---

## Related docs

- [mission.md](../../mission.md) — mission and core features (constitution, article one)
- [roadmap.md](../../roadmap.md) — phase planning (constitution, article three)
- [development.md](development.md) — full development guide, debugging, env vars
- [architecture.md](../user-guide/architecture.md) — system architecture and DB schema
