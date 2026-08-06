Status: complete — all 8 tickets shipped and live-verified on this host. `docker_test` binary 37/37 green with 0 skips (incl. `test_runsc_dns_rewrite_in_instance`, now un-dormant after the template-image rebuild), and the ticket-08 host smoke (`scripts/network_isolation_smoke_test.sh`) passes 11/11 on both `runc` and `runsc` with no leftover bridges.

# Per-Instance Isolated Networking — Dedicated /30 Bridges

## Problem Statement

The platform shares one Linux box among multiple developers via browser-based virtual desktops (KasmVNC / ttyd / Jupyter), optionally with a Docker daemon inside the instance (DinI). Instances currently run on the Docker **default `bridge`** network, which means every instance's container shares `docker0` and the same broadcast domain as every other instance — and every other container on the host.

That is a cross-tenant reachability hole. **Verified live:** two containers on the default `bridge` ping each other freely in both directions. Under DinI, nested `--network=host` containers bind their ports inside the instance's network namespace, so their services land on the shared `docker0` subnet and are directly reachable by *every other tenant* at their instance's docker0 IP. A tenant can scan the shared `docker0` range and connect to any other tenant's workspace services (VNC, ttyd, Jupyter, or nested apps) without authentication.

The previous fix — moving instances to the default `bridge` — was chosen because user-defined bridges break DNS under the `runsc` runtime (Docker injects its embedded resolver `nameserver 127.0.0.11`, which does not bind inside a gVisor sandbox, so nothing resolves in the instance, including nested `dockerd` pulls). But the default `bridge` is precisely the network that cannot isolate tenants from each other.

## Solution

Give **every instance its own dedicated Docker bridge network** with a `/30` subnet — the smallest a Docker bridge supports. Each instance is the *only* container on its network, with exactly one usable IP (`x.x.x.2`, gateway `.1`). Docker's built-in network isolation rules then make cross-instance traffic impossible at L3 (verified live: two separate `/30` networks are mutually unreachable — no ping either direction — while internet access via Docker NAT still works). Nothing about this touches the host: `docker network create` is a standard Docker-API call, and Docker wires up the bridge interface and iptables rules itself.

Because any user-defined bridge breaks DNS under `runsc`, the instance images rewrite `/etc/resolv.conf` to a real resolver at entrypoint time, driven by a new API setting (`OW_INSTANCE_DNS`, default public resolvers). This restores name resolution for desktop apps and nested `dockerd` alike (verified live under `runsc`: after the rewrite, in-instance `curl` resolves and a nested `docker pull` succeeds).

The host-port-pool model is unchanged: each instance's single outer service port stays published on the host and Traefik keeps routing to `https://host.docker.internal:<host_port>`. The `/30` secures everything *behind* that port — tenant and nested services bind on the instance's unique IP and are unreachable from other tenants.

The default `bridge` decision is therefore **replaced** by per-instance `/30` bridges for **all** instances (DinI and non-DinI), with a single code path and no host configuration changes.

## User Stories

1. As a **tenant**, I want other tenants' instances to be unreachable from my instance, so that nobody else on the box can scan or connect to my workspace.
2. As a **tenant**, I want my instance's services to remain reachable at the same platform URLs after the change, so that the isolation work is invisible to me.
3. As a **tenant**, I want my instance to be isolated even when DinI is disabled, so that my desktop itself is not a peer of other tenants' desktops on a shared network.
4. As a **tenant**, I want the services I run inside my instance (nested `--network=host` containers) to bind only on my instance's unique IP, so that other tenants cannot probe the apps I develop.
5. As a **tenant**, I want DNS to keep working inside my instance — both desktop apps and nested `docker pull` — so that the isolated networking does not break name resolution under any runtime.
6. As a **tenant**, I want my instance's networking to be stable across stop/start and restart, so that restarting my workspace never changes my addresses or drops my connectivity.
7. As a **tenant**, I want my instance to keep exactly one usable IP on its own subnet, so that there is no spare address on my network for anyone else to occupy.
8. As a **workspace manager**, I want the API to create a dedicated isolated network automatically when an instance is launched, so that I never configure networking per template or per instance.
9. As a **workspace manager**, I want the instance's isolated network to be removed when the instance is deleted, so that the host does not accumulate bridge networks.
10. As a **workspace manager**, I want the isolated network to be recreated idempotently if it is ever missing (e.g. a pre-existing instance started before this feature), so that backfills need no manual steps.
11. As a **workspace manager**, I want the isolated subnet pool to be configurable, so that it can be moved away from any private ranges I already use.
12. As a **workspace manager**, I want the instance DNS resolvers to be configurable, so that instances can resolve via my internal DNS if public resolvers are blocked by policy.
13. As an **infrastructure admin**, I want tenant isolation delivered without modifying the host, so that the host stays in stock Docker state with no daemon.json/iptables/route edits.
14. As an **infrastructure admin**, I want the unique per-instance IP to be discoverable from the API response, so that I can debug connectivity by curling `10.200.x.2` directly from the host.
15. As an **infrastructure admin**, I want a smoke test that proves two instances are isolated and DNS works, on both runtimes, so that a host or image change cannot silently reintroduce cross-tenant reachability.
16. As a **platform developer**, I want subnet allocation to be a pure function, so that the allocation algorithm is unit-testable without Docker or a database.
17. As a **platform developer**, I want network creation/removal to be idempotent, so that launch retries, stop/start, and error recovery never leak or corrupt networks.
18. As a **platform developer**, I want a single code path for DinI and non-DinI instances, so that the network logic has no runtime branching.
19. As a **platform developer**, I want the DNS-rewrite contract (env var name, resolv.conf format) owned by this repo's image entrypoints for both image variants, so that the API and images cannot drift apart.
20. As a **platform developer**, I want the existing `tc`/HTB bandwidth limiting to keep working on the new networks, so that per-instance bandwidth caps are unaffected by the topology change.
21. As a **platform developer**, I want the zero-warning policy preserved, so that removing the default-bridge path and adding the network seam leaves no dead code or suppressions.

## Implementation Decisions

### 1. Network topology — per-instance `/30` bridges (global, all instances)

The default-bridge decision is **replaced**. Every instance container joins its own dedicated user-defined bridge network:

- **Subnet:** `/30` — the smallest a Docker bridge accepts (network `.0`, gateway `.1`, instance `.2`, broadcast `.3`). The instance is the only endpoint: one usable IP, no room for any other tenant.
- **Driver:** `bridge` (Docker NAT egress). Verified: Docker accepts `--subnet <n>.0/30 --gateway <n>.1` and assigns the container exactly `.2`.
- **Isolation:** Docker's own bridge isolation (per-network iptables FORWARD drop) makes instances mutually unreachable at L3, while each retains internet via Docker NAT. Verified live: two separate `/30` networks cannot ping each other in either direction, and both reach `8.8.8.8`.
- **All instances:** DinI and non-DinI alike — one code path, no branching on `docker_in_instance`.

Why `/30` rather than `/32`: a bridge network needs a gateway inside the subnet, so `/32` cannot host a Docker bridge; `/30` is the true minimum and still yields exactly one instance IP.

### 2. Network name & identity

- The network is named deterministically from the instance's stable identifier (the same identity that keys the container name), e.g. `ow-<instance-id>`.
- The name is a pure derivation: no state is needed to recompute it, which makes create idempotent and lets any restart reconstruct a missing network.

### 3. Subnet allocation — lowest free from a base range

A new pure allocator module mirrors the existing host-port allocator:

- **Base range:** configurable `OW_INSTANCE_NET_BASE` (default `10.200.0.0/16`), chosen to be clear of the live ranges verified on the host (docker0 `172.17.0.0/16`, LAN `10.122.78.0/24`, wireguard `10.0.255.0/24`, tailscale `100.64.0.0/10`). A `/16` base yields 16,384 possible `/30`s.
- **Algorithm:** pure function — given the set of subnets already in use and the base CIDR, return the lowest free `/30`. The in-use set is built by listing existing Docker networks' subnets via a new `list_networks` seam on the Docker service.
- **Conflict handling:** Docker's create returns a subnet-conflict error if a candidate is taken by something outside the scan; the launch retries the next candidate (bounded, mirroring the host-port retry pattern).

### 4. Network lifecycle — API-owned, idempotent

- **Create:** before every launch/start of an instance, `create_network` is called idempotently (ignore "already exists"). This covers first launch, restart of a stopped instance, launch retries after a port conflict, and the backfill of pre-existing instances on their next start — no explicit backfill pass needed.
- **Reuse:** stop/start and error-recovery flows reuse the same network; the container is (re)created attached to it.
- **Remove:** on instance delete, after the instance container is removed, `remove_network` is called and ignores "not found". Stopped/error instances keep their network until deleted.
- **No startup reconciliation** of orphaned networks for v1 (a crash between create and delete is the only leak window; out of scope).

### 5. Container attachment

- The container's `network_mode` is the instance network's name (replacing the current `"bridge"`), so the container is born on its own `/30` and Docker assigns it the single usable IP.
- The route orchestration order is: allocate host port → ensure instance network → create container (attached to that network) → start. The port-conflict retry recreates the container against the same already-existing network.

### 6. DNS under `runsc` — resolv.conf rewrite contract

User-defined bridges force Docker's embedded resolver (`nameserver 127.0.0.11`) into the container's `resolv.conf`; under `runsc` that resolver does not bind, so nothing resolves. **Verified:** passing `--dns 8.8.8.8` does NOT fix it (Docker keeps `127.0.0.11`), but rewriting `/etc/resolv.conf` in-container does.

- **Setting:** new `OW_INSTANCE_DNS` (comma-separated nameservers), default `8.8.8.8,1.1.1.1`, following the existing settings pattern.
- **API → image contract:** the API sets the container env var `OW_DNS` (same value) on **every** instance, DinI or not.
- **Image behavior:** the entrypoints of both in-repo image variants (`kasmvnc` and `kasmvnc-dini`) rewrite `/etc/resolv.conf` from `OW_DNS` before starting their services. This is a one-way contract change owned by the images; the API only passes the value.
- **Why both variants:** the runsc DNS break applies to any user-defined bridge, so non-DinI desktops need the rewrite too (their apps resolve).
- Public-DNS default is acceptable because the setting is configurable for environments with private resolvers.

### 7. Port model — unchanged

- Each instance keeps exactly one host port from the pool, published to `OW_HOST_GATEWAY_IP` (default `172.17.0.1`); Traefik routes to `https://host.docker.internal:<host_port>`.
- Publishing to the container's IP (`10.200.x.2`) is **not** possible — docker-proxy can only bind host interfaces — so the published host port remains the single controlled cross-network ingress.
- The `/30` change secures everything else: tenant and nested `--network=host` services now bind inside the instance netns on its unique `/30` IP, reachable only from the host (admin) and from the instance itself, never from other tenants. Previously these landed on the shared `docker0` and were peer-reachable.

### 8. Security posture

- **Isolation proof:** verified live that two `/30` networks are mutually unreachable while each keeps internet; default-bridge peer-reachability was the flaw being fixed.
- **Nested services** (`--network=host`, since nested `dockerd` runs `--iptables=false --ip6tables=false`) bind on `10.200.x.2:<port>`, host-reachable only. No new host-exposed surface.
- The outer published port's exposure is unchanged from today (bound on the configured gateway IP, off the LAN by default).

### 9. Bandwidth QoS — unaffected

`apply_bandwidth_limit` matches the host-side veth by the container `eth0`'s peer ifindex via `nsenter`; it is network-type agnostic and works identically on a `/30` bridge. No changes.

### 10. API surface & data

- **No database schema change.** The network name derives from the instance id and the subnet lives in Docker's network object; nothing needs persisting (unlike `host_port`, which the DB owns).
- The instance API response may expose the instance's allocated subnet/IP (e.g. from `inspect_network` / container inspect) for operator debugging.

### 11. Traefik routes — unchanged target, deployment note

- Route files keep targeting `https://host.docker.internal:<host_port>`; no route-writer change.
- **Deployment note (carried from grilling):** Traefik's `host.docker.internal:host-gateway` resolves to the gateway of Traefik's own network, which may not be the gateway the published port is bound to (`172.17.0.1` default). The deploy step must verify `OW_HOST_GATEWAY_IP` matches an IP Traefik can actually reach (or bind `0.0.0.0`). This concern pre-exists the `/30` change and is orthogonal to it.

## Testing Decisions

What makes a good test: assert **external behavior** — which network-create/network-remove calls land on the Docker seam, what subnet the created container's IP belongs to, whether cross-network ping succeeds or fails, what `resolv.conf` contains, what env the container receives — rather than internal plumbing. Prefer the highest existing seam; the primary behavioral seam is the mocked `DockerService` integration tests, mirroring the container-runtime, network-bandwidth, and DinI specs.

### Seam 1 — Pure logic unit tests (in `apps/api/src/`)

- **Subnet allocator** (new module): given a used-subnet set and base CIDR, returns the lowest free `/30`; skips used blocks in order; returns `None` when exhausted; boundary cases (`/30` at the very top of the base range); base-range parsing/validation.
- **Network-name derivation**: deterministic, stable across calls.
- **Settings**: `OW_INSTANCE_NET_BASE` and `OW_INSTANCE_DNS` defaults and env overrides (extends the existing settings unit-test pattern).
- **`port_bindings_for`**: unchanged behavior re-asserted (existing tests keep passing).

### Seam 2 — Mocked `DockerService` integration tests (primary)

- **Launch:** the instance network is created (name derived from instance id, `/30` subnet from the allocator) before the container; the container config carries the network name as its `network_mode`; `OW_DNS` env is set on the container.
- **Stop/start:** the network is reused (create is idempotent — "already exists" is not an error); restart attaches to the same network.
- **Delete:** after container removal, `remove_network` is called for the instance network and "not found" is tolerated.
- **Retry:** a port-conflict retry recreates the container against the same existing network without re-allocating a subnet.
- **Backfill:** a pre-existing instance without a network gets one created on next start.
- **Helper containers** (persistent-volume `alpine` helpers, test harness) are unaffected and stay on the default bridge.

### Seam 3 — Feature-gated real-Docker integration tests (`docker` feature)

- Creating a network with a `/30` subnet and attaching a container yields exactly the unique IP `.2` (gateway `.1`).
- Two `/30` networks with one container each are mutually unreachable (ping fails both directions) while each reaches the internet.
- The resolv.conf rewrite path is exercised live under `runsc`: after writing `OW_DNS` resolvers, in-instance resolution works (and nested `docker pull` resolves when DinI is on).

### Seam 4 — Host smoke test

A new smoke script (mirroring `dini_smoke_test.sh` and `apply_bw_smoke.sh`) that, on both runtimes: creates two isolated instances, proves they cannot reach each other, proves both reach the internet and resolve DNS, and confirms each got its own unique `/30` IP.

### Seam 5 — No DB seam

No new columns or migrations; no entity `From` conversion changes. (If the API exposes the subnet/IP for debugging, assert it via the mocked-`DockerService` tests in Seam 2.)

### Prior art

- `network-bandwidth` spec: pure `tc`-builder functions plus a mocked-`DockerService` smoke seam.
- `container-runtime` spec: pure `runtime_to_host_config` function and settings env-var tests.
- `docker-in-instance` spec (prior): mocked `DockerService` lifecycle tests, feature-gated real-Docker tests, and the `dini_smoke_test.sh` precedent.
- Existing `host_port.rs` allocator tests are the template for the new subnet allocator.

## Out of Scope

- **Nested port publishing** (`docker run -p` forwarding, host ports for nested services, Traefik routes for nested apps). Nested services stay `--network=host` inside the instance, unchanged.
- **Host firewall / iptables / route modifications** of any kind. Isolation comes entirely from Docker bridge semantics.
- **Per-instance egress ACLs or DPI** beyond Docker's bridge isolation (a firewall feature, not topology).
- **Multi-host / swarm isolation.**
- **IPv6** on instance networks (nested `dockerd` already runs `--ip6tables=false`).
- **Startup reconciliation** of orphaned instance networks.
- **Mid-flight migration** of already-running legacy containers; a pre-existing instance gets its network on its next start.
- **Private/internal DNS integration beyond the `OW_INSTANCE_DNS` setting** (no host resolv.conf mirroring).
- **Subnet-pool exhaustion UX** beyond the standard launch error path.

## Further Notes

- **Verified live facts** underpinning this spec: (1) Docker accepts `/30` user-defined bridges and assigns the container exactly one IP; (2) two `/30` networks are mutually unreachable while both keep internet; (3) `--dns` alone does not bypass the broken `127.0.0.11` resolver under `runsc`; (4) rewriting `/etc/resolv.conf` in-container restores resolution under `runsc` (in-instance `curl` and nested `docker pull` both verified); (5) the default `bridge` is cross-reachable between instances — the flaw being fixed.
- **Image-side work:** the resolv.conf rewrite lives in the in-repo image entrypoints (both `kasmvnc` and `kasmvnc-dini`). The API only sets `OW_DNS`. Tickets should split API-side (settings, allocator, lifecycle, tests) from image-side (entrypoint rewrite) and verify the pair together in the smoke test.
- **Base range hygiene:** `10.200.0.0/16` was chosen against the live host's ranges; operators with different private ranges must set `OW_INSTANCE_NET_BASE`.
- **Prod deploy note:** confirm `OW_HOST_GATEWAY_IP` / Traefik `host.docker.internal` resolution against the bound port during the deploy step (pre-existing concern, carried forward).
