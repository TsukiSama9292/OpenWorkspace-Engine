# 08 — Flag-gated routes

**Track:** backend

**What to build:** Every route gate migrates from the legacy role to the effective-context flags. User management is gated by `can_manage_users`; template create by `can_create_template` with edit/delete restricted to the template's owner (admin bypass); instance lifecycle by ownership, `can_manage_group_instances` (owners sharing ≥1 group), and admin; the raw-Docker surface by `can_manage_docker`; the registry surface by `can_manage_registry`; admin settings by `is_system_admin`. The instance list includes same-group instances for `can_manage_group_instances` holders. Template creators' own templates are launchable by them (the policy module already adds the self-whitelist).

**Blocked by:** 06-be-launch-preflight

**Status:** ready-for-agent

- [ ] Each gate is route-level tested with a mocked `DockerService`: permitted → 2xx, denied → 403
- [ ] A template creator can edit/delete only their own templates; admin can edit any
- [ ] `can_manage_group_instances` scope is exactly same-group owners; owners and admins keep their powers
- [ ] Same-group instances appear in the list for group managers
