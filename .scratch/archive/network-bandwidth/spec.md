Status: completed

# Container Network Bandwidth Limiting for Workspace Instances

## Problem Statement

Workspace instances run untrusted developer workloads on a shared host. Today each instance container is only constrained by CPU (`cores`) and memory (`memory`); there is no network-level limit. A single instance can saturate the host's link — uploading or downloading at line rate — degrading every other instance and the services (Traefik/nginx, Postgres, the API) sharing the box.

The platform already drops `NET_RAW`/`NET_ADMIN` inside instance containers to prevent packet-level attacks, but socket-level bandwidth exhaustion (connection floods, bulk transfers) is out of reach of capability drops. Without a per-instance bandwidth cap, there is no way to keep one workload from starving the others.

## Solution

Add two per-template bandwidth settings — upload and download, each in Mbps — persisted in the database, exposed through the template API and the template form, and enforced with Linux `tc`/HTB at the kernel level on the instance's veth pair. The Rust API applies the limit with `tc` after the container starts: the upload cap shapes egress on `eth0` inside the container's network namespace, and the download cap shapes egress on the host-side veth (which is the container's ingress). A value of `0` means unlimited and skips `tc` entirely. Application happens on every start path, because Docker recreates the veth pair (and thus destroys any qdisc) on each stop/start cycle.

The API container must be given the minimum privileges to reach the host and instance network namespaces: host PID sharing plus `SYS_ADMIN`/`NET_ADMIN`.

## User Stories

1. As a **workspace manager**, I want to set a per-template upload bandwidth limit in Mbps, so that a single instance cannot saturate the host's outgoing link.
2. As a **workspace manager**, I want to set a per-template download bandwidth limit in Mbps, so that a single instance cannot consume the host's whole incoming bandwidth.
3. As a **workspace manager**, I want upload and download limits to be independent, so that I can allow fast downloads while capping uploads (and vice versa).
4. As a **workspace manager**, I want `0` to mean "unlimited", so that templates without a limit behave exactly as they do today.
5. As a **workspace manager**, I want the bandwidth fields to appear in the advanced section of the template form, so that the main form stays uncluttered.
6. As a **workspace manager**, I want both fields to default to `0` (unlimited) on new templates, so that I don't need to know about bandwidth to create a template.
7. As a **workspace manager**, I want negative bandwidth values to be rejected by both the API (400) and the form, so that typos fail loudly instead of silently changing behavior.
8. As a **workspace manager**, I want to edit a template's bandwidth limits later, so that I can adjust limits without recreating the template.
9. As an **API consumer**, I want to send `network_bandwidth_up_mbps` and `network_bandwidth_down_mbps` when creating or updating a template, so that I can manage limits programmatically.
10. As an **API consumer**, I want both fields included in every template JSON response (list and single), so that I can audit limits across all templates.
11. As a **platform developer**, I want the limit applied automatically when an instance container is created and started, so that a freshly launched instance cannot start out unlimited.
12. As a **platform developer**, I want the limit re-applied when an existing stopped container is restarted, so that Docker's veth teardown/recreate cannot leave an instance unlimited.
13. As a **platform developer**, I want a failed `tc` application to log an error and leave the container running, so that a bandwidth-shaping fault does not take down a user's desktop.
14. As a **platform developer**, I want the `tc` command construction and the host-veth discovery to be pure functions, so that they are testable without Docker or host privileges.
15. As an **infrastructure admin**, I want the API container granted only the needed capabilities (`SYS_ADMIN`, `NET_ADMIN`) plus host PID sharing, so that `tc` can reach the host and instance namespaces without a fully privileged container.
16. As an **infrastructure admin**, I want a smoke script that verifies real shaping on a live host, so that I can validate the deployment once after setup.
17. As an **instance user**, I want my desktop session to keep working under its bandwidth cap, so that the cap protects the host without breaking the VNC session.
18. As a **platform developer**, I want the limit enforced in the kernel (HTB), so that no agent or library needs to be installed inside the container image.

## Implementation Decisions

### 1. Database schema
- Two new columns on `workspace_templates`: `network_bandwidth_up_mbps INTEGER NOT NULL DEFAULT 0` and `network_bandwidth_down_mbps INTEGER NOT NULL DEFAULT 0`.
- `0` = unlimited. Negative values are rejected at the API layer (see API contract).
- Added via a new SeaORM migration following the existing per-feature migration pattern (sequence after the current latest).

### 2. Rust data model
- The public `WorkspaceTemplate` type gains `network_bandwidth_up_mbps: i64` and `network_bandwidth_down_mbps: i64`, mapped from the SeaORM entity (default `0`).
- `ContainerConfig` gains the same two `i64` fields so the create/start funnel carries them.
- Template create/update request structs gain the two fields (serde default `0`).

### 3. API contract
- `POST /api/templates` and `PUT /api/templates/{id}` accept `network_bandwidth_up_mbps` / `network_bandwidth_down_mbps` (optional, default `0`).
- `GET /api/templates` and `GET /api/templates/{id}` return both fields.
- Negative values → `400`. Very large values are accepted as-is (`0` already means unlimited, so no arbitrary cap is imposed).

### 4. `tc` application (kernel-level shaping)
- New module with pure helpers:
  - **Command builder**: given a rate in Mbps and an interface name, produce the `tc` qdisc/class arguments (`htb` root with a single default class; `rate` from the configured Mbps; burst/ceil left at kernel defaults).
  - **Host-veth discovery**: match the container `eth0` ifindex (read inside the container's netns) against host-side `iflink` values (read inside the host netns) to find the host-side veth name.
- **Upload (egress)**: run `tc` against `eth0` inside the instance container's network namespace, entered via `nsenter` targeting the container's PID (from Docker inspect).
- **Download (ingress)**: run `tc` against the host-side veth, entered via `nsenter` targeting host PID 1 (host netns).
- Only `rate` is exposed; `burst`/`ceil` are not configurable.
- `0` in either direction → skip that direction entirely (no qdisc, no overhead).

### 5. `DockerService` integration
- New trait method `apply_bandwidth_limit(container_id, up_mbps, down_mbps) -> Result<(), String>`, implemented by the real Docker client (and available on the mock).
- Called automatically inside `create_container_from_template` after the container starts (the config carries the values — a single funnel covering both new-create and recreate paths).
- Called by the start-instance route after restarting an existing stopped container (the route has the template's values; the Docker layer has no database access).
- **Failure handling (fail-open)**: on error, log via `tracing::error!` and continue — the container still runs, just without a limit. The error site carries a `TODO` for a future warning/notification mechanism.

### 6. Deployment / privileges
- The API Docker image installs `iproute2` (for `tc`) and `util-linux` (for `nsenter`) and runs as root.
- The API compose service gains: `pid: host`, `cap_add: [SYS_ADMIN, NET_ADMIN]`, and a read-write mount of the Docker socket.
- Only the production stack's compose file changes (dev runs on the user's own host); dev and personal-dev compose files are untouched.

### 7. Web frontend
- Template form advanced section gains two numeric inputs: "upload bandwidth (Mbps)" and "download bandwidth (Mbps)", default `0`.
- `TemplateFormState` and the `Template` TypeScript type gain the two fields.
- `submitTemplate`/`updateTemplate` send the fields as top-level template properties (not inside `run_config`).
- Form-side validation rejects negative values.

## Testing Decisions

### What makes a good test
- Test external behavior, not implementation details: that `apply_bandwidth_limit` is invoked with the right limits (or skipped entirely when `0`) on each start path; that the produced `tc` commands match the expected shape; that the veth matcher selects the correct interface; that the API persists and returns the fields.
- Pure logic is tested as input/output transforms; orchestration is tested through the mockable `DockerService` seam.
- Real shaping cannot run in CI (needs root + live Docker + a real host netns); it is covered by the manual smoke script instead.

### Test seams (preferred order)

| Seam | Type | What it tests |
|---|---|---|
| `DockerService` trait (mockall, route level) | Mock-based integration | `bandwidth > 0` → `apply_bandwidth_limit` called with the right `(container_id, up, down)` on launch, recreate, and restart paths; `= 0` → never called |
| New network-QoS module pure functions | Unit test | `tc` command assembly (`htb` qdisc + class, `rate` in Mbit); host-veth discovery ifindex↔iflink matching |
| Template API (existing Postgres integration pattern) | Integration | POST/PUT carry the new fields, GET/list return them, DB round-trip preserves them; negative → 400 |
| `scripts/apply_bw_smoke.sh` | Manual (not a CI seam) | On a live host, create a limited container and verify actual throughput matches the configured rate |

### Prior art
- Mock-based Docker orchestration tests: `docker_test.rs` (mockall `DockerService`, create/start/pause flows).
- Template API tests: `templates_test.rs` (create, get, list, update, delete).
- Entity `From` conversion tests: existing `Model → WorkspaceTemplate` conversion tests in the DB test module.
- Repository round-trip tests: existing template create/find/update tests against the Postgres test container.

## Out of Scope

- **Per-instance bandwidth override**: limits are template-level; instances inherit them. No per-instance UI or column.
- **`burst`/`ceil` tuning**: only `rate` is configurable; kernel defaults are used for burst.
- **Per-flow / per-IP shaping inside a container**: the whole container is one shaped pool.
- **Network egress policy / dropping outbound entirely**: blocking connectivity is a separate concern from rate limiting.
- **Ingress limiting via `ifb`**: the host-side-veth approach is chosen; the `ifb`+`mirred` in-namespace alternative is explicitly not used.
- **Bandwidth usage metrics/alerting**: a real notification mechanism is deferred and only marked as a `TODO` at the failure site.
- **Changes to dev/personal-dev compose files**: only the production stack's compose file and the API image change.
- **TC on non-instance containers**: the API's own infrastructure containers (Postgres, Traefik, nginx) are never shaped.

## Further Notes

- Docker removes the veth pair when a container stops and recreates it on start, so qdisc rules never survive a restart — re-application on every start path is mandatory, not a nicety.
- `tc` shapes at the kernel veth regardless of the container's OCI runtime: with `runsc` (gVisor), the sandbox still transmits through the kernel veth, so shaping still applies. This should be confirmed once via the smoke script.
- The API container changes (`pid: host`, `cap_add`, docker socket rw, root) make it roughly as privileged as the Docker daemon itself — this is the inherent cost of host-side shaping and should be noted in deployment docs.
- The smoke script should exercise both directions (e.g., `iperf3` or a large `curl`) and confirm measured throughput converges toward the configured rate.
