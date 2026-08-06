# 03 — Remove Container-IP Lookup (get_container_ip)

**What to build:** No code path resolves a container's bridge IP anymore — routing and health checks both use the port pool, so the API's only networking duty is maintaining host ports. This is a wide mechanical deletion (trait method, implementation, call sites, and every mock/test expectation that pinned the old behavior) done after the call sites have already migrated, keeping the compile clean under the zero-warning policy.

**Blocked by:** 01 — Port-Pool Networking for Instances, 02 — Stop/Start/Delete/Health on the Port-Pool Topology

**Status:** done

- [x] No reference to container-IP lookup remains anywhere in the API (both feature gates compile with zero warnings).
- [x] All mock expectations and real-Docker integration tests that asserted IP-based routing/probing are updated or removed.
- [x] Launch, start, delete, and health flows run end-to-end with the mock suite green after the deletion.

## Notes

- Deleted `get_container_ip` trait method + its `network_name` parameter and `DockerClient::get_container_ip` impl (`src/docker.rs`, −25).
- Removed all 60 `expect_get_container_ip` mock blocks across `instances_mock_test.rs` / `health_worker_test.rs`.
- Removed the 3 real-Docker `get_container_ip` tests in `docker_test.rs` and the now-obsolete `test_launch_get_ip_fails_still_succeeds`.
- `network_name()`/`with_network` kept: still used by the real `DockerClient` create path.
- Verification: `scripts/check.sh` clean (both gates), `scripts/run_tests.sh` = **423 passed, 0 skipped** on two consecutive runs.
