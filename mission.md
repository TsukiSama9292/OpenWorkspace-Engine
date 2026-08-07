# OpenWorkspace Engine — Mission

> This document is the project's "constitution", article one: it states **why**
> we exist, **what** we build, and **what** we deliberately do not build. Every
> feature, architecture decision, and trade-off should trace back to one of the
> value pillars in this document.

---

## One-line positioning

> **Turn any idle Linux server into a multi-user, browser-accessible, zero-setup
> cloud development environment.**

OpenWorkspace Engine is a lightweight container orchestration platform: it
provisions isolated Linux workspaces on demand (KasmVNC desktops, Jupyter Lab,
ttyd terminals) accessed through a Traefik reverse proxy in the browser, with
JWT authentication, auto-sleep, idle reclamation, bandwidth shaping, and
persistent user data.

---

## Why this project exists (The Why)

### The pain

| Pain point | Impact |
|---|---|
| **Hardware inflation** | DRAM/GPU prices outpace budgets; labs and small companies cannot refresh hardware |
| **Resource waste** | 2–5-year-old servers (many cores + GB-scale RAM) sit idle more than 90% of the time |
| **Environment chaos** | Installing CUDA/Python directly on the host causes driver conflicts and system breakage |
| **Resource hoarding** | One person owns the whole machine; off-peak, 90%+ of CPU/RAM spins unused |

### What we believe

1. **Pragmatic realism** — give old hardware a second life and relieve the
   hardware anxiety of academic and small teams.
2. **Pragmatic sustainability** — maximize utilization through software
   optimization, not by stacking hardware indefinitely.
3. **Uncompromising developer experience** — the hardware may be old, but the
   experience must be modern, smooth, and work out of the box.

### Why the browser as the entry point

Zero client installs, no VPN, no SSH setup — keep the browser open and enter a
full GUI desktop. This dramatically lowers both the psychological and technical
barriers to putting an old machine back to work.

---

## Goals (The What)

We are building a **multi-tenant, single-host-first, zero-trust-isolated**
container workspace platform:

1. **Hyper-efficiency** — Docker containers replace the 15–20% overhead of VMs;
   an 8GB-RAM host can run several isolated environments at once.
2. **Dynamic allocation** — create when needed, auto-stop when idle; hardware is
   never held empty.
3. **Browser-as-entry** — zero client, no VPN; the full GUI desktop lives in the
   browser.
4. **Zero-trust isolation** — container isolation + JWT auth + per-instance
   access credentials + per-instance `/30` subnet + cgroup resource limits.

### Glossary

| Concept | Definition |
|---|---|
| **Template** | A preset configuration bundle (image, resources, environment variables) users launch instances from |
| **Instance** | A running container launched from a template (KasmVNC / ttyd / Jupyter) |
| **User** | A person with an account; authorization is **group-based** — permissions (flags, template whitelist, instance ceiling) live on groups and are re-resolved into an effective context per request |

---

## Core Features

### Supported interfaces
- **Desktop (KasmVNC)** — a full Linux GUI desktop in the browser (HTML5 Canvas
  + WebSocket).
- **Jupyter Lab** — data-science environment with a preinstalled Python kernel.
- **Terminal (ttyd)** — lightweight browser terminal for quick CLI access.

### Security & isolation
- **Cross-tenant isolation** — every instance gets an independent 127-char
  random access token; all traffic must pass through Traefik with a valid token
  to reach the container.
- **gVisor sandbox** — `runsc` runtime selectable per template, intercepting
  high-risk syscalls to protect the host.
- **GPU passthrough (NVProxy)** — gVisor's `--nvproxy` proxies NVIDIA ioctls
  from the sandbox to the host driver (Turing/Ampere/Ada/Hopper).
- **JWT cookie auth** — `ow_token` cookie + Traefik ForwardAuth validating
  WebSocket upgrades.
- **Headless instance auth** — the proxy injects credentials server-side
  (KasmVNC/Jupyter/ttyd); the browser never sees the secrets.
- **Group-based RBAC** — permissions consist of group flags + a template
  whitelist + an instance ceiling, recomputed from the DB per request (no stale
  token problem).

### Resource governance
- **Auto-sleep (run-time limit)** — per-template `max_run_seconds`; exceeding it
  triggers `timeout_action` (remove/stop/pause); the frontend counts down and
  warns.
- **Keep time (idle reclamation)** — the browser tab heartbeats every 10s while
  open, visible, and focused; instances idle past `keep_time_seconds` are
  reclaimed.
- **Bandwidth shaping** — per-template up/down Mbps caps enforced at the kernel
  via `tc`/HTB on the instance veth.
- **Instance ceiling** — group `max_instances` + per-user `direct_max_instances`
  combine into an effective ceiling.

### Persistent user data
- **Whole-home-directory persistence** — data survives stop, restart, and
  delete; the first mount auto-populates the image's built-in environment.
- **Server-side path resolution** — host paths are resolved and validated by the
  API as `{root}/{template_name}/{user_id}`; the client never supplies paths.
- **Three launch modes** — use persistent / don't use (ephemeral) / reset (wipe
  and start over, with a frontend confirmation warning).
- **One persistent instance per template** — a second persistent launch for the
  same (template, owner) returns 409 until the old instance is removed.

### Admin UI
- **Single-page dashboard** — instance cards, template editing, Sessions,
  Volumes, Groups, Users, and Settings all on one page.
- **Group & user management** — create accounts, assign group memberships, set
  per-user ceilings.
- **Template visibility** — `public` / `private` / `hidden` plus a group
  whitelist.

---

## Design Philosophy: Security · Stability · Performance

We optimize for the **best balance of the three** — no single one is sacrificed
for the other two:

| Layer | Technology | What we buy |
|---|---|---|
| Control-plane API | Rust | **Security + Performance** — memory safety + zero-cost abstractions; <35MB RAM, high-concurrency non-blocking I/O |
| Frontend | SvelteKit | **Performance + DX** — a lightweight static SPA without sacrificing developer convenience |
| Reverse proxy | Traefik | **Stability + Performance** — file provider + inotify hot reload; adding/removing routes is **zero-downtime** |
| Static assets | Nginx | **Performance** — HTTP caching removes repeated I/O bottlenecks |
| Container runtime | Docker OCI + runC | **Performance** — standard OCI runtime, fast instance creation |
| Container runtime (hardened) | gVisor (runsc) | **Security** — a user-space kernel intercepts syscalls, sharply reducing escape risk; selectable per template |
| Instance networking | Per-instance `/30` + host-published port | **Security (network isolation)** — see below |

### Why a dedicated `/30` per instance

A single flat subnet (e.g. one `/16`) is convenient, but it is also a
**lateral-movement attack surface**: a compromised user can scan the shared
segment and attack other instances.

- An instance's service port is **published directly to a host port on the
  Docker bridge gateway** (`<host_gateway_ip>:<host_port>`) — Traefik reaches it
  via `host.docker.internal:<host_port>` and never uses container IPs.
- Outbound internet uses a **per-instance dedicated `/30` subnet**: only the
  gateway and the instance have usable IPs, forming its own L2 segment —
  **east-west attacks are structurally impossible**.

That is network isolation taken to the extreme: each container lives in its own
bubble, with the single controlled published port as the only entrance.

---

## Anti-Goals / Out of Scope

A constitution must also define boundaries to prevent scope creep:

1. **No generic PaaS** — we are not Heroku/Vercel; we focus on "interactive
   development workspaces", not stateless web-app hosting.
2. **No multi-host high availability (for now)** — currently **single-host
   first**; multi-host orchestration (Tailscale mesh, clustering) is on the
   roadmap, but a single host must always remain the viable minimum bar.
3. **No virtual machines** — container isolation is the core; KVM/QEMU are not
   in scope.
4. **No hardware-acceleration abstraction beyond GPUs** — NVProxy covers NVIDIA
   only; AMD/Intel GPUs are not yet promised.
5. **No upgrades for their own sake** — any dependency bump must have a clear
   reason (security / performance / required feature).
6. **No enterprise identity integration without SSO** — LDAP/OIDC/2FA are
   optional roadmap items, not baseline commitments.

---

## Success Metrics (how we know we won)

- **Resource utilization**: one 8GB host can serve multiple active, mutually
  isolated dev environments, with idle instances auto-releasing resources.
- **Launch latency**: from "click launch" to "browser enters the interface" in
  seconds (container creation + route hot reload).
- **Stable & secure**: east-west isolation at the network layer, per-instance
  credentials, optional gVisor sandbox — no known attack path under default
  settings.
- **DX bar**: one `pnpm run docker:up` deploys the whole platform from source; a
  new user can create an account and launch their first instance without a
  setup guide.

---

## Related docs

| Document | Content |
|---|---|
| [tech-stack.md](docs/developer-guide/tech-stack.md) | Technology decisions, deployment & update flow (constitution, article two) |
| [roadmap.md](roadmap.md) | Phase and schedule planning (constitution, article three) |
| [docs/user-guide/architecture.md](docs/user-guide/architecture.md) | System architecture, routing, lifecycle, DB schema |
| [docs/user-guide/rbac.md](docs/user-guide/rbac.md) | Permission model (group-based) |
