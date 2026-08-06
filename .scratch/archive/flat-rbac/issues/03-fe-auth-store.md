# 03 — Auth store reads the effective context

**Track:** frontend

**What to build:** The web auth store now populates from the effective context returned by `/auth/me` instead of the legacy role. The role-derived `isAdmin` / `isManager` stores are replaced by `isSystemAdmin` plus flag-based helpers (`canManageUsers`, `canManageGroupInstances`, `canCreateTemplate`, `canManageDocker`, `canManageRegistry`) computed from the effective context. The dashboard surfaces the signed-in user's own effective ceiling and allowed templates so a user knows what they may launch.

**Blocked by:** 01-fe-contract-types

**Status:** completed

- [ ] Auth store loads the effective context from `/auth/me` on login and on `check`
- [ ] Derived permission helpers come from flags / `is_system_admin`, never from a role string
- [ ] The dashboard displays the user's effective `max_instances` and allowed template count
- [ ] Vitest covers the store and derived helpers against mocked effective-context payloads
