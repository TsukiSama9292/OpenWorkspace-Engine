# 04 — Effective-context policy module, per-request auth, `/auth/me`

**Track:** backend

**What to build:** The deep pure policy module from the spec (the successor to the quota policy module) plus the per-request wiring. `calculate_effective_context` takes a user, their personal template ids, their groups, and the group→template-id map and returns an `EffectiveContext` (flags OR-ed, whitelist union-ed, `direct_max_instances` precedence with group-max fallback, admin bypass, creator self-whitelist). `pre_flight` runs the three launch checks (whitelist, per-user ceiling, host ceiling) as a pure decision function. The auth extractor resolves the user, memberships, and both whitelists from the database on every request and builds the context; `AuthUser` keeps compatibility methods (`is_admin`, `can_manage_users`, …) backed by the context so existing route gates keep compiling and behave identically. The JWT claim set drops `role` (identity only), and `GET /api/auth/me` returns the effective context.

**Blocked by:** 02-be-schema-migration

**Status:** ready-for-agent

- [x] Policy module fully unit-tested: flag OR, whitelist union, ceiling precedence, admin bypass, empty-whitelist default-deny, creator self-whitelist, ceiling `0` = no limit, pre-flight ordering
- [x] `/auth/me` reflects a group-flag change on the very next request without re-login
- [x] The JWT carries no role claim; login/validate/session flows updated
- [x] Existing role-based route gates still behave identically during this transition
