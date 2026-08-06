# 01 — `system_settings` singleton: host capacity, admin settings API, admin settings page

**What to build:** the host capacity and global policy knobs become real, editable configuration. A new single-row `system_settings` table (id = 1) stores total CPU cores, total RAM bytes, the global instance limit, and the shared-mode fuse values. On startup the API seeds this row if it is absent: auto-detecting the host's CPU/RAM through the Docker socket (`docker info` — the host's totals, not the API container's), preferring explicit environment overrides, and falling back with a warning to conservative defaults when detection fails. An admin-only settings API exposes read/write over an admin UI page. From the admin's perspective: after this ticket, they can view and edit the machine's resource policy instead of restarting with different environment variables.

**Blocked by:** None — can start immediately.

**Status:** completed

- [ ] A single-row `system_settings` table exists (id = 1) with `max_cpu_cores`, `max_ram_bytes`, `host_instance_limit`, `shared_max_cpu`, `shared_max_ram`, and the row is guaranteed to exist (upserted) after startup.
- [ ] Startup seeds the row when absent: Docker-detected host capacity by default, environment variables overriding it, and a logged `WARN` + conservative defaults (8 cores / 16 GiB) when detection fails and no env override is set.
- [ ] The API still boots cleanly even when Docker detection fails (fail-open).
- [ ] Admin-only `GET` / `PUT` admin settings endpoints expose all five values; `PUT` validates non-negative values; non-admin callers get `403`.
- [ ] The admin system settings page reads and updates all five values and surfaces errors from the API.
- [ ] Unit tests cover the env/fallback precedence; API tests cover authorization and validation; frontend tests cover the settings page round-trip.
