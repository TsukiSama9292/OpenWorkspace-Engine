# 02 — Per-user quotas: users columns, role defaults, override API, admin user fields

**What to build:** every user gets a personal quota with sane role defaults and the ability for an admin to grant a specific override. The `users` table gains three nullable columns — `instance_limit`, `max_cpu_cores`, `max_ram_bytes` — where `NULL` means "inherit the role default". Role defaults are a code constant: user = 2 instances / 4 cores / 8 GiB, manager = 5 / 12 / 32 GiB, admin = exempt. A single `resolve_effective_quota` function turns a user's role and optional override into the effective quota (this is the seam a future Group tier slots into). The existing admin user management API accepts per-user overrides, and passing `NULL` restores the role default. From the admin's perspective: after this ticket, they can see and set any user's personal limits without a special role.

**Blocked by:** None — can start immediately.

**Status:** ready-for-agent

- [ ] `users` has the three nullable quota columns, `NULL` preserved by the API round-trip.
- [ ] `resolve_effective_quota(override, role)` implements role defaults and override precedence, returning "exempt" for Admin; it is a single function so a future Group tier is one more fallback level.
- [ ] The user update API accepts `instance_limit` / `max_cpu_cores` / `max_ram_bytes` overrides and treats `NULL` as restore-to-role-default; non-admin callers cannot set quotas.
- [ ] The admin user management UI exposes the three fields with empty = inherit role default, and shows the resulting effective values.
- [ ] Unit tests cover role defaults, override precedence, `NULL` inheritance, and Admin exemption; API tests cover authorization and the `NULL` restore.
