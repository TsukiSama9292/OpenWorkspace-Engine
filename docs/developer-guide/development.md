# Development

## Prerequisites

| Tool | Version | Purpose |
|------|---------|---------|
| Docker Engine | ≥ 24 | Containers, networks, Traefik |
| Docker Compose | ≥ 2.20 | Stack orchestration |
| pnpm | 9.x | Workspace/package manager |
| Node.js | ≥ 18 | SvelteKit/Vite toolchain |
| Rust toolchain | stable | API (Axum), migrations, nextest |

> **Supported platforms:** AMD64 (x86_64) and ARM64 (aarch64). The Docker Hub
> template images are published multi-arch for both; other architectures are
> untested. gVisor (`runsc`) is only available on these two architectures, so
> `runsc`-pinned templates cannot launch elsewhere.

## Quick Start

```bash
pnpm install                  # install workspace deps
pnpm run init                 # create ow-network + register gVisor runsc runtime
pnpm run dev                  # full dev stack (API + web + Traefik + Postgres)
```

`pnpm run dev` is the one-shot entry point. It:

1. `kill-dev.sh` — free ports 3000/5173 from stale dev servers
2. `init` — ensure `ow-network` exists (auto-selects a free `172.16-31.0.0/16` subnet) and gVisor `runsc` runtime is registered in `/etc/docker/daemon.json`
3. `docker:dev:up` — start dev Traefik + Postgres containers (`docker/openworkspace_dev/docker-compose.yml`)
4. `network:allow` — grant `cap_sys_ptrace,cap_sys_admin+ep` to `/usr/bin/nsenter` and `cap_net_admin+ep` to `/usr/sbin/tc` so the host-run API can shape bandwidth
5. run the Rust API (`:3000`) and Vite dev server (`:5173`) with `concurrently`

Stop with `pnpm run dev:stop` (compose down + revoke caps) or `pnpm run dev:remove` (full wipe incl. volumes).

**No-sudo variant:** `pnpm run dev:nosudo` skips both privileged steps — the gVisor `runsc` registration (`init`) and `network:allow` (capability grants). It still ensures `ow-network`, starts Traefik + Postgres, and runs API + web, so no password prompt is needed. Trade-offs: bandwidth shaping (tc/nsenter) fails open (logged, not enforced), and `runsc`-pinned templates cannot launch unless gVisor was registered previously. Stop with `pnpm run dev:stop:nosudo` (compose down only).

> **Dev routing note:** the dev Traefik proxies to the **host-run** servers via `host.docker.internal` (`:5173` / `:3000`), and the host-run API writes route YAMLs to `docker/openworkspace_dev/traefik/dynamic` (its compile-time default when `TRAEFIK_DYNAMIC_DIR` is unset). Instances are still created by the host-run API via the Docker socket, and Traefik reaches them through host-published ports.

### Dev Compose Stack

`docker/openworkspace_dev/docker-compose.yml` (Traefik v3.7.4 + Postgres 18-alpine only — the API and web run on the host):

| Service | Container name | Ports | Notes |
|---------|---------------|-------|-------|
| `traefik` | `ow-dev-traefik` | `80`, `8080` | Docker provider (instances) + file provider (`/etc/traefik/dynamic`, watch); `host.docker.internal:host-gateway` |
| `postgresql` | `ow-dev-postgres` | `55432:5432` | Volume `server-dev-pgdata`; `TZ=Asia/Taipei` |

`ow-network` is **external** (created by `scripts/docker-network.sh`, not by compose).

## Monorepo Layout

```
OpenWorkspace-Engine/
├── apps/
│   ├── web/              # SvelteKit frontend (Svelte 5, Tailwind v4, Skeleton)
│   └── api/              # Rust Axum API (bollard → Docker, sqlx → Postgres)
├── scripts/              # host tooling (docker-network, gvisor runtime, cleanup, ...)
├── docker/
│   ├── openworkspace/        # production compose + Dockerfiles
│   ├── openworkspace_dev/    # dev compose (Traefik + Postgres)
│   ├── template_images/      # build.sh + instance image variants
│   └── base_images/          # shared base images
├── docs/                 # user guide + developer guide
├── references_repo/      # upstream sources (KasmVNC, gVisor, Docker docs)
├── apps/api/migration/   # sqlx migrations (000001–000023)
└── package.json          # turbo orchestration + dev scripts
```

The API (see `apps/api/`):

```
apps/api/
├── src/
│   ├── main.rs               # bootstrap: settings, DB + migrations, seed admin, VNC cache, CORS
│   ├── core/                 # settings, jwt, errors, hash, config
│   ├── routes/               # auth, users, groups, admin_settings + workspace/{instances, templates, registry, docker_raw, persistent_volumes}
│   ├── docker.rs             # DockerService (mockable seam) + DockerClient (bollard container/network/volume/bandwidth ops)
│   ├── host_port.rs          # host-port pool allocator
│   ├── instance_net.rs       # per-instance /30 subnet allocation
│   ├── route_writer.rs       # per-instance Traefik YAML generation
│   ├── network_qos.rs        # tc/HTB arg builders + veth matcher (pure, unit-tested)
│   ├── health_worker.rs      # probe / auto-sleep / keep-time worker (3s tick)
│   └── vnc_cache.rs          # DashMap access_token → {status}
├── migration/src/            # sqlx migrations (000001–000023)
├── scripts/                  # check.sh, run_tests.sh, create_test_pg.sh
└── tests/                    # integration tests (nextest, --features docker)
```

## API Environment Variables

All variables are read via `core/settings.rs` (`Settings::from_env`). Only `DATABASE_URL` and `JWT_SECRET` are **required**; the rest have defaults.

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | *(required)* | Postgres connection string |
| `JWT_SECRET` | *(required)* | Signing secret for the `ow_token` JWT |
| `ADMIN_PASSWORD` | `admin` | Bootstrap password for the seeded admin user |
| `SERVER_HOST` / `SERVER_PORT` | `0.0.0.0` / `3000` | API bind address |
| `DB_MAX_CONNECTIONS` | `5` | sqlx connection pool size |
| `OW_CONTAINER_RUNTIME` | `runc` | Server-level default container runtime (`runsc`, `runc`, …); used when a template doesn't pin its own |
| `OW_HOST_GATEWAY_IP` | `172.17.0.1` | Host IP instances publish their ports on |
| `OW_HOST_PORT_START` / `OW_HOST_PORT_END` | `10000` / `20000` | Host-port pool for instance services |
| `OW_INSTANCE_NET_BASE` | `10.200.0.0/16` | CIDR base for per-instance `/30` nets (must be net-aligned; `NetBase::parse` validates) |
| `OW_INSTANCE_DNS` | `8.8.8.8,1.1.1.1` | DNS resolvers injected as `OW_DNS` (container entrypoint rewrites `/etc/resolv.conf`) |
| `TRAEFIK_DYNAMIC_DIR` | dev default | Where per-instance route YAMLs are written (defaults to `docker/openworkspace_dev/traefik/dynamic` when unset) |

In dev, the API process reads a `.env` file next to the workspace root (loaded via `dotenvy`); the dev compose file forwards `${POSTGRES_USER:-postgres}` / `${POSTGRES_PASSWORD:-postgres}` / `${POSTGRES_DB:-postgres}` to Postgres.

## Scripts

### `scripts/`

| Script | Purpose |
|--------|---------|
| `docker-network.sh` | Create `ow-network` bridge on a free `172.16-31.0.0/16` subnet (idempotent) |
| `docker-runtime-gvisor.sh` | Download `runsc` (gVisor) + merge the runtime into `/etc/docker/daemon.json` (JSON merge, `.bak` backup, idempotent) |
| `test-docker-runtime-gvisor.sh` | Test that the gVisor runtime registration works |
| `cleanup.sh` | Clean up dev artifacts: `tests` \| `instances` \| `network` \| `traefik` \| `all` (see below) |
| `kill-dev.sh` | Free ports 3000/5173 from stale dev servers |
| `apply_bw_smoke.sh` (in `apps/api/scripts/`) | Verify a live host actually shapes bandwidth (uses `python3`, `busybox:1` image) |
| `network_isolation_smoke_test.sh` | Smoke test that instance networks are isolated from the control plane |
| `dini_smoke_test.sh` | Smoke test for the `_dini` (Docker-in-instance) images |
| `benchmark/benchmark-prod.sh` | Production stack CPU/RAM benchmark (pure bash): preflight → compose up → platform window → six concurrent instances → teardown → CSV + Markdown report |
| `benchmark/smoke_test.sh` | Live end-to-end verification of the benchmark (`--smoke`): platform healthy, six instances running, report produced, host clean afterwards |

### `scripts/benchmark/`

Measure the production compose stack's CPU/RAM — idle baseline, per-instance cost by
remote type (KasmVNC / ttyd / Jupyter) and container runtime (runC vs runsc/gVisor),
and the host before→after delta. Pure bash (bash + docker + curl + jq; no Node/Python).

```bash
# Fast end-to-end check (short windows, ~5s each) — also run by smoke_test.sh
./scripts/benchmark/benchmark-prod.sh --smoke

# Full run: 60s per window, six instances under both runtimes
./scripts/benchmark/benchmark-prod.sh

# Just the platform-idle phase (compose up + 60s platform window)
./scripts/benchmark/benchmark-prod.sh --phase 2
```

Requirements: `runsc` registered (`pnpm run init`), port 80 free (no dev stack), the
six dini images present (auto-built by the repo image script when missing), and admin
creds via `OW_ADMIN_USER` / `OW_ADMIN_PASSWORD` (defaults `admin`/`admin`). Output lands
in `scripts/benchmark/reports/bench-<timestamp>/` (gitignored). The pure sampling /
parsing / aggregation / Markdown functions live in `benchlib.sh`, unit-tested with
fixtures in `scripts/benchmark/tests/` (no Docker needed).

### `scripts/cleanup.sh`

Subcommands (`--verbose` supported):

- `tests` — remove test Postgres + instance containers/networks created by the test suite (net pattern `^ow-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$`); the test dynamic dir is `apps/api/target/traefik-dynamic`
- `instances` — stop/remove leftover dev instances and their `ow-*` networks
- `network` — remove `ow-network`
- `traefik` — clean the dev dynamic dir
- `all` — everything above

### `apps/api/scripts/`

| Script | Purpose |
|--------|---------|
| `check.sh` | **Zero-warning gate.** Runs `cargo check --lib` and `cargo clippy --all-targets --all-features -- -D warnings` for both the default and the `docker` feature sets, greps for `warning`/`error`. Both must produce **no output**. |
| `create_test_pg.sh` | Start/stop a throwaway Postgres container for the test suite |
| `run_tests.sh` | Full suite: starts test Postgres, points `TRAEFIK_DYNAMIC_DIR` at `apps/api/target/traefik-dynamic`, runs `cargo nextest run --features docker`, cleans up via trap |
| `apply_bw_smoke.sh` | End-to-end bandwidth-shaping smoke test |
| `security_api.sh` | Dual-pass Schemathesis security fuzzing against a running dev stack (see "Security Fuzzing") |

## Testing

### Rust API (`apps/api`)

```bash
cd apps/api
bash scripts/check.sh                # warning gate (must be silent)
bash scripts/run_tests.sh            # cargo nextest run --features docker
```

Coverage today: **228 unit tests** in `src/` and **412 integration tests** in `tests/` (auth, db, docker lifecycle, instances, monitor, registry, templates, users, vnc-verify, health). Integration tests require Docker (they create real containers/networks) and run in parallel via `cargo nextest`.

### Web (`apps/web`)

```bash
cd apps/web
pnpm check        # svelte-kit sync + svelte-check (typecheck) + eslint (hard lint gate)
pnpm test         # vitest run — 25 files, 310 tests
```

Vitest uses `happy-dom` (unit + component tests). Playwright E2E lives in the standalone `e2e/` package and requires a **running** dev stack (`pnpm run test:e2e` smoke / `test:e2e:full` live VNC). Not run in CI.

## Security Fuzzing

```bash
pnpm run dev:nosudo        # pre-condition: the dev stack must be running
pnpm run security:api      # regenerate spec → provision fuzz-user → Pass 1 (admin) → Pass 2 (fuzz-user)
```

Fuzzes the 20 security-fuzzable API endpoints against the running dev stack with Schemathesis in two passes (design in `.scratch/archive/security-fuzzing/spec.md`):

- **Pass 1 (admin session)** — asserts schema-valid 200s (or declared 4xx) and never a 5xx under malformed/extreme input.
- **Pass 2 (fuzz-user session)** — a self-provisioned low-privilege user; the custom `admin_gated_boundary` check fails any `admin-gated` operation returning 2xx (the RBAC boundary; 403/404 pass).

Mechanics:

- Runs via the `ow-schemathesis` Docker image (built from `apps/api/scripts/schemathesis.Dockerfile`; the official `schemathesis:stable` bundles a broken `tracecov` plugin that crashes `run`) — no host Python / pipx.
- The API is fuzzed directly at `http://localhost:3000` (the proxy's API router can't reach the health path).
- The spec is regenerated from the running code each run (`cargo run --bin export_openapi`), asserted to cover exactly 20 paths, and the seed is fixed (default `20260101`, env-overridable) so failures are reproducible.
- Runtime hardening already landed from this work: the login request documents a malformed-input response, and Schemathesis's unexpected-method probing is disabled (`[phases.coverage] unexpected-methods = []` in `schemathesis.toml`) so the exported spec is the only fuzz surface.

## Zero-Warning Policy

`apps/api/` must compile with **zero warnings** (default features **and** the `docker` feature gate). Suppression attributes (`#[allow(dead_code)]`, `#[allow(unused)]`, …) are forbidden — fix the root cause.

The test harness (`tests/common/mod.rs`) is compiled independently by each integration-test binary, so items unused by any *single* binary trigger `dead_code`. Fix patterns:

1. Split shared code into focused submodules (e.g. `common/pg.rs`) so binaries that only need `ensure_pg` don't compile `TestContext`.
2. Use `#[path]` when a test file needs one submodule without its parent.
3. Move single-use helpers into the test file that uses them.
4. Convert `ctx.client.get(…)` to `ctx.get(…)` so helper methods are exercised across files.
5. Add a `test_context_helpers` test in each binary that calls every `TestContext` method at least once.

## Allowing Users to `apt` Install Packages

To let instance users install packages interactively (e.g. inside the ttyd/Jupyter/desktop session), the instance OS user needs passwordless `sudo` for the apt binaries. The documented sudoers entry is:

```bash
# /etc/sudoers.d/ow-apt (or appended to the image's sudoers)
ow_user ALL=(ALL) NOPASSWD: /usr/bin/apt, /usr/bin/apt-get
```

This is image-level configuration — see `docker/template_images/` for where the base images bake it in.

## Production Deploy

```bash
pnpm run init                                   # ow-network + runsc runtime
pnpm run docker:up                              # build template images + compose up -d --build
pnpm run docker:down                            # stop
pnpm run docker:remove                          # down -v + cleanup.sh
```

The production stack (`docker/openworkspace/docker-compose.yml`) adds `api` and `web` containers:

| Service | Image | Ports | Notes |
|---------|-------|-------|-------|
| `traefik` | traefik:v3.7.4 | `80`, `127.0.0.1:8080` | **File provider only** (no Docker socket); dynamic dir mounted read-only |
| `api` | `ow-api` (built) | — | Runs as **root**, `pid: host`, `cap_add: [SYS_ADMIN, NET_ADMIN]`, `apparmor=unconfined`, rw Docker socket |
| `web` | `ow-web` (built) | — | SvelteKit static build served by nginx |
| `postgresql` | postgres:18-alpine | — | Volume `server-pgdata` |

Production Traefik routes to services by compose network name (`ow-api:3000`, `ow-web:80`) instead of `host.docker.internal`. The API container writes routes into `./traefik/dynamic` (its `TRAEFIK_DYNAMIC_DIR`, mounted rw into the api container, ro into traefik at `/etc/traefik/dynamic`). Per-instance route files (`*-ws.yml`) are gitignored.

Deploy flow: `docker compose -f docker/openworkspace/docker-compose.yml up -d --build`. Postgres data lives in `server-pgdata`; `ow-network` is created by `scripts/docker-network.sh`.
