# 02 — Stop/Start/Delete/Health on the Port-Pool Topology

**What to build:** A stopped instance keeps its host port and its route; restarting reuses the same port with no route churn; instances created before this feature get a port backfilled on first start; deleting an instance frees its port; health checks probe the exact published path real traffic uses.

**Blocked by:** 01 — Port-Pool Networking for Instances

**Status:** ready-for-agent

- [ ] Stopping and starting an instance keeps the same `host_port` (no re-allocation, no route rewrite).
- [ ] An existing instance with no stored port is allocated and persisted one on next start, before its container is created/started.
- [ ] Deleting an instance removes its row and frees the port for reuse; an `error`-state instance keeps its reservation until deleted/replaced.
- [ ] The health worker probes `https://<host gateway IP>:<host_port>/` — the identical path Traefik uses — and no longer resolves a container IP for health.
- [ ] Seam 2 mocked lifecycle and health-worker tests are green (port stability, backfill, free-on-delete, new probe URL).
