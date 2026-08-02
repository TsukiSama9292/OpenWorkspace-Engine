# 04 — Launch wiring: isolated network created before container create

**What to build:** the launch path delivers a truly isolated instance. When a tenant launches an instance, the API allocates the next free `/30` from the base range (via ticket 01), ensures the instance's dedicated network exists (via ticket 02, idempotently — already-exists is fine), and only then creates and starts the container attached to that network with `OW_DNS` set (via ticket 03). The port-conflict retry re-attaches to the same already-created network without re-allocating a subnet. The launch/instance response exposes the instance's network identity so an operator can curl its unique IP from the host for debugging.

**Blocked by:** 01 — subnet-allocator-settings; 02 — docker-network-seam; 03 — container-network-attachment.

**Status:** done

- [x] Launch ensures the instance network (subnet from the allocator, name from the id) **before** calling the container-create path; a concurrent launch or retry never double-allocates a subnet or fails on an existing network.
- [x] The container is created and started attached to the instance network with `OW_DNS` in its environment.
- [x] The port-conflict retry recreates the container against the same existing network without re-allocating or recreating it.
- [x] If network creation fails, the launch fails cleanly with an error status on the instance (same pattern as host-port-pool exhaustion) and no half-created container is left running.
- [x] The instance JSON exposes the allocated network/subnet/IP for operator debugging.
- [x] Mocked-`DockerService` integration test: launch calls create-network, then create-container with that network name and `OW_DNS`, in that order.
- [x] Feature-gated real-Docker test: launching through the API route lands the container on a `/30` with the unique IP.

## Notes

- `ensure_instance_network` (instances.rs): builds the used-subnet set from `list_networks()`, picks the lowest free `/30` (with `spread_block_offset` seeding), creates the network idempotently, and retries up to 4 times when Docker rejects the pool as overlapping. Serialized by a new `AppState.network_lock` (tokio Mutex) so concurrent launches in one process cannot both claim the same `/30` from the same `list_networks()` snapshot; cross-process races are absorbed by the overlap retry.
- The launch path calls `ensure_instance_network` **once**, before the port-conflict retry loop; `create_container_with_port_retry` / `build_and_create_container` attach to the already-created network (`network_name`) and pass `instance_dns` → `OW_DNS`. A network-create failure marks the instance `error` with no container created.
- `delete_instance` removes the instance network (idempotent — not-found is tolerated, real failures only warn) after the container is gone; this completes the delete half of the lifecycle (ticket 05 wires the start/backfill side).
- Response JSON exposes `network_name` (`ow-<instance-id>`) on instance serialization.
- Mocked tests: `test_launch_creates_network_before_container_with_ow_dns` (order asserted), `test_launch_network_overlap_reallocates_subnet`, `test_launch_subnet_pool_exhausted_marks_error`, `test_launch_network_create_failure_marks_error`, `test_launch_list_networks_failure_marks_error`, `test_launch_response_exposes_network_name`, `test_launch_port_conflict_retry_reuses_same_network`, plus `remove_network` expectations added to the 6 delete-route tests. Real-Docker: `docker_lifecycle_test` asserts the container lands on the instance network with the `/30`'s unique `.2` IP and that delete cleans it up.
- Verification: `bash scripts/check.sh` clean (zero warnings, both feature gates); `scripts/run_tests.sh` = **482 passed, 0 skipped**.
