# 05 — Start, backfill, and delete wiring

**What to build:** the network lifecycle stays consistent across an instance's whole life. On `start` of a stopped instance, the API idempotently ensures the instance network before restarting/recreating the container (via `ensure_container_running`), so a pre-existing instance created before this feature gets its isolated network on its next start with no manual step, and a restart after a stop reuses the same network. The recreate-on-stolen-port path attaches to the same network. On `delete`, after the instance container is removed, the API removes the instance network and tolerates "not found" so a double-delete or crash-cleaned network is not an error.

**Blocked by:** 01 — subnet-allocator-settings; 02 — docker-network-seam; 03 — container-network-attachment.

**Status:** done

- [x] `start` ensures the instance network (idempotent) before restarting or recreating the container; an unchanged restart attaches to the same network and subnet.
- [x] A pre-existing instance with no network gets one on its next start (backfill) without extra state or a migration.
- [x] Recreate-on-stolen-port reuses the same network rather than allocating a new subnet.
- [x] `delete` removes the instance network after the container is removed and treats a missing network as success.
- [x] Stopped/error instances keep their network until deleted (no premature removal).
- [x] Mocked-`DockerService` integration tests cover start (idempotent ensure + reuse), backfill (create-if-missing), stolen-port recreate (same network), and delete (remove, not-found tolerated).

## Comments

- Deletion order matters: Docker refuses to remove a network with attached containers, so the container removal precedes network removal.
- Verified in code: `ensure_container_running` (`instances.rs:1057-1070`) calls `ensure_instance_network` before any restart/recreate (idempotent reuse + backfill); the port-conflict retry loop reuses the same ensured `network_name` (only the host port changes). `delete_instance` removes the container first, then `remove_network(&network_name)` (`instances.rs:603`), and `remove_network` treats "not found" as success. `timeout_action::stop` never touches the network, so stopped/error instances keep it. Real-Docker `docker_lifecycle_test` covers the launch/start/delete paths (full suite green).
