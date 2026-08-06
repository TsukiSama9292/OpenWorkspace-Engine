# 02 — Docker network seam

**What to build:** the Docker-API capability the API needs to own per-instance networks, added to the `DockerService` trait with a real client implementation and the mock. Three methods: `create_network` (driver `bridge`, given subnet + gateway) treated **idempotently** — an "already exists" result is success, not an error; `remove_network` treated idempotently — "not found" is success; and `list_networks` returning the subnets Docker currently knows, so the allocator can compute the in-use set. All three map "not found / already exists" Docker responses to success so retries, restarts, and backfills never surface as errors.

**Blocked by:** None — can start immediately.

**Status:** done

- [x] `create_network(name, subnet, gateway)` exists on the trait + real client (driver `bridge`), returns `Ok` when the network already exists, `Err` only for genuine failures (e.g. invalid subnet, permission).
- [x] `remove_network(name)` exists on the trait + real client, returns `Ok` when the network is already gone.
- [x] `list_networks()` returns the set of existing network subnets (and names) so the allocator's in-use set is accurate.
- [x] The mock (`mockall`) exposes the new methods so existing mocked-`DockerService` tests keep compiling.
- [x] Zero-warning policy preserved: no dead code, no suppressions.
- [x] Feature-gated real-Docker tests: create a `/30` network → the container-side gateway/subnet land as requested; re-create returns `Ok`; remove-then-remove returns `Ok`; list reflects the created network.

## Comments

- Part of per-instance `/30` isolation spec. `docker network create --driver bridge --subnet <n>.0/30 --gateway <n>.1` is a standard Docker-API call — no host configuration involved.
- Verified in code: `DockerService` trait methods at `docker.rs:364-377`, idempotency via `network_error_is_idempotent_success` (`docker.rs:203-225`), `#[mockall::automock]` on the trait. Real-Docker `test_network_create_list_remove_idempotent` passes (`docker_test` binary, 37/37 green).
