Status: completed

# Cross-Process Host-Port Locking via flock

## Problem Statement

Concurrent instance launches race on host-port allocation. Docker binds the host port at `start`, not `create`; when two launches pick the same probe-free port, the slower `start` fails with `port is already allocated`, its container sits `created`, and under the `runsc` runtime its sandbox process leaks permanently (observed: up to 23.7GB of orphaned `runsc-sandbox`/`runsc-gofer` processes on the host).

The current in-process `PortPool` reservation closes that window only within a single API process. It does not arbitrate across processes — yet both real deployments of this platform are multi-process:

- the parallel integration test suite (`cargo nextest`) runs each test as its own process against its own isolated database, and
- a single-host multi-API-replica deployment runs several API processes.

Today cross-process collisions still occur (measured: ~59 leaked sandboxes cleaned by the kill backstop per full-suite run) and are absorbed only by the bounded retry plus a kill-time orphan sweep. The leak source itself — two processes handing the same port to two containers — is not prevented.

## Solution

Replace the in-process `PortPool` with **per-port Linux file locks**: each host port gets one lockfile; reserving a port *is* `flock(LOCK_EX | LOCK_NB)` on that file. The lock is held by an `OwnedFd` across the whole allocate → create → start → DB-commit window, and releasing it is dropping the fd. Because the kernel ties the lock to the open file description, a process that crashes or is killed mid-window automatically releases its ports — crash-safety with no TTL, no reaper, and no registry to sweep. Lockfiles are never unlinked, so no two processes can ever hold locks on two different inodes at the same path.

The lockfiles live in a directory resolved by a deterministic chain — env `PORT_LOCK_DIR` → `Settings.port_lock_dir` → `/run/user/<uid>/ow_ports` (writable) → `$XDG_RUNTIME_DIR/ow_ports` (writable) → `/tmp/ow-ports-<uid>` — so every process of the same UID on the host derives the same directory and therefore arbitrates against each other, across parallel tests and across single-host API replicas alike. The directory is created `0700` and its ownership/mode verified before use; allocation fails closed if no usable directory exists.

Allocation keeps today's layers: the DB-committed port set is still excluded first, then each candidate is tried with a non-blocking `flock` (winner), then the TCP probe (covers ports bound by running containers). A port bound by a stopped container of another process can still be picked; that collision surfaces at `start` and is absorbed by the existing bounded retry, per the agreed acceptance bar (leak source zero, all collisions recovered, no errors, no residue). The kill-time residual-runsc sweep (Direction 1) remains as the final backstop.

## User Stories

1. As the API, I want host ports arbitrated across every process sharing the host, so that two concurrent launches can never hand the same port to two containers.
2. As the API, I want the reservation released automatically if my process dies mid-launch, so that no port is ever permanently blocked and no background reaper is needed.
3. As a developer running the parallel test suite, I want every test process to lock against the same host port registry, so that cross-test collisions can no longer leak `runsc` processes.
4. As an operator deploying multiple API replicas on one host, I want them to arbitrate through the shared lock directory, so that replica-to-replica races never leak orphan sandboxes.
5. As the API, I want lockfiles never deleted, so that two processes can never end up holding locks on two different inodes at the same path (the delete/recreate TOCTOU).
6. As the API, I want the lock directory created `0700` with `O_CLOEXEC` and `O_NOFOLLOW` on every lockfile, so that other local users cannot interfere with or hijack the arbitration.
7. As the API, I want the lock directory resolved deterministically per UID from env → settings → runtime dir → tmp, so that production replicas and test processes derive the same path without coordination.
8. As the API, I want allocation to fail closed when no usable lock directory exists, so that a broken lock infrastructure never silently reintroduces the leak.
9. As the API, I want the reservation handle to be the only way to observe the port during the window, so that a port cannot be released by a stale owner.
10. As the API, I want the reservation released on every success and failure path (DB commit, network-ensure failure, port-conflict retry, container-create failure, DB-commit failure, retry exhaustion), so that normal operation never leaks a port.
11. As the API, I want the existing bounded retry on `port is already allocated` kept, so that collisions outside the lock registry (external binders, stopped-container reuse) still recover without error.
12. As the API, I want the TCP probe kept, so that ports bound by running containers — even those that never went through our allocator — are not handed out again.
13. As a developer, I want the removed in-process `PortPool` and its `AppState` field to be gone entirely, so that there is exactly one arbitration mechanism.
14. As a developer, I want a deterministic unit test proving per-OFD flock arbitration (two open file descriptions on one port → exactly one winner), so that the kernel-level semantics are pinned down cheaply.
15. As a developer, I want a mock-harness test launching many instances concurrently through the HTTP API and asserting distinct allocated ports, so that the reservation handle's lifecycle wiring is verified end-to-end.
16. As a developer, I want the kill-time residual-runsc sweep and the `scripts/cleanup.sh` last-resort sweep unchanged, so that even an unforeseen leak is still reaped.

## Implementation Decisions

- **Mechanism**: Linux `flock` via the `rustix` crate (add `rustix = "1"` as a direct dependency; already present transitively). Non-blocking `LOCK_EX | LOCK_NB` only, so the calls are safe directly in async code — no `spawn_blocking`.
- **Reservation primitive**: a `ReservedPort` handle (`port: u16` + `lock: OwnedFd`) in the host-port module. Acquire = `open(lockfile, O_CREAT | O_RDWR | O_CLOEXEC | O_NOFOLLOW)` then `flock(NB)`; success means the caller owns the port for the window. Release = dropping the handle (RAII); the fd close releases the kernel lock. Lockfiles are **never unlinked**.
- **Lock directory resolution**: `Settings.port_lock_dir` (new field; empty = unset) layered as env `PORT_LOCK_DIR` → settings → `/run/user/<uid>/ow_ports` (if usable) → `$XDG_RUNTIME_DIR/ow_ports` (if usable) → `/tmp/ow-ports-<uid>`. Creation with mode `0700`, followed by `fstat` verification that the directory is owned by the current UID and is not a symlink; a candidate that fails verification is skipped for the next. "Usable" means existing-and-verified, or created-and-verified.
- **Allocation loop** (replaces the in-process pool logic): start from the DB-committed port set (`collect_used_host_ports`, unchanged); for each candidate in circular order from the current scan offset: try `flock(NB)` — on failure skip; on success run the TCP probe — if busy, drop the lock and skip; else return the `ReservedPort`. Keep the token-derived scan offset for retry spread.
- **Lifecycle wiring**: `allocate_and_reserve_port` now returns `Option<ReservedPort>` instead of `Option<u16>`; the explicit `release_reserved_port` helper is removed and replaced by dropping the handle at each abandonment/commit point (network-ensure failure, port-conflict retry, container-create failure, DB-commit failure, retry exhaustion, and after the successful DB commit). `create_container_with_port_retry` and `ensure_container_running` carry the handle across their window and read the port via the handle.
- **Removals**: `AppState.port_pool` field and the `PortPool` struct (and its two unit tests) are deleted; no in-process mutex is added, because `flock` locks are per open file description — concurrent allocations in one process contend exactly like across processes.
- **Kept unchanged**: the bounded port-conflict retry, `is_port_conflict`, `collect_used_host_ports`, the TCP probe, the kill-time residual-runsc sweep (Direction 1), and `scripts/cleanup.sh`.
- **Fail-closed**: if every lock-directory candidate fails, allocation returns exhaustion and the launch errors rather than proceeding unlocked.

## Testing Decisions

A good test here asserts only observable arbitration outcomes — exactly one winner per port, port reusable after drop or after a simulated crash, distinct ports under concurrency — never lockfile internals.

- **Seam 1 (unit, host-port module, temp lock directory)**: per-OFD contention (two acquisitions of the same port → exactly one wins); drop of the handle makes the port immediately re-acquirable (simulated process death); a released-but-kept handle does not block re-acquisition after drop; lockfiles are still present (never unlinked) after release; lock-directory resolution order (env override wins, writable-runtime-dir fallback, `/tmp` last). Prior art: the existing pure-function unit tests in `host_port.rs`.
- **Seam 2 (integration, mock-instance harness — real DB, mocked Docker, real HTTP server)**: concurrent launches through `POST /api/instances` allocate distinct `host_port` values; reservation is released on each failure path (network-ensure failure, container-create failure, DB-commit failure, retry exhaustion) and after commit; a port-conflict retry re-allocates a fresh distinct port; a restart of a stopped instance reuses its committed port. Prior art: the launch/start/error-path tests in the mock-instance test file.
- **Implicit cross-process coverage**: the parallel `nextest` suite — every test process resolves the same lock directory by construction, so the real multi-process arbitration is exercised by the whole suite; the existing post-suite check (zero residual `runsc` processes) serves as the acceptance signal.
- **Regression gates**: full suite stays green and `bash scripts/check.sh` (zero warnings, both feature gates) is clean.

## Out of Scope

- Multi-machine / multi-host port coordination (flock is host-local by design; each host arbitrates its own ports).
- Changing the committed-port semantics: `workspace_instances.host_port` remains the long-term registry.
- The stopped-container cross-process reuse case (port committed in another process's DB, currently unbound): remains absorbed by the bounded retry — that is the agreed acceptance bar.
- Lockfile garbage collection (files are intentionally never deleted).
- Removing the TCP probe, the DB used-port scan, or the kill-time orphan sweep.
- The `scripts/cleanup.sh` last-resort sweep.

## Further Notes

- `flock` locks attach to the open file description, not to the process — which is why a single-process contention test is a genuine arbitration test, and why no in-process mutex is needed.
- Production API runs as root: `/run/user/0` is typically absent, so the chain falls through to a configured `PORT_LOCK_DIR` (compose env) or `/tmp`. Dev and the test suite run as UID 1000, where `/run/user/1000/ow_ports` is the natural shared location.
- This replaces the previously proposed DB-backed reservation table: the kernel-owned lock needs no TTL/reaper, and the shared directory (not per-test databases) is what gives the parallel suite its arbitration — the "test pollution" objection to a DB table is moot here because sharing the directory is the point.
