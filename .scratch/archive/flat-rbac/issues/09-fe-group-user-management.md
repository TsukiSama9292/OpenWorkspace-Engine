# 09 — Group management and user management UI

**Track:** frontend

**What to build:** The admin-only **Group Management** view: create/edit/delete groups, toggle the five permission flags, set the group `max_instances`, and edit the group template whitelist. The user-management view is reworked from a role dropdown plus quota fields into group-membership assignment, a personal `direct_max_instances` field, and a personal template-whitelist editor. Permission-sensitive actions are hidden from users who lack the flag (admin-only group policy, `can_manage_users` for membership/personal overrides).

**Blocked by:** 07-fe-dashboard-permissions

**Status:** completed

- [ ] Admins can create, edit, delete groups and set flags, whitelists, and `max_instances`
- [ ] `can_manage_users` holders can assign/remove group memberships and set personal ceiling and personal whitelist
- [ ] Non-admins never see group-policy controls; non-managers never see membership controls
- [ ] Vitest covers the new forms and the visibility rules
