# 01 — Effective-Context contract types and API client

**Track:** frontend

**What to build:** The web app's contract layer for the flat-RBAC feature, pinned to the spec and independent of any backend landing. TypeScript types for the effective-context payload (`EffectiveContext`: `is_system_admin`, the five permission flags, `effective_max_instances`, allowed template ids, group memberships, personal `direct_max_instances`), the `Group` type (flags, `max_instances`, group template whitelist), and the user-management payloads (group memberships, personal ceiling, personal whitelist). Typed API client functions for the pinned endpoints (`/auth/me`, group CRUD, user update with memberships/overrides, orphaned-volumes list and cleanup). No UI changes in this ticket.

**Blocked by:** None — can start immediately.

**Status:** ready-for-agent

- [ ] `EffectiveContext`, `Group`, and the membership/whitelist payload types match the contract pinned in the flat-rbac spec
- [ ] API client functions are typed against those payloads, tested with Vitest against mocked responses
- [ ] No existing UI behavior changes; `pnpm check` and `pnpm test` stay green
