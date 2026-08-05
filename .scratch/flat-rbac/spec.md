Status: ready-for-agent

# Flat RBAC with Groups & Effective-Context Policy Engine

## Problem Statement

OpenWorkspace controls who can do what through a 3-tier role hierarchy (`admin` / `manager` / `user`) hardcoded in the API, and bounds resource use through a per-user quota system (`instance_limit`, `max_cpu_cores`, `max_ram_bytes` with role-default fallbacks) enforced by a five-step pre-flight pipeline that deducts dedicated/shared CPU/RAM pools against a host capacity budget. Both mechanisms are rigid and complex:

- **Roles are a fixed tree.** A user is exactly one role; there is no way to grant "manage users but not Docker", or to belong to more than one management context. Adding a permission means editing a hardcoded method on the `Role` enum and re-deriving every route gate.
- **Resource quotas fight the hardware.** Administrators must reason about host capacity, dedicated reservations, shared fuses, and per-user CPU/RAM sums — a tree of deductive locks that is the source of the system's deadlock risk, and whose numbers drift from reality on every hardware change.
- **No team boundary.** There is no concept of a group of users sharing a template whitelist or an instance ceiling; every permission decision is either global or per-user, so a lab manager cannot say "these 30 students may launch up to 2 instances each from these 3 templates".
- **No data-safety lifecycle.** Persistent host directories outlive the users and instances that created them, but the system has no registry of them, no way to see orphans, and no guarded cleanup path — an operator tempted to "clean up" risks `rm -rf` on live experiment data.

The platform needs a permission model that is *flat* (no hidden hierarchy), *composable* (a user belongs to any number of groups plus their own personal overrides), *fast to evaluate* (set operations, no pool arithmetic), and *safe for data* (never auto-deletes anything that persists).

## Solution

Replace the role hierarchy and the resource-quota pipeline with a **flat RBAC model** built from:

- **Flat groups** — a single tier of policy containers. Each group carries five boolean permission flags (`can_create_template`, `can_manage_users`, `can_manage_group_instances`, `can_manage_docker`, `can_manage_registry`), a group `max_instances`, and a group template whitelist.
- **Personal user config** — each user may belong to any number of groups, set a personal `direct_max_instances` (a personal override that wins over every group), and hold a personal template whitelist.
- **An effective-context engine** — on every request the API computes the user's *effective context* by OR-ing the flags across all their groups, union-ing every group's template whitelist plus their personal whitelist, and resolving `max_instances` as *personal value if set, else the maximum across their groups*. A `is_system_admin` boolean bypasses every check.
- **A minimal pre-flight** — at launch, exactly three fast checks: the target template is in the effective whitelist (default-deny, admin bypassed), the user's active instance count is below their effective `max_instances` (exact, via a single user-row lock), and the global active count is below `host_instance_limit` (best-effort, no lock; applies to system admins too). All CPU/RAM/dedicated/shared pool accounting is deleted.
- **A persistent-volume registry** — a `persistent_volumes` table records every host data path at launch, flips to `orphaned` when nothing references it, and is never auto-deleted; only a confirmed manual "thorough cleanup" by an administrator (or a `can_manage_users` holder) removes the directory.

The existing roles, per-user quota columns, `allocation_mode`, and host-capacity/shared-fuse settings are migrated away or dropped. The effective-context and pre-flight decision logic live in a pure module (no DB, no Docker) so the whole policy engine is unit-testable; orchestration (single user-row lock, counter queries, reservation) stays in the route/DB layer.

## User Stories

1. As a **system admin**, I want to be identified by an `is_system_admin` boolean instead of a `role`, so that every permission decision collapses to one bypass check.
2. As a **system admin**, I want my `is_system_admin` status to bypass the template whitelist and the per-user instance ceiling, so that I can always operate without friction (the global host ceiling still applies to admins — see Decision 2).
3. As a **system admin**, I want to create, edit, and delete flat groups, so that I can define team boundaries and management scopes.
4. As a **system admin**, I want to set a group's five permission flags, so that I can grant exactly the management surface each team needs.
5. As a **system admin**, I want to set a group's `max_instances`, so that every member of the group gets a sane instance ceiling by default.
6. As a **system admin**, I want to attach a group template whitelist, so that a team can only launch the templates I approve.
7. As a **system admin**, I want group policy (CRUD, flags, whitelist, `max_instances`) to be admin-only, so that a non-admin cannot forge a high-privilege group and join it.
8. As a **system admin**, I want the seeded **Managers** group (all five flags enabled) created by the migration and all former `manager` users moved into it, so that their existing management powers survive the role removal unchanged.
9. As a **system admin**, I want my own former `admin` account to migrate to `is_system_admin = true`, so that I keep full control after the upgrade.
10. As a **system admin**, I want the dashboard to show a group-management view, so that I can maintain groups, flags, whitelists, and ceilings without touching the database.
11. As a **user manager** (holder of `can_manage_users`), I want to create, edit, and delete user accounts and reset passwords, so that I can run the daily people operations without bothering the admin.
12. As a **user manager**, I want to assign users to existing groups and remove them, so that I can staff teams without ever touching group policy.
13. As a **user manager**, I want to set a user's personal `direct_max_instances`, so that I can grant an individual a custom instance ceiling.
14. As a **user manager**, I want to set a user's personal template whitelist, so that I can grant a specific template to one student without widening the whole group.
15. As a **user manager**, I want to be denied the ability to create or edit groups, flags, or group whitelists, so that I cannot escalate my own privileges.
16. As a **user manager**, I want to see all orphaned persistent volumes and perform a double-confirmed "thorough cleanup", so that I can reclaim disk space while being forced to confirm destructive actions.
17. As a **group instance manager** (holder of `can_manage_group_instances`), I want to start, stop, pause, unpause, and delete instances owned by any user who shares at least one group with me, so that I can manage my team's workspaces.
18. As a **group instance manager**, I want the dashboard to list the instances of same-group users, so that I can see the team's live state at a glance.
19. As a **group instance manager**, I want the instance-management scope to be exactly "owners sharing ≥1 group", so that I never control strangers' workspaces.
20. As a **template creator** (holder of `can_create_template`), I want to create templates that are globally browsable, so that everyone can see what environments exist.
21. As a **template creator**, I want to edit and delete only my own templates, so that I cannot corrupt another manager's environment definitions.
22. As a **template creator**, I want my own templates to be automatically included in my effective whitelist, so that I can immediately launch what I build without asking someone to whitelist me.
23. As an **instance user**, I want to launch any template in my effective whitelist, so that I can start my workspace with one click.
24. As an **instance user**, I want a launch attempt on a template outside my effective whitelist to be rejected with `403`, so that I know the reason and can request access.
25. As an **instance user**, I want a launch that would exceed my effective `max_instances` to be rejected, so that the ceiling is enforced.
26. As an **instance user**, I want `stopped` and `error` instances not to count against my ceiling, so that a stopped workspace frees my allowance.
27. As an **instance user**, I want concurrent launches from my account to never overshoot my ceiling, so that the limit is exact even under a rapid-fire client.
28. As an **instance user**, I want to see my own effective limits (instance ceiling and whitelist) in the UI, so that I know what I am allowed to launch.
29. As a **platform developer**, I want the effective-context computation to be a pure function over plain inputs, so that I can unit-test every combination without a database.
30. As a **platform developer**, I want the pre-flight decision to be a pure function, so that the three checks are fully branch-tested without infrastructure.
31. As a **platform developer**, I want the API to recompute the effective context on every request from the database, so that group edits take effect immediately and never require re-authentication.
32. As a **platform developer**, I want the JWT to carry only the user id, so that a stale role claim can never outlive a permission change.
33. As a **platform developer**, I want the `allocation_mode` column, the host dedicated/shared pools, and the host-capacity fields removed from the schema, so that the dead quota machinery is gone.
34. As a **platform developer**, I want the default container runtime to become `runsc`, so that fresh deployments are gVisor-isolated by default.
35. As a **platform developer**, I want `docker_in_instance` and its gVisor-sandboxed profile to remain exactly as they are, so that the already-shipped Docker-in-gVisor path is untouched.
36. As an **API consumer**, I want a new `/auth/me` endpoint returning my effective context (flags, `effective_max_instances`, whitelist, group memberships, `is_system_admin`), so that the UI and CLI can render permissions without guessing.
37. As an **API consumer**, I want a structured rejection body for whitelist and instance-ceiling violations, so that the client can render the exact reason.
38. As a **data owner**, I want the platform to never delete a persistent host directory automatically when a user, group, or instance is removed, so that my experiment data can never be silently destroyed.
39. As a **data owner**, I want my orphaned volumes to be visible with their full host path, so that I (or an admin) can decide their fate explicitly.
40. As a **platform developer**, I want the migration to copy existing `users.instance_limit` values into `direct_max_instances` and drop `max_cpu_cores` / `max_ram_bytes`, so that existing per-user ceilings survive and the dead columns disappear.

## Implementation Decisions

### 1. Schema: role removal and the flat tables

- `users`: drop `role`; add `is_system_admin BOOLEAN NOT NULL DEFAULT FALSE` and `direct_max_instances INT NULL` (`NULL` = inherit the group maximum).
- New table `groups`: `name`, `description`, the five flag booleans, `max_instances INT NOT NULL DEFAULT 2`.
- New join tables `user_groups` (user ↔ group, `ON DELETE CASCADE`), `group_templates` (group ↔ template, `ON DELETE CASCADE`), `user_templates` (user ↔ template, `ON DELETE CASCADE`).
- New table `persistent_volumes`: `owner_id` (nullable, `ON DELETE SET NULL` so a deleted user's row survives as an orphan), `host_path`, `status` (`active` | `orphaned`).
- `workspace_templates`: drop `allocation_mode`.
- `system_settings`: drop `max_cpu_cores`, `max_ram_bytes`, `shared_max_cpu`, `shared_max_ram`; keep `host_instance_limit` (0 = unlimited) alongside the existing registry/secret/runtime fields.
- Migration data steps: former `admin` → `is_system_admin = true`; seed the **Managers** group (all five flags on, `max_instances` from the old manager default) and move every former `manager` into it; copy `users.instance_limit` → `direct_max_instances`; drop the dropped columns.

### 2. Effective-context policy module (deep module, pure)

A single pure module (the successor to the `quota.rs` policy module) owns every permission and ceiling decision behind one small interface:

- `EffectiveContext` — `user_id`, `is_system_admin`, the five flags, `effective_max_instances`, and `allowed_template_ids` (a set).
- `calculate_effective_context(user, personal_template_ids, groups, group_template_ids_by_group) → EffectiveContext`:
  1. If `is_system_admin`, return the full-bypass context (admin flag on, everything else unconstrained).
  2. Flags = OR across all the user's groups.
  3. Whitelist = personal templates ∪ every group's templates.
  4. `effective_max_instances` = `direct_max_instances` if set, else the max across the user's groups (0 when there are no groups, i.e. default-deny on the ceiling only if 0 is interpreted as "none"; see Decision 4).
  5. A template creator's own templates are always added to `allowed_template_ids` (self-whitelist).
- `pre_flight(context, template_id, user_active_count, host_active_count, host_instance_limit) → Result<(), PreFlightViolation>`:
  1. If not admin and `template_id` ∉ `allowed_template_ids` → `403` (template not allowed).
  2. If not admin and `user_active_count + 1 > effective_max_instances` → instance-ceiling violation (unless the ceiling is 0, meaning no limit).
  3. If `host_instance_limit > 0` and `host_active_count + 1 > host_instance_limit` → global-ceiling violation.

This module is the single seam through which all permission/ceiling logic is exercised. No DB, no Docker, no locks inside.

### 3. Auth: per-request effective context, JWT carries only the user id

- The JWT claim set drops `role`; the token carries only `sub` (user id) and `exp`.
- The auth extractor resolves the user, their group memberships, and both whitelists from the database on every request and builds the `EffectiveContext` via the policy module. All route gates switch from `AuthUser.role` methods to the context's flags / `is_system_admin`.
- A new `GET /api/auth/me` returns the effective context so the UI can render permissions.

### 4. Ceiling semantics

- **Active** = `running`, `starting`, `paused` (unchanged from the current `ACTIVE_STATUSES`). `stopped` and `error` never count.
- `effective_max_instances == 0` means **no ceiling** (matches the old `host_instance_limit = 0` convention); a user with no groups and no personal ceiling is thus unlimited on count, but still subject to the whitelist (default-deny) and the global ceiling.
- The per-user ceiling is **exact**: the launch path runs inside a transaction that takes a `SELECT … FOR UPDATE` on the *single* user row, gathers the user's active count, runs `pre_flight`, and only then writes the reservation. A single-row lock cannot form a deadlock cycle.
- The global ceiling is **best-effort**: a non-locking `COUNT` of active instances read at check time. Racing launches from *different* users may momentarily overshoot by one or two; this is accepted to keep cross-user launches lock-free.

### 5. Permission gates

- **Group policy** (create/edit/delete groups, set flags, group whitelist, group `max_instances`): `is_system_admin` only. Non-admins get `403` even if they hold `can_manage_users`.
- **`can_manage_users`**: user account CRUD + password reset + group-membership assignment/removal + setting a user's `direct_max_instances` and personal whitelist. Also grants the orphaned-volume view and cleanup (Decision 7).
- **`can_manage_group_instances`**: start / stop / pause / unpause / delete on instances whose owner shares at least one group with the actor, plus dashboard visibility of those same-group instances. Owners always control their own instances; admins bypass.
- **`can_create_template`** + `owner_id`: edit/delete only one's own templates; admins edit/delete any.
- **`can_manage_docker`**: the raw-Docker admin surface. **`can_manage_registry`**: the registry settings surface.
- Templates are a **global browsable catalog** — every authenticated user can list/view them, but launch is gated by the effective whitelist (default-deny for empty whitelists).

### 6. Container runtime

- The settings default `OW_CONTAINER_RUNTIME` flips from `"docker"` to `"runsc"`, so templates that don't pin a runtime resolve to gVisor. Templates may still pin a runtime explicitly.
- `docker_in_instance` and its sandboxed profile (`privileged` + `/var/lib/docker` tmpfs under `runsc`, `docker.rs` `dini_security_profile`) are unchanged — Docker-in-gVisor already ships.

### 7. Persistent-volume registry and cleanup

- On every persistent launch, upsert a `persistent_volumes` row keyed by the resolved host path (`{root}/{template_name}/{owner_id}`), owner set to the instance owner, status `active`.
- When an instance is deleted, if no other active instance still references that host path, flip the row to `orphaned`. **No deletion path ever runs `rm -rf` or removes the Docker volume automatically.**
- The orphaned view and the double-confirmed "thorough cleanup" action (remove the directory/volume, then delete the row) are available to `is_system_admin` and to `can_manage_users` holders, for all volumes regardless of group.
- `instance.resolved_volume_host_path` is kept as the join key between instance and registry.

### 8. Rejection contract

- Template-not-allowed and instance-ceiling rejections use `403` for permission/whitelist failures and `409` for ceiling violations, each with a structured body (`scope` + numbers where applicable), mirroring the existing quota-rejection shape.

### 9. Web UI

- Replace the role dropdown and quota fields in user management with: group-membership assignment, personal `direct_max_instances`, and a personal template-whitelist editor.
- New admin-only **Group Management** view: create/edit/delete groups, toggle the five flags, set group `max_instances`, edit the group template whitelist.
- New **Orphaned Volumes** view (admins + `can_manage_users`) with the double-confirm cleanup.
- Remove the quota modal, `allocation_mode` from the template form, and all role-based visibility logic (`canControlInstance` etc.) in favor of `effective_*` fields from `/auth/me` and the instance owner's group relationship.
- The client renders the structured rejection body from the pre-flight for whitelist/ceiling errors.

### 10. Module shape (seams)

- **Effective-context / pre-flight policy module** — the deep pure module from Decision 2, unit-tested without infra. This replaces `quota.rs`'s role in the codebase (one policy module, all decision logic behind a small interface).
- **DB/repository layer** — reads for user + memberships + whitelists, the user-row lock + counter queries inside the launch transaction, and the `persistent_volumes` registry operations.
- **Auth extractor** — builds `EffectiveContext` from the repositories via the policy module.
- **`DockerService` trait** — unchanged, the only Docker seam; instance lifecycle after the pre-flight keeps using it.

## Testing Decisions

- **Only external behavior is tested**: a launch on a non-whitelisted template returns `403` and leaves no row; a launch past the ceiling returns `409` with the right numbers; a permitted launch reserves and builds. Tests never assert on internal call counts.
- **Effective-context policy module — unit tests, no DB/Docker** (prior art: the pure-logic tests in `quota.rs`, `network_qos.rs`). Cover: flag OR across multiple groups; whitelist union including the personal set; `direct_max_instances` precedence over group maxima; group-max fallback; admin bypass of the whitelist and per-user ceiling checks; empty-whitelist default-deny; creator self-whitelist; ceiling `0` = no limit; pre-flight ordering (whitelist before ceilings); the global host ceiling rejecting admins too.
- **Auth/`/auth/me` — integration tests** (prior art: `tests/common/pg.rs`): a group flag toggled on the server is reflected in the very next `/auth/me` call without re-login; the JWT carries no role claim.
- **Permission gates — route-level integration tests with mocked `DockerService`** (prior art: `instances_mock_test.rs`): group policy admin-only; `can_manage_users` gates on user CRUD and membership; `can_manage_group_instances` scope is exactly same-group owners; a template creator can edit only their own template; `can_manage_docker` / `can_manage_registry` gates.
- **Concurrency — integration tests against the Postgres test container**: two concurrent launches from the same user at the ceiling → exactly one succeeds (single user-row lock is exact); concurrent launches from *different* users at the global ceiling may overshoot but never deadlock.
- **Persistent volumes — integration tests**: launch upserts the registry row; deleting the last referencing instance flips it to `orphaned`; deleting a user leaves an `orphaned` row behind (`owner_id` nulled); cleanup removes the row only after the confirmed action; no path ever auto-deletes the host directory.
- **Frontend — Vitest** (prior art: `user-quota.test.ts`, `template-form.test.ts`): user-management round-tripping of group membership / personal ceiling / personal whitelist; the admin group-management form; the orphaned-volume confirm-cleanup flow; `allocation_mode` absent from the template form.

## Out of Scope

- **Any CPU/RAM/GPU/storage quota at the user or group level.** The only ceilings are instance counts (personal effective + global host). Template CPU/RAM remain as per-container limits only.
- **Hierarchical / nested groups** — groups are one flat tier by design; a group cannot contain another group.
- **Group-scoped persistent-volume visibility** — orphaned volumes are visible only to admins and `can_manage_users` holders, never scoped by group.
- **Per-group resource limits** (e.g. "this group may use at most 8 instances in total") — only per-user effective ceilings exist.
- **GPU accounting** — unchanged from today.
- **Changing `docker_in_instance`** — the Docker-in-gVisor implementation is out of scope and untouched.

## Further Notes

- The decision for `effective_max_instances == 0` meaning "no ceiling" mirrors the existing `host_instance_limit = 0` convention, so a fresh user with no groups is unlimited on count while still default-denied on templates.
- The single user-row lock is the *only* lock in the launch path: it serializes one user's launches (exact ceiling) while leaving different users entirely independent (no lock contention, no deadlock cycle).
- `is_system_admin` bypasses the whitelist and the per-user ceiling, but NOT the global host ceiling: admins are subject to `host_instance_limit` like everyone else, and their instances still count toward the global host count and the instance-count queries like any other instance.
- The effective context is always recomputed from the database, so permission changes are live within one request; the JWT is only an identity claim, never a permission cache.
