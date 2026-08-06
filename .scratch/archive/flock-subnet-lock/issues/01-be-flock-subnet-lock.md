# 01 — Backend: cross-process /30 subnet locking via flock

**Track:** backend

**What to build:**

Extend the existing `flock` registry to `/30` subnet blocks so that instance-network allocation is arbitrated across every process sharing the host (parallel integration-test processes and single-host API replicas), mirroring the host-port lock already shipped. From the user's perspective: two concurrent launches can never hand the same `/30` block to two networks, a crashing API process auto-releases its subnet reservation via the kernel with no TTL/reaper, and subnet collisions are prevented rather than merely recovered by retry.

Each free `/30` block's network address gets one lockfile in the same per-UID lock directory already used for ports; the lockfile name is derived from the network address (e.g. `10.200.0.0.lock`) so it can never collide with numeric port lockfiles. Lockfiles are never unlinked (no delete/recreate inode TOCTOU), are opened `O_CREAT | O_RDWR | O_CLOEXEC | O_NOFOLLOW` mode `0600` inside the verified `0700` lock directory, and allocation fails closed when no usable lock directory resolves — exactly matching the port path. `Settings.port_lock_dir` / env `PORT_LOCK_DIR` / the `ow_ports` directories are unchanged, now documented as shared by both registries; no new setting.

Generalize `host_port::acquire_lock` from `(lock_dir, port: u16)` to `(lock_dir, key: &str)` — port callers pass the numeric port as a string, behavior unchanged. Add a `ReservedSubnet { subnet: Ipv4Addr, lock: OwnedFd }` reservation and a `try_allocate_subnet(used, base, from_block, lock_dir)` allocator in the instance-network module (importing `acquire_lock` from the host-port module), mirroring `try_allocate_port`: exclude the Docker-derived used set, then per candidate a non-blocking `flock` is the winner. No subnet "probe" is added — the used set already contains every Docker network, and the bounded `Pool overlaps` retry absorbs the residual stale-snapshot race.

Rewire `ensure_instance_network`: resolve the lock directory once (fail-closed), then per attempt list networks → derive used set → `try_allocate_subnet` → `create_network` while holding the reservation → on success drop the reservation (Docker's pool commit is the release boundary) and return; on `Pool overlaps` drop and retry from the per-instance spread offset; on other errors drop and fail. The idempotent existing-network reuse check (a network that already exists by its instance-derived name is reused without allocation) is unchanged.

Remove entirely: `AppState.network_lock` (the in-process `tokio::sync::Mutex`) and its `.lock().await` guard — flock locks are per open-file-description, so in-process contention is genuine contention and one mechanism replaces the other. Update all reference sites.

Kept unchanged: the bounded `Pool overlaps` retry, `is_network_pool_overlap`, the per-instance spread offset, the idempotent ensure, and Docker's atomic pool-commit as the source of the used set.

**Blocked by:** None — can start immediately.

**Status:** completed

- [x] Zero-warning check passes (`bash scripts/check.sh`, both feature gates)
- [x] Seam 1 (unit, instance-network module, temp lock directory): per-OFD contention on a `/30` (two acquisitions of one subnet → exactly one winner); `drop` of the handle makes the subnet immediately re-acquirable (simulated process death); the allocator skips a flock-held candidate; `None` when every candidate in a narrow pool is held; lockfiles still present (never unlinked) after release
- [x] Seam 2 (mock harness — real DB, mocked Docker, real HTTP): concurrent launches allocate distinct `/30` subnets; the reservation is held only during `create_network` (success releases immediately); an existing network is reused without allocation
- [x] `AppState.network_lock` and its guard are gone entirely
- [x] Full integration suite stays green
