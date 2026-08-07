# Container Runtime: How runC and gVisor (runsc) Report Resources Differently

> This document explains the architectural difference between the two
> container runtimes, and why the same CPU/RAM limits "look like they are not
> applied" inside a `runc` container yet show correctly inside a `runsc`
> container. Audience: developers and operations engineers. End users do not
> need these details.

## 1. Two runtimes, two roles

Each template can choose one of two runtimes (Template → Advanced → **Runtime**):

- **`runC` (Docker, default)**: the standard OCI runtime. Best performance and
  full GPU compatibility (including NVIDIA Container Toolkit passthrough). The
  server-level `OW_CONTAINER_RUNTIME` defaults to `docker`, and a template
  without an explicit runtime falls back to the server value.
- **`runsc` (gVisor, optional hardening)**: a user-space kernel (Sentry)
  intercepts syscalls, sharply reducing the container-escape surface, at the
  cost of performance.

Runtime selection lives in `runtime_to_host_config()` in
`apps/api/src/docker.rs`: an empty string / `docker` leaves the runtime unset
(Docker's default, runc); any other value is passed to Docker as
`--runtime <name>`.

## 2. The core difference: who provides `/proc`

Both runtimes write CPU/RAM limits into **cgroups** (the throttling is real);
they differ in **where the `/proc` seen inside the container comes from**.

### runC: shares the host kernel; `/proc` comes straight from the host procfs

- Isolation and resource limits rely on Linux **namespaces + cgroups**.
- `/proc/meminfo`, `/proc/cpuinfo`, and `/proc/stat` are **not cgroup-aware
  and not namespaced**; a container sees the host's global data.
- As a result, `free`, `htop`, `top`, and `nproc` inside the container report
  the **host's** total memory and core count, not this container's quota.
- `--privileged` does not change this (privileges are unrelated to procfs
  virtualization).

### runsc (gVisor): Sentry emulates the kernel in user space and synthesizes `/proc`

- Syscalls are intercepted by **Sentry** (the user-space kernel) and handled
  internally; they never hit the host's real `/proc`.
- Sentry knows this sandbox's OCI resource quota, so when `/proc/cpuinfo` or
  `/proc/meminfo` is read it **generates the content dynamically from the
  container's own limits**.
- Therefore `free` / `htop` inside a gVisor container show the **configured
  CPU/RAM limits** for that container.

### Comparison

| | `runc` (standard container) | `gVisor` (`runsc`) |
|---|---|---|
| Kernel of record | Shared host kernel (namespaces + cgroups) | User-space Sentry kernel emulation |
| `/proc` source | Host procfs (not virtualized) | Synthesized by Sentry from container limits |
| `free` / `htop` report | **Host** totals | **Container's** configured resources |
| Limits enforced? | Yes (cgroup) | Yes (cgroup + sandbox) |
| Extra dependency for correct display | Needs LXCFS or similar | Built into the runtime |

## 3. The limits really are enforced (verified)

Measured on template `Ubuntu Jammy - Desktop` (4 CPU / 12 GiB, runc):

| Check | Result |
|---|---|
| Container HostConfig | `NanoCPUs=4000000000`, `Memory=12884901888` (=12 GiB), runtime `runc` |
| cgroup v2 | `cpu.max = 400000 100000` (4 cores), `memory.max = 12884901888` |
| CPU stress (12 `yes` processes) | `docker stats` CPU = **~391%** (throttled to 4 cores) |
| RAM stress (allocating 24 GiB) | **OOM-killed by cgroup at 11 GiB** (12 GiB limit) |

Bottom line: **limits are enforced under runc too**; the in-guest monitoring
tools just cannot read them.

## 4. The resource-limit flow

```
Template (workspace_templates.cores / memory, in bytes)
  → ContainerConfig { cores, memory }            apps/api/src/routes/workspace/instances.rs
  → HostConfig { nano_cpus: cores × 1e9, memory }     apps/api/src/docker.rs
  → cgroup v2 cpu.max / memory.max (enforced by the kernel)
```

- `memory` is stored in **bytes**; the UI form's RAM(GB) is multiplied by
  `1024^3` before submission (`apps/web/src/lib/templates/template-form.ts`).
- Docker defaults `MemorySwap = 2 × Memory`. If the host has swap enabled, a
  container exceeding its RAM limit spills into swap instead of being OOM-killed
  immediately; on this host swap = 0, so going over 12 GiB OOMs directly.

## 5. If you need runc containers to show the correct limits

Options, by increasing effort:

1. **Accept the status quo**: limits are enforced; the guest reporting host
   resources is standard runc behavior.
2. **Pick runsc at the template level**: for templates where correct in-guest
   display matters (and the performance trade-off is acceptable), select
   `runsc`.
3. **Introduce LXCFS**: run a FUSE daemon on the host that rewrites `/proc`
   based on cgroups, and bind-mount it into each instance container; then
   `free` / `htop` show the container's own limits. Larger effort — requires
   adding the mount to every instance-creation flow.
4. **Show the effective limits in the UI**: label the instance card with
   "enforced: N CPU / M GiB" so users know the real ceiling without guessing
   from inside the container.

## Related docs

- `docs/developer-guide/gvison.md` — gVisor/runsc install, GPU (NVProxy) passthrough.
- `docs/developer-guide/tech-stack.md` §4 — the dual-runtime technology decision.
