Status: ready-for-agent

# Cross-Process /30 Subnet Locking via flock

## Problem Statement

Instance `/30` subnet allocation is not arbitrated across processes. Each launch calls `ensure_instance_network`: it snapshots the Docker network list, computes the lowest free `/30` from the base range, and creates the network. The only guard is `AppState.network_lock` — a `tokio::sync::Mutex`, which serializes only within one API process.

Both real deployment shapes are multi-process, exactly as with host ports: the parallel integration test suite runs each test as its own process, and a single-host multi-API-replica deployment runs several API processes. Two concurrent launches can read the same `list_networks` snapshot, compute the same free `/30`, and race to `docker network create`. The loser's `Pool overlaps` error is absorbed by the bounded retry (with a per-instance spread offset), so today the race is recovered but not prevented — and the recovery depends on Docker's reject, not on allocation itself being exclusive.

This is the mirror image of the host-port problem already fixed via `flock` (see `.scratch/flock-port-lock/spec.md`): ports have a cross-process `flock` registry, subnets do not. The asymmetry is accidental; the same mechanism should cover both.

## Solution

Extend the existing `flock` registry to `/30` blocks. Each free `/30` block network address gets one lockfile in the same per-UID lock directory already used for ports; allocating a subnet *is* `flock(LOCK_EX | LOCK_NB)` on that file, held by an `OwnedFd` for the duration of the `create_network` call. The kernel ties the lock to the open file description, so a crashing process releases its lock automatically; lockfiles are never unlinked, so no two processes can hold locks on two different inodes at the same path.

Allocation keeps today's layers, mirroring the port allocator: the Docker-derived used set (every existing network's `/30`) is still excluded first, then each candidate is tried with a non-blocking `flock` (winner). Because the used set already contains every Docker network, no extra "probe" is needed — the only race left is a stale snapshot, where the loser wins `flock` on a subnet its snapshot didn't see and is then rejected by Docker's `Pool overlaps`, absorbed by the existing bounded retry. No new probe seam.

The existing `AppState.network_lock` is removed: `flock` locks are per open file description, so in-process concurrency contends exactly like cross-process concurrency, and one mechanism replaces the other.

## User Stories

1. As the API, I want `/30` subnet allocation arbitrated across every process sharing the host, so that two concurrent launches can never hand the same `/30` block to two networks.
2. As the API, I want the subnet reservation released automatically if my process dies mid-allocation, so that no `/30` block is ever permanently blocked and no background reaper is needed.
3. As a developer running the parallel test suite, I want every test process to lock against the same subnet registry, so that cross-test subnet collisions are prevented rather than merely recovered.
4. As an operator deploying multiple API replicas on one host, I want them to arbitrate subnet allocation through the shared lock directory, so that replica-to-replica races never collide on a `/30`.
5. As the API, I want subnet lockfiles never deleted, so that two processes can never end up holding locks on two different inodes at the same path (the delete/recreate TOCTOU).
6. As the API, I want subnet lockfiles opened with `O_CREAT | O_RDWR | O_CLOEXEC | O_NOFOLLOW` and mode `0600` inside the verified `0700` lock directory, so that other local users cannot interfere with or hijack the arbitration.
7. As the API, I want the subnet and port lockfiles to share one lock directory, so that operators configure exactly one lock location and both registries are always consistent.
8. As the API, I want subnet lockfile names derived from the `/30` network address (e.g. `10.200.0.0.lock`) so that they can never collide with numeric port lockfiles in the same directory.
9. As the API, I want allocation to fail closed when no usable lock directory exists, so that a broken lock infrastructure never silently reintroduces the subnet race.
10. As the API, I want the reservation held for the duration of the `create_network` call only, so that a success that commits the pool atomically releases the lock immediately afterward and never blocks subsequent allocations.
11. As the API, I want the existing bounded retry on `Pool overlaps` kept, so that the residual stale-snapshot collision (loser wins `flock` on a subnet its snapshot missed) still recovers without error.
12. As the API, I want the existing idempotent ensure behavior kept: a network that already exists (per its instance-derived name) is reused without any allocation or lock, so that restarts and backfill never allocate twice.
13. As a developer, I want the removed `AppState.network_lock` field and its in-process mutex gone entirely, so that there is exactly one arbitration mechanism for subnets.
14. As a developer, I want `try_allocate_port` and `try_allocate_subnet` to share the same `acquire_lock` primitive, so that the lock semantics are defined once.
15. As a developer, I want a deterministic unit test proving per-OFD flock arbitration on a `/30` (two open file descriptions on one subnet → exactly one winner), so that the kernel-level semantics are pinned down cheaply.
16. As a developer, I want unit tests proving the allocator skips flock-held candidates and returns `None` when every candidate in the pool is held, so that exhaustion behavior is pinned without Docker.
17. As a developer, I want the existing two-process flock E2E to additionally assert the two concurrently launched instances land on distinct `/30` subnets, so that the cross-process subnet path is exercised under real contention.

## Implementation Decisions

- **Mechanism**: Linux `flock` via the existing `rustix` dependency. Non-blocking `LOCK_EX | LOCK_NB` only, safe directly in async code.
- **Shared primitive**: `host_port::acquire_lock` is generalized from `acquire_lock(lock_dir, port: u16)` to `acquire_lock(lock_dir, key: &str)`; port callers pass the numeric port as a string (unchanged behavior, same `{port}.lock` filenames). `ReservedPort` is unchanged.
- **New reservation primitive**: `ReservedSubnet` (`subnet: Ipv4Addr` + `lock: OwnedFd`) in the instance-network module. Acquire = `open(lockfile, O_CREAT | O_RDWR | O_CLOEXEC | O_NOFOLLOW)` then `flock(NB)`; release = dropping the handle (RAII). Lockfiles never unlinked.
- **Allocation loop**: `try_allocate_subnet(used, base, from_block, lock_dir) -> Option<ReservedSubnet>` in the instance-network module, importing `acquire_lock` from the host-port module. Mirrors `try_allocate_port`: clone the used set; for each candidate in circular order from `from_block`, try `flock(NB)` — on failure skip and remember as busy; on success return the reservation. `lowest_free_subnet` merges into this loop's `from_block == 0` case (avoids dead code under the zero-warning policy).
- **Lifecycle wiring**: `ensure_instance_network` resolves the lock directory once (fail-closed), then per attempt: list networks → derive used set → `try_allocate_subnet` → `create_network` while holding the reservation → on success drop the reservation (pool committed atomically by Docker) and return; on `Pool overlaps` drop the reservation and retry from the per-instance spread offset; on other errors drop and fail. The idempotent existing-network reuse check is unchanged and runs before any allocation.
- **Removals**: `AppState.network_lock` field and the `.lock().await` guard in `ensure_instance_network`. All reference sites updated.
- **Naming**: `Settings.port_lock_dir` / env `PORT_LOCK_DIR` / the `ow_ports` directories are **unchanged** — now documented as shared by the port and subnet registries. No new setting.
- **Fail-closed**: if no usable lock directory resolves, subnet allocation returns exhaustion and the launch errors, matching the port path.
- **Kept unchanged**: the bounded `Pool overlaps` retry, `is_network_pool_overlap`, the per-instance spread offset, the idempotent ensure, Docker's atomic pool-commit as the source of the used set.

## Testing Decisions

A good test here asserts only observable arbitration outcomes — exactly one winner per `/30`, subnet reusable after drop or after a simulated crash, distinct subnets under concurrency — never lockfile internals.

- **Seam 1 (unit, instance-network module, temp lock directory)**: per-OFD contention on a `/30` (two acquisitions of one subnet → exactly one winner); `drop` of the handle makes the subnet immediately re-acquirable (simulated process death); the allocator skips a flock-held candidate and returns `None` when every candidate in a narrow pool is held; lockfiles still present (never unlinked) after release. Prior art: the pure-function unit tests in `instance_net.rs` and the flock tests in `host_port.rs`.
- **Seam 2 (integration, mock-instance harness — real DB, mocked Docker, real HTTP server)**: concurrent launches allocate distinct `/30` subnets; `ensure_instance_network` holds the reservation only during create (success releases immediately); an existing network is reused without allocation. Prior art: the launch/start tests in the mock-instance test file.
- **Implicit cross-process coverage**: the existing two-process flock E2E (`two_process_flock_e2e_test.rs`) is extended to assert the two concurrently launched instances land on distinct `/30` subnets, exercising the cross-process subnet path under real contention with two real API processes.
- **Regression gates**: full suite stays green and `bash scripts/check.sh` (zero warnings, both feature gates) is clean.

## Out of Scope

- Multi-machine subnet coordination (flock is host-local by design; each host arbitrates its own Docker networks).
- Changing the existing-network reuse semantics: a network that already exists keeps its subnet across stops, starts, and backfill.
- Introducing a subnet "probe": the Docker-derived used set already contains every network, and the bounded retry absorbs the stale-snapshot race.
- Renaming `port_lock_dir` / `PORT_LOCK_DIR` / `ow_ports` to neutral names (documented as shared instead).
- Lockfile garbage collection (files are intentionally never deleted).
- Removing the bounded `Pool overlaps` retry or the per-instance spread offset.

## Further Notes

- `flock` locks attach to the open file description, not to the process — so the in-process unit contention test is a genuine arbitration test, and removing `network_lock` loses no in-process serialization.
- The reservation window is deliberately narrower than for ports: the port handle is held until the DB commit because the port's authoritative record is the DB row; a subnet's authoritative record is Docker itself, and `create_network`'s success commits the pool atomically — so the release boundary is the create call.
- Production API runs as root: the same resolution chain as ports applies (`/run/user/0` usually absent → configured dir or `/tmp`). Dev and the test suite run as UID 1000 with `/run/user/1000/ow_ports`.
