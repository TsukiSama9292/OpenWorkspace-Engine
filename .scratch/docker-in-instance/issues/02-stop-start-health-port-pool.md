# 02 — Stop/Start/Delete/Health on the Port-Pool Topology

**What to build:** A stopped instance keeps its host port and its route; restarting reuses the same port with no route churn; instances created before this feature get a port backfilled on first start; deleting an instance frees its port; health checks probe the exact published path real traffic uses.

**Blocked by:** 01 — Port-Pool Networking for Instances

**Status:** done

- [x] Stopping and starting an instance keeps the same `host_port` (no re-allocation, no route rewrite).
- [x] An existing instance with no stored port is allocated and persisted one on next start, before its container is created/started.
- [x] Deleting an instance removes its row and frees the port for reuse; an `error`-state instance keeps its reservation until deleted/replaced.
- [x] The health worker probes `https://<host gateway IP>:<host_port>/` — the identical path Traefik uses — and no longer resolves a container IP for health.
- [x] Seam 2 mocked lifecycle and health-worker tests are green (port stability, backfill, free-on-delete, new probe URL).

## Notes

- `stop` and `timeout_action::stop` no longer call `delete_route`; the route survives stop and start skips rewriting it when the port is unchanged (rewrites only on legacy backfill or recreate-on-stolen-port). `timeout_action::remove` still deletes the route (row removed → port freed).
- Health worker drops the `docker`/`template_repo` params entirely and probes `https://{OW_HOST_GATEWAY_IP}:{host_port}/`; a `starting` instance with a NULL `host_port` is skipped (never happens after 01 except a crash window). `host_gateway_ip` is threaded through `health_worker::run` from settings.
- `route_writer::default_dynamic_dir` made `pub` so integration tests can assert route files.
- Tests: +5 (`test_probe_no_host_port_skips_instance`, `test_stop_preserves_host_port_and_route`, `test_start_backfills_host_port_before_create`, `test_delete_frees_host_port`, `test_error_instance_keeps_port_reservation`); Seam 3 `test_stop_and_start_instance` now asserts port stability. 427 passed (0 skipped), `check.sh` zero warnings.
