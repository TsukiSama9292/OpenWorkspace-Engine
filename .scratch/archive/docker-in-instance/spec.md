Status: complete — all 8 tickets shipped and verified in code. Networking-topology decision §1 is **superseded** by `.scratch/archive/docker-in-instance_2` (per-instance `/30` bridges + `OW_DNS` resolv.conf rewrite); the port-pool, DinI security matrix, image contract, and host-provisioning decisions remain current.

# Docker in Instances (DinI) — Host Port Pool Networking + In-Instance dockerd

## Problem Statement

The platform shares one Linux box among multiple developers via browser-based virtual desktops (KasmVNC / ttyd / Jupyter). Today each instance is a plain bridge container on `ow-network`, and Traefik routes to `https://<container_ip>:<service_port>`. That architecture has two problems:

1. **Tenants cannot run their own Docker workloads inside their workspace.** There is no in-instance Docker daemon, so developers cannot build/run containers in the browser. The platform's runtime story is also split: instances run under the Docker default runtime (`runc`) or gVisor (`runsc`), and each runtime needs a different privilege treatment for an in-instance `dockerd` to function.
2. **Instance networking couples the API to Docker internals.** Routing requires the API to discover each container's bridge IP (`get_container_ip`) after every create/start and embed it in Traefik route files. This is incidental complexity, and it makes health checks probe a different path than real traffic.

## Solution

Redesign instance networking around a **host port pool**. The API maintains an allocation pool of host ports (`10000–20000`); every instance's single service (KasmVNC `6901`, ttyd `7681`, or Jupyter `8888`) is published to the host Docker-bridge gateway IP (`172.17.0.1`, configurable), and Traefik routes to `https://host.docker.internal:<host_port>` instead of the container IP. The API never needs a container IP again.

On top of this, add an optional per-template switch — **`docker_in_instance` (DinI)** — which runs an in-instance `dockerd` so tenants can run their own containers. When enabled, the API grants the instance the privilege its runtime needs (`--privileged` for both `runsc` and `runc`; safe under gVisor, high-risk under `runc`), mounts a `tmpfs` at `/var/lib/docker`, and sets `OW_DOCKER_IN_INSTANCE=true`; the instance image's entrypoint starts `dockerd` with gVisor-compatible flags. Docker-in-instance images are built and version-controlled in this repo.

## User Stories

1. As a **tenant**, I want to run my own Docker containers inside my workspace instance, so that I can develop, build, and test services without leaving the browser.
2. As a **tenant**, I want Docker-in-instance to work under both the default (`runc`) and gVisor (`runsc`) runtimes, so that I can choose between maximum sandboxing and native performance.
3. As a **tenant**, I want my instance's KasmVNC / ttyd / Jupyter service to be reachable through the same platform URL as before, so that the networking redesign is invisible to me.
4. As a **tenant**, I want to access a nested container's service by opening `http://localhost:<port>` in my instance's desktop/terminal, so that I can iterate on the apps I run.
5. As a **tenant**, I want my nested containers' data to survive an instance restart when I bind-mount paths under my persistent home directory, so that only the data I choose is retained.
6. As a **tenant**, I want my instance to keep the same host port across stop/start cycles, so that my experience (and any bookmarked URL) stays stable.
7. As a **workspace manager**, I want a single per-template **"Enable Docker in Instance (DinI)"** toggle, so that a template either grants in-instance Docker or keeps today's hardened defaults.
8. As a **workspace manager**, I want the template form to show a clear safety indicator when DinI is enabled, so that I understand the security posture: sandboxed-safe for `runsc`, high-risk warning for `runc`.
9. As a **workspace manager**, I want `docker_in_instance` to be a first-class template property (create, edit, read), so that I can manage it through the UI and API alike.
10. As a **workspace manager**, I want a template with DinI disabled to keep the current default instance security (non-privileged, dropped `NET_RAW`/`NET_ADMIN`), so that the hardening we already have is never silently weakened.
11. As an **API consumer**, I want the instance JSON to expose the allocated `host_port`, so that I can debug connectivity by curling `172.17.0.1:<host_port>` directly.
12. As an **infrastructure admin**, I want the host gateway IP configurable via an environment variable (`OW_HOST_GATEWAY_IP`, default `172.17.0.1`), so that non-standard Docker hosts (custom `bip`, CNI, etc.) work without recompiling the API.
13. As an **infrastructure admin**, I want the host port pool range configurable via environment variables, so that I can avoid conflicts with other host services.
14. As an **infrastructure admin**, I want a repeatable, idempotent host-provisioning script that installs `runsc` and registers it in `/etc/docker/daemon.json`, so that gVisor-backed DinI instances work on any machine with one command.
15. As an **infrastructure admin**, I want the API to never read container IPs, so that its only networking duty is maintaining the host port pool.
16. As an **infrastructure admin**, I want a host port to be freed only when the instance is deleted, so that stopped instances never leak their port to another tenant (avoiding cross-tenant route collisions).
17. As an **infrastructure admin**, I want a smoke-test script that proves DinI works end-to-end on both runtimes, so that a host upgrade or image change cannot silently break in-instance Docker.
18. As a **platform developer**, I want port allocation to be a pure function, so that the allocation algorithm is unit-testable without Docker or a database.
19. As a **platform developer**, I want the runtime × DinI → HostConfig mapping to be a pure function, so that the security matrix is unit-testable and documented in code.
20. As a **platform developer**, I want `get_container_ip` removed from the codebase entirely (zero-warning policy), so that no dead code remains after routing and health checks stop using container IPs.
21. As a **platform developer**, I want health checks to probe the exact path real traffic uses (`https://<host gateway IP>:<host_port>/`), so that "health says OK" implies "Traefik can reach it".
22. As a **platform developer**, I want the Docker-in-instance image contract (env var name, tmpfs mount, `dockerd` flags) owned by this repo's image builds, so that the API and image cannot drift out of sync.

## Implementation Decisions

### 1. Network topology (global, all instances)

> **SUPERSEDED — replaced by `.scratch/archive/docker-in-instance_2` (per-instance `/30` bridges).**
> This decision shipped, then was reversed. Sharing the default `bridge` left every
> instance (and, under DinI, every nested `--network=host` service) on the shared
> `docker0` subnet, directly reachable by every other tenant — a cross-tenant
> isolation hole. The successor spec gives each instance its own dedicated `/30`
> bridge (`ow-<instance-id>`, gateway `.1`, instance `.2`) and fixes the runsc DNS
> break this section worked around by having the instance images rewrite
> `/etc/resolv.conf` from an `OW_DNS` env var. The port-pool / Traefik / `get_container_ip`
> bullets below are **still current**; the "join the default bridge" bullets and the
> rationale are historical only.

All instances use the host-port-binding model. **As originally shipped**, the instance container joined the Docker default `bridge` network (`network_mode = "bridge"`), **not** `ow-network`; under the successor spec the container's `network_mode` is its per-instance network name instead (`instance_net::network_name`, i.e. `ow-<instance-id>`). The compose stack (`api`, `web`, `traefik`, `postgres`) keeps using `ow-network`; instances are deliberately excluded from it in both generations.

- Its single service port (`RemoteType::port()`: KasmVNC `6901`, ttyd `7681`, Jupyter `8888`) is published to **`<host_gateway_ip>:<host_port>`** via Docker port bindings (`exposed_ports` + `port_bindings`). *(still current)*
- Traefik route files target **`https://host.docker.internal:<host_port>`** with the existing `kasm-insecure` serversTransport. No scheme/transport branching is introduced. *(still current)*
- `get_container_ip` is removed from the Docker service trait, its implementation, call sites, mocks, and feature-gated tests. *(still current)*

*Historical rationale (why the default `bridge` was chosen at the time; obsolete under the successor):* on a user-defined bridge, Docker injects its embedded resolver (`nameserver 127.0.0.11`) into every container's `resolv.conf`. Inside a `runsc` sandbox that resolver does not bind, so **all name resolution fails in the instance** — including for the in-instance `dockerd`, which inherits the instance's `resolv.conf` (observed live: nested `docker run hello-world` fails `lookup registry-1.docker.io on 127.0.0.11:53: connection refused`, and even `curl` to a public host fails in the instance). The default `bridge` network carries the host's real upstream DNS servers, which are reachable from the instance and its nested daemon; the DinI smoke test (which runs on `bridge`) proves the pull path works. The successor approach fixes the same root cause differently — rewriting `/etc/resolv.conf` in-image from `OW_DNS` — which is what makes per-instance user-defined bridges viable again.

Why the host gateway IP rather than loopback *(still current)*: Traefik runs in a container, so a strictly `127.0.0.1`-bound port is unreachable through `host.docker.internal`. Binding on the Docker bridge gateway (default `172.17.0.1`) is reachable via `host.docker.internal:host-gateway` while staying off the LAN.

### 2. Host gateway IP setting

- New setting `OW_HOST_GATEWAY_IP`, default `"172.17.0.1"`, parsed with the existing settings pattern. Used both for the Docker port-binding `HostIp` and as the health-check probe host.
- Not discovered at runtime; no extra Docker calls (keeps the API purely port-pool-driven). Non-standard hosts override via env.

### 3. Host port pool

- **Granularity:** one host port per instance, mapped to that instance's single `remote_type` container port. (An instance runs exactly one service, so one port is always enough.)
- **Range:** `[OW_HOST_PORT_START, OW_HOST_PORT_END)` — new settings, defaults `10000` and `20000` (10,000 usable ports).
- **Storage:** new nullable `host_port` column on `workspace_instances` with a **UNIQUE index**. The database is the concurrency arbiter (two concurrent launches cannot both win the same port).
- **Allocation:** at first launch, query the set of host ports in use by non-deleted instances, then pick the lowest free port in the range. Before committing, best-effort TCP probe `connect(gateway_ip, port)`; if something is already listening, skip it.
- **Lifecycle:** allocated at first launch, persisted, reused across stop/start, **freed only on delete** (row removal releases the unique slot). A broken `error` instance keeps its reserved port until it is deleted/replaced.
- **Failure handling:** if container creation fails with a port-conflict error, the launch retries up to 5 times; each retry scans the pool *circularly* from an offset derived from the instance's access token, so concurrent launches don't all re-pick the same lowest free port and re-collide. Docker's bind at create time is the final arbiter.
- **Backfill:** a pre-existing instance row with `host_port = NULL` is allocated a port on next `start` (same pattern as the existing persistent-path backfill), before the container is created/started.
- **Start-path defense:** if restarting a stopped container fails because a concurrent launch bound its freed host port, the start route drops the stale container and recreates it on a fresh port (same bounded retry). In normal operation the DB keeps the port reserved across stop/start so this cannot happen; it guards the shared-Docker test harness (per-test DBs cannot see each other's reservations) and any out-of-band port theft.

### 4. Security matrix (`docker_in_instance`)

A single per-template boolean `docker_in_instance` (default `false`) drives all privilege changes; no extra escape-hatch toggle.

| DinI | container_runtime | Instance container config | Host risk |
|---|---|---|---|
| `false` | any | unchanged: `privileged=false`, `cap_drop=[NET_RAW, NET_ADMIN]`, full seccomp/apparmor | safe |
| `true` | `runsc` | `privileged=true` (in-sandbox perceived capabilities only; gVisor never grants host caps) | safe |
| `true` | `runc` | full `privileged=true` (seccomp/apparmor/masked paths lifted) | **high** — UI must show a red warning |

Rationale for full `--privileged` on `runc` rather than a curated capability list: a rootful `dockerd` under stock `runc` is still blocked by masked paths (`/sys/fs/cgroup`, `/proc`), seccomp, and apparmor even with `CAP_SYS_ADMIN`; without rootless mode, full privilege is the pragmatic path. The risk is accepted with an explicit UI warning, and the safe default recommendation is DinI + `runsc`.

### 5. Docker-in-instance provisioning contract (API ↔ image)

When `docker_in_instance == true`, the API's container-creation path additionally applies:

- `privileged = true`
- no `cap_drop`
- `tmpfs` mount at `/var/lib/docker` with options `exec,mode=755` (the `exec` option is required — without it `noexec` blocks `dockerd`)
- environment variable `OW_DOCKER_IN_INSTANCE=true`

The image entrypoint owns `dockerd` lifecycle:

- If `OW_DOCKER_IN_INSTANCE == true`: start `dockerd --iptables=false --ip6tables=false --data-root=/var/lib/docker` in the background (logs to `/var/log/dockerd.log`), poll readiness via `docker info` (15 s timeout; on timeout print the log and exit non-zero), then continue to the main service (KasmVNC / ttyd / Jupyter) via `exec`.
- The contract is uniform across both runtimes: `--iptables=false` is a gVisor hard requirement and is kept uniform for `runc` too, so nested containers expose ports via `--network=host` in both cases.

The Dockerfiles and entrypoint scripts for the DinI image variants are built in this repo (new `docker/` directory). The template default image values are updated to the in-repo image names, and the deploy flow gains a build step for these images.

### 6. Host provisioning (gVisor)

New repo script `scripts/docker-runtime-gvisor.sh`, wired into `pnpm run init` alongside the existing network provisioning:

- Idempotent: skips already-satisfied steps.
- Installs `runsc` to `/usr/local/bin/runsc` if missing (downloads the official release for the host architecture).
- Merges (does not overwrite) a `runsc` entry with `runtimeArgs: ["--net-raw", "--allow-packet-socket-write"]` into `/etc/docker/daemon.json` (JSON merge, backs up to `daemon.json.bak`). `--allow-packet-socket-write` covers Docker v28+; the tmpfs `/var/lib/docker` approach already covers the v29+ containerd-snapshotter storage requirement.
- Reloads/restarts the Docker daemon to apply the change.

### 7. Traefik routes

- Route-writer output changes only its target: `https://host.docker.internal:<host_port>` (replacing `https://<container_ip>:<port>`), keeping `kasm-insecure`, auth headers, and strip-prefix middlewares exactly as today.
- The route-writer call sites pass the instance's allocated `host_port` instead of a container IP; no other routing behavior changes.
- Prod `traefik` service gains `extra_hosts: ["host.docker.internal:host-gateway"]` (the dev Traefik already has it). The `api` service needs no `extra_hosts` — it connects to the gateway by IP, not hostname.

### 8. Health checks & dead code

- The health worker probes `https://{OW_HOST_GATEWAY_IP}:{host_port}/` (from the API container), which is path-identical to Traefik's route target.
- `get_container_ip` is removed from the `DockerService` trait, the real client, the launch/start route paths, the health worker, mock expectations, and the feature-gated Docker integration tests, so the zero-warning policy is preserved.

### 9. Persistence boundary

- `/var/lib/docker` is always `tmpfs` (volatile), even for persistent instances. Nested Docker images/containers/volumes do not survive instance restart or host reboot. This is the intended design: it satisfies gVisor's storage requirement, avoids overlay-on-overlay on the host's overlay2, and keeps a single uniform storage path.
- The persistent volume remains the workspace home only. Tenants who need nested data to survive restart bind-mount paths under their persistent home into nested containers.

### 10. Nested service exposure (known limitation)

- Because `dockerd` runs with `--iptables=false --ip6tables=false`, nested `docker run -p` publishing does not function on either runtime. Nested containers must use `--network=host`.
- Nested services then bind inside the instance container's network namespace (its dedicated `/30` IP `10.200.x.2` under the successor topology; was the default-`bridge` IP under the superseded v1 topology): reachable from the instance's own localhost (KasmVNC desktop browser, ttyd) — but **not** through the host port pool / Traefik / the tenant's browser via platform URLs. This is an accepted boundary, deliberately kept out of scope (see Out of Scope).

### 11. Database schema & API contract

- `workspace_templates.docker_in_instance` — `BOOLEAN NOT NULL DEFAULT false` (new migration).
- `workspace_instances.host_port` — nullable integer with a UNIQUE index (new migration).
- Template API (`POST`/`PUT`/`GET /api/templates…`) accepts and returns `docker_in_instance`.
- Instance API responses include `host_port` (number or `null`).

## Testing Decisions

What makes a good test: assert the **external behavior** (what lands in `HostConfig`, what the route YAML says, what the health worker probes, what the API returns) rather than internal plumbing. Prefer the highest existing seam; the primary behavioral seam is the mocked `DockerService` integration tests, mirroring how the container-runtime and bandwidth features were tested.

### Seam 1 — Pure logic unit tests (in `apps/api/src/`)

- Port allocation: given a used-port set and range, returns the lowest free port; returns `None` when the pool is exhausted. Also covers the port-conflict retry selection.
- HostConfig security builder: `(docker_in_instance, runtime) → {privileged, cap_drop, tmpfs, dind_env}` covers all three matrix rows (`false` → current hardened config; `true`+`runsc` → privileged; `true`+`runc` → privileged), and the tmpfs string (`exec,mode=755`) + `OW_DOCKER_IN_INSTANCE=true` env. Extends the existing `runtime_to_host_config` pure-function seam.
- Route writer: generated YAML targets `https://host.docker.internal:<host_port>` with `kasm-insecure` for all three remote types; auth/strip middleware content preserved.

### Seam 2 — Mocked `DockerService` integration tests

- Launch/start lifecycle: `host_port` is allocated, persisted, and passed into the container config and route write; `host_port` is stable across stop/start; delete frees it; error-state keeps it.
- Health worker: probes `https://<OW_HOST_GATEWAY_IP>:<host_port>/` and never calls `get_container_ip` (the mock expectation is gone).
- Template API: `docker_in_instance` round-trips through create/read/update.

### Seam 3 — Feature-gated real-Docker integration tests

- `port_bindings` reach `172.17.0.1:<host_port>` on the created container; `privileged`, the `/var/lib/docker` tmpfs, and `OW_DOCKER_IN_INSTANCE=true` are present when DinI is on and absent when off; `runsc` runtime pass-through is unchanged.

### Seam 4 — Database tests

- `workspace_instances.host_port` column, UNIQUE index enforcement (duplicate insert fails), default-NULL for legacy rows.
- `workspace_templates.docker_in_instance` default `false`, persisted and read back.
- Entity `From` conversions include both new fields.

### Seam 5 — Frontend vitest

- Template form: the DinI toggle serializes into the API payload and deserializes back on edit; the runC+DinI high-risk warning state and runsc+DinI sandbox indicator are driven correctly.

### Prior art

- `container-runtime` spec: pure `runtime_to_host_config` function, settings env-var tests, entity `From` conversion tests, template API round-trip tests.
- `network-bandwidth` spec: mocked-`DockerService` integration tests and a host smoke-test script (`apply_bw_smoke.sh`) — the DinI smoke test follows the same shape.
- Existing route-writer inline unit tests; existing health-worker mock tests; existing `instances_mock_test.rs` lifecycle tests.

## Out of Scope

- **Nested port forwarding** (watching nested containers, allocating host ports for them, registering Traefik routes for nested services). Nested services stay reachable only via the instance's own localhost.
- **Rootless `dockerd`** in instances.
- **Nested Docker data persistence** (`vfs` storage driver, or relocating `/var/lib/docker` off tmpfs). Nested state is ephemeral by design; tenants persist via home bind-mounts.
- **Manual host-port reservation or per-instance override** of the pool.
- **Pre-validating runsc registration** on the host before container create (Docker rejects at create if missing, as today).
- **Migrating already-running legacy instances** to the new topology beyond the `host_port` backfill on next start.
- **GPU + DinI interplay** is not addressed; GPU device requests are orthogonal and unchanged.

## Further Notes

- Health-check probing and Traefik routing now share an identical path (`host-gateway` → published port), which removes the class of bug where "health says OK but Traefik can't reach".
- Binding on `172.17.0.1` (the default-bridge gateway) keeps published ports off the LAN. Because instances no longer share `ow-network`, they lose direct container-IP peer access to one another; cross-instance traffic flows only through published ports, which is acceptable and more isolated.
- Port binding under `runsc` and the tmpfs-backed `dockerd` behavior must be confirmed once by the smoke script (the repo precedent is `apply_bw_smoke.sh`).
- New smoke script `scripts/dini_smoke_test.sh` verifies, on both runtimes: (1) `dockerd` readiness within 15 s via `OW_DOCKER_IN_INSTANCE=true`; (2) a nested `--network=host` service (e.g. `nginx:alpine`) reachable at `localhost` inside the instance; (3) a nested container bind-mounting the persistent home writes through to the host volume.
