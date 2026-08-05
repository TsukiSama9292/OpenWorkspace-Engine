# 02 — Flat-RBAC schema migration (additive expand)

**Track:** backend

**What to build:** The additive first half of the schema change, so nothing breaks before the contract step. A SeaORM migration that creates the flat tables (`groups`, `user_groups`, `group_templates`, `user_templates`, `persistent_volumes`), adds `users.is_system_admin` and `users.direct_max_instances`, seeds the **Managers** group (all five flags enabled), and backfills data: former `admin` users become `is_system_admin = true`, former `manager` users become members of the Managers group, and existing `users.instance_limit` values are copied into `direct_max_instances`. The legacy `role` and quota columns are deliberately kept in place.

**Blocked by:** None — can start immediately.

**Status:** ready-for-agent

- [ ] Migration up creates all new tables and columns and seeds the Managers group
- [ ] Backfill produces the expected rows (admin → `is_system_admin`, manager → Managers membership, `instance_limit` → `direct_max_instances`)
- [ ] `down` reverses the migration
- [ ] Legacy `role` and quota columns remain; the existing test suite stays green
