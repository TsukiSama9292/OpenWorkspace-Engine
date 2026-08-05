# 07 — Dashboard permission rework

**Track:** frontend

**What to build:** The dashboard's per-instance and per-user controls are computed from the effective context instead of the legacy role. Instance actions honor ownership, `is_system_admin`, and `can_manage_group_instances` (same-group owners). Users with `can_manage_group_instances` see the instances of users sharing at least one of their groups. The template panel reflects creator self-launch (a template the user created is shown as launchable for them).

**Blocked by:** 05-fe-launch-rejection-ux

**Status:** ready-for-agent

- [ ] Instance action buttons follow the effective flag scope (owner / admin / same-group manager)
- [ ] Group managers see same-group members' instances; plain users see only their own
- [ ] The template list indicates which templates the signed-in user may launch (creator self-whitelist included)
- [ ] Vitest covers the permission helpers against mocked contexts
