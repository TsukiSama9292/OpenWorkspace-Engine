# 03 — Remove Container-IP Lookup (get_container_ip)

**What to build:** No code path resolves a container's bridge IP anymore — routing and health checks both use the port pool, so the API's only networking duty is maintaining host ports. This is a wide mechanical deletion (trait method, implementation, call sites, and every mock/test expectation that pinned the old behavior) done after the call sites have already migrated, keeping the compile clean under the zero-warning policy.

**Blocked by:** 01 — Port-Pool Networking for Instances, 02 — Stop/Start/Delete/Health on the Port-Pool Topology

**Status:** ready-for-agent

- [ ] No reference to container-IP lookup remains anywhere in the API (both feature gates compile with zero warnings).
- [ ] All mock expectations and real-Docker integration tests that asserted IP-based routing/probing are updated or removed.
- [ ] Launch, start, delete, and health flows run end-to-end with the mock suite green after the deletion.
