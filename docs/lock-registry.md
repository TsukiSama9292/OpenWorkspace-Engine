# Host Port & Subnet Lock Registry

Cross-process arbitration for the two finite resource pools the API hands out per
instance: **host ports** (`host_port.rs`) and **instance `/30` subnets**
(`instance_net.rs`). Both allocators share one mechanism — non-blocking `flock`
lockfiles in a per-UID directory — so concurrent launches can never claim the
same port or subnet, no matter how many API processes share the host.

This replaced the old in-process `network_lock` mutex (`tokio::sync::Mutex`),
which only serialized allocation within a single process. The flock registry
works across processes with zero coordinator, no TTL, and no reaper.

---

## Why flock and not a mutex / DB row / TTL

| Approach | Problem |
|---|---|
| In-process `Mutex` | Only serializes one process. Two API replicas on one host can collide; so can the test suite running alongside a dev/prod API. |
| DB row + transaction | Adds a DB dependency on the hot path; both allocators are also used before any DB is reachable, and the E2E runs two processes against the same Postgres. |
| TTL / reaper | Needs a reaper and tolerates stale locks; also can't distinguish "held briefly by a live peer" from "abandoned". |
| `flock` | The **kernel** holds the lock on the open file description: a live holder releases it on `drop`, a crashed process releases it automatically. No coordinator, no TTL, no cleanup. |

A non-blocking `flock` (not a blocking wait) keeps the allocator a simple scan:
candidates that fail to lock are just skipped. No process ever blocks on
another's allocation.

---

## Lockfile model

- **Directory:** one shared per-UID directory for *both* registries, resolved by
  `host_port::resolve_lock_dir` in order:
  1. `Settings.port_lock_dir` (`PORT_LOCK_DIR` env var) if set
  2. `/run/user/<uid>/ow_ports`
  3. `$XDG_RUNTIME_DIR/ow_ports`
  4. `/tmp/ow-ports-<uid>`

  The dir is created `0700` and verified (owned by the current UID, real dir,
  no group/other bits) before use — a candidate that can't be verified is
  skipped. `resolve_lock_dir` returning `None` makes allocation **fail closed**.

- **Filename:** `<key>.lock` where the key is the resource identity:
  - ports → the decimal port, e.g. `10042.lock`
  - subnets → the `/30` network address, e.g. `10.200.0.0.lock`

- **Lockfiles are created if absent and never unlinked.** Unlinking would let
  two processes hold "the same lock" on two different inodes at one path. The
  directory is a fixed registry; files accumulate (a few bytes each) and are
  reused forever.

- **Hold window:** only as long as the `OwnedFd` handle lives (`ReservedPort`,
  `ReservedSubnet`). Both reservation structs are RAII — dropping the handle
  releases the lock even if the process dies mid-window.

---

## Allocation

### Host ports — `try_allocate_port`

1. Scan circularly from `from` (lowest free port on first attempt; a per-instance
   token-derived **spread** on retry) over the pool, skipping the DB-committed
   `used` set.
2. For each candidate: try a non-blocking `flock`. A lock **loser** is skipped
   (a live peer owns the port).
3. A lock **winner** is then TCP-probed (`host_port::port_in_use`) to catch
   ports bound by things that don't participate in the flock (a container
   created by another tool, a manual process, Docker's own binding). Probe
   "busy" → drop the lock, skip.
4. Winner holds the `ReservedPort` through allocate → container create → start →
   DB commit, then drops it.

### Subnets — `try_allocate_subnet`

1. Compute the `used` set from `docker list_networks` (the source of truth for
   which `/30`s already exist).
2. Pick the lowest free aligned `/30` (or, on retry, scan circularly from the
   token-derived spread), skipping used blocks.
3. `flock` the block's lockfile; a loser is skipped. **No probe** — the
   Docker-derived `used` set already contains every existing network, so the
   only residual race is a stale snapshot, which the overlap retry absorbs.
4. Winner holds the `ReservedSubnet` only through the `create_network` call —
   Docker's pool commit is the release boundary.

Both allocators return `None` when the pool is exhausted (every candidate used
or lock-held).

---

## Retry: the stale-snapshot safety net

The flock prevents *two cooperating processes* from claiming the same resource
from the same snapshot. It cannot prevent a resource that was **created after**
our snapshot: `list_networks` is read, then a *non-participating* actor (or a
subnet that appeared between list and create) makes our candidate stale.

That residual race is absorbed by a bounded retry keyed on Docker's own error:

- **Ports:** `create_container_with_port_retry` catches `port is already
  allocated`, re-allocates from the per-instance spread, and retries the create
  (also used on the start/recreate path).
- **Subnets:** `ensure_instance_network` catches `Pool overlaps`, drops the
  reservation, re-lists, re-allocates from the spread, and retries — up to 4
  attempts total.

The per-instance spread (`spread_offset` / `spread_block_offset`, FNV-1a over
the access token) means concurrent retries don't stampede back onto the same
lowest block.

Allocation **fails closed** when no usable lock directory resolves — proceeding
unlocked would risk handing out a duplicate port or subnet. Failures of the
`flock` syscall on an individual candidate (after the dir resolved) simply skip
that candidate.

---

## Lifecycle & release

| Event | Port | Subnet |
|---|---|---|
| Normal success | `ReservedPort` dropped after container create/start | `ReservedSubnet` dropped after `create_network` |
| Instance delete | Container `docker rm -f` releases Docker's binding; flock handle already gone | `remove_network` after the container is gone (network refuses removal while veths live) |
| Process crash mid-window | Kernel releases the `flock` on fd close | same |
| Kill of a stuck runsc process | `kill_residual_runtime_procs` frees veth/port leaks | same (frees the veth so the network can be removed) |

There is intentionally **no** "force unlink / force release" for a lockfile:
the kernel tie-breaks via the open file description, and any unlink-then-recreate
would create the two-inode race. If a holder is wedged, the lock is only ever
freed by that process exiting (kernel release) or being killed.

---

## Fail-closed vs fail-open

- **Lock dir unresolvable** → allocation fails closed (launch errors rather
  than risking a duplicate).
- **Individual candidate lock busy** → skipped, next candidate.
- **Create-time conflict** (`port already allocated` / `Pool overlaps`) →
  bounded retry from a spread; exhaustion after retries is a launch error.
- **Network removal failure** → logged, does not block instance deletion; the
  subnet stays in Docker and is therefore still counted in the used set, so it
  is never re-allocated while it exists.

---

## Testing

- **Unit (no Docker):** `host_port.rs` and `instance_net.rs` exercise the
  allocators against isolated temp lock dirs — lock-held candidates are skipped,
  used sets respected, circular scan wraps, exhaustion returns `None`,
  drop/crash releases, lockfiles persist.
- **Seam 2 (mock HTTP + real allocator):** `instances_mock_test.rs` proves two
  concurrent launches from an empty snapshot get distinct ports/subnets
  (Barrier-gated in the mock), that a successful launch releases its subnet
  reservation, and that start reuses the existing network without allocating.
- **Two-process E2E:** `two_process_flock_e2e_test.rs` runs two independent API
  processes against one Postgres and asserts distinct host ports *and* distinct
  aligned `/30`s via `docker inspect`.

---

## Key files

- `apps/api/src/host_port.rs` — `acquire_lock`, `resolve_lock_dir`, `ReservedPort`,
  `try_allocate_port`, `is_port_conflict`, `spread_offset`
- `apps/api/src/instance_net.rs` — `NetBase`, `ReservedSubnet`, `try_allocate_subnet`,
  `spread_block_offset`, `gateway_ip`
- `apps/api/src/routes/workspace/instances.rs` — `ensure_instance_network`,
  `create_container_with_port_retry` (the retry loops)
- `apps/api/src/docker.rs` — `remove_container_by_id` (`force: true`),
  `kill_residual_runtime_procs`, network idempotent-success seam
