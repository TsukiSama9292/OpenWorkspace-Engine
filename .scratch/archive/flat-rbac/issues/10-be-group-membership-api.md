# 10 — Group and membership management API

**Track:** backend

**What to build:** The management endpoints behind the UI in 09. Group CRUD (create/edit/delete groups, set the five flags, group `max_instances`, group template whitelist) is restricted to `is_system_admin`. Membership assignment/removal, a user's personal `direct_max_instances`, and a user's personal template whitelist are managed by `can_manage_users` holders through the user endpoints. Privilege escalation (a non-admin creating or editing a group or joining a privileged group) is rejected.

**Blocked by:** 08-be-flag-gated-routes

**Status:** completed

- [ ] Group CRUD and group-policy edits are admin-only (403 otherwise, including for `can_manage_users` holders)
- [ ] Membership assignment and personal overrides round-trip and are visible in the user's effective context
- [ ] Escalation attempts (e.g. granting a flag via a forged group write) are rejected at the route level
- [ ] Route-level integration tests cover the permission matrix from the spec
