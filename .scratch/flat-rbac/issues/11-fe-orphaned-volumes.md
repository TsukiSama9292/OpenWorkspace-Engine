# 11 — Orphaned-volumes view

**Track:** frontend

**What to build:** A view listing orphaned persistent volumes (host path, owner, since-when) for `is_system_admin` and `can_manage_users` holders, with a double-confirm "thorough cleanup" action that calls the cleanup endpoint. Non-privileged users never see the view.

**Blocked by:** 09-fe-group-user-management

**Status:** ready-for-agent

- [ ] The view lists orphaned volumes with their full host paths
- [ ] The cleanup action requires a two-step confirmation before sending the request
- [ ] Visibility is gated to `is_system_admin` and `can_manage_users`; others get no route into it
- [ ] Vitest covers the list rendering and the confirm-cleanup interaction
