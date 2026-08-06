# 01 — Backend: cross-process host-port locking via flock

**Track:** backend

**What to build:**

Replace the in-process `PortPool` reservation with per-port Linux `flock` lockfiles so that host-port allocation is arbitrated across every process sharing the host (parallel integration-test processes and single-host API replicas), eliminating the runsc-orphan leak source. From the user's perspective: two concurrent launches can never hand the same host port to two containers, a crashing API process auto-releases its ports via the kernel with no TTL/reaper, and parallel tests no longer leak sandbox processes.

The lockfiles live in a directory resolved by a deterministic per-UID chain — env `PORT_LOCK_DIR` → `Settings.port_lock_dir` → `/run/user/<uid>/ow_ports` (if usable) → `$XDG_RUNTIME_DIR/ow_ports` (if usable) → `/tmp/ow-ports-<uid>` — so same-UID processes derive the same directory by construction. Directory is created `0700` and owner/mode verified via `fstat` (non-matching candidates are skipped for the next; allocation fails closed if none is usable). Lockfiles are never unlinked, eliminating the delete/recreate inode TOCTOU.

Reserving a port *is* a non-blocking `flock(LOCK_EX | LOCK_NB)` on its lockfile, held by an `OwnedFd` (`ReservedPort { port, lock }`) across the whole allocate → create → start → DB-commit window; releasing is dropping the handle (RAII). The allocation loop keeps today's layers: DB-committed ports excluded first, then per-candidate flock (winner) followed by the TCP probe (covers ports bound by running containers). The bounded `port is already allocated` retry, the token-derived retry spread, `collect_used_host_ports`, the TCP probe, and the kill-time residual-runsc sweep all stay unchanged.

Remove entirely: `AppState.port_pool`, the `PortPool` struct and its unit tests, and the explicit `release_reserved_port` helper — the reservation handle replaces them (no in-process mutex is added; flock locks are per open-file-description so in-process contention is genuine contention). Rewire the launch/restart/recreate lifecycle to carry the handle and drop it at every commit/abandonment point (network-ensure failure, port-conflict retry, container-create failure, DB-commit failure, retry exhaustion, post-commit).

Add `rustix = "1"` as a direct dependency and a `port_lock_dir` field to `Settings` (empty = unset), updating all construction sites.

**Blocked by:** None — can start immediately.

**Status:** completed

- [ ] Zero-warning check passes (`bash scripts/check.sh`, both feature gates)
- [ ] Seam 1 (unit, temp lock directory): two acquisitions of the same port → exactly one winner (per-OFD contention); dropping the handle makes the port immediately re-acquirable (simulated crash); lockfiles still present (never unlinked) after release; lock-directory resolution follows env → settings → runtime-dir → tmp order
- [ ] Seam 2 (mock harness — real DB, mocked Docker, real HTTP): concurrent launches allocate distinct `host_port` values; the reservation is released on each failure path and after commit; a port-conflict retry re-allocates a fresh distinct port; a restart of a stopped instance reuses its committed port
- [ ] Full integration suite stays green
- [ ] No residual runsc processes after a full suite run
