# 01 — System groups, effective context, tier guardrails, group-only template auth, contract drop

**Track:** backend

**What to build:** The entire flat-rbac_2 backend in one continuous pass — schema, policy engine, guardrails, system-group rules, group-only template authorization, and the contract drop. One continuous pass: the migration runs expand → contract in sequence, and every backend test lands green with zero compiler warnings before the frontend ticket starts.

**Migration (expand first):** Add `groups.kind` and seed the three system groups: create `Admin` (kind `admin`, all five flags on, unlimited `max_instances`) and `User` (kind `user`, no flags, `max_instances = 1`), rename the existing `Managers` group to `Manager` (kind `manager`, all five flags on, `max_instances = 2`, members preserved). Move every user with `is_system_admin = TRUE` into the `Admin` group. Backfill the `Admin` group onto the template whitelist of every existing template. The legacy `users.is_system_admin` column and the `user_templates` table stay in place during the additive phase; they are dropped at the contract stage of this same ticket.

**Effective-context rework:** A user's tier is the highest kind-tier among their group memberships (`admin` = 2, `manager` = 1, everything else = 0). Admin status is "member of the Admin group", no longer the user boolean. The effective instance ceiling is the max of the personal ceiling and all group ceilings, with 0/unlimited as the highest (personal never lowers below group; unlimited always wins). The template whitelist is the union of the user's groups' whitelists only — personal and creator-self whitelists are no longer inputs. The launch pre-flight no longer exempts admins from the whitelist or the ceiling check (the host-ceiling check still applies to every tier). `/auth/me` returns the reworked context, and the group list exposes each group's `kind`.

**Tier guardrails:** Deleting a user, writing a user's policy (personal ceiling and group memberships), and assigning groups all require the actor's tier strictly greater than the target's tier — a Manager cannot edit, delete, or re-assign Admin or fellow-Manager accounts; only an Admin can delete an Admin. Group assignment additionally forbids placing a target into a group whose tier is at or above the actor's tier. Instance stop/delete and the shared-group instance listing let a `can_manage_group_instances` holder control instances owned only by users of a strictly lower tier, even when a group is shared; owner-self and root control are always allowed. The users list exposes each user's derived admin status and tier.

**System-group rules:** The three system groups (kind `admin`, `manager`, `user`) cannot be deleted and cannot be renamed. The Admin group's permission flags are fixed on (all five) and the User group's flags are fixed off (all five); only the Manager group's flags remain editable by an admin. `max_instances` stays editable for all three. Custom groups (kind `NULL`) behave exactly as before.

**Group-only template authorization:** Creating a template whitelists the Admin group on it by default (no other group is auto-whitelisted), so new templates are admin-usable until the Admin group is removed from the template's whitelist via the group-management API. The user-policy write path drops the personal template whitelist: the payload no longer accepts `template_ids`, and the validation and repository writes for personal template grants are removed. The launch whitelist continues to be the group-union from the effective context.

**Contract drop:** A migration drops the `user_templates` table and the `users.is_system_admin` column, and any remaining code that read them is removed (dead repository methods, residual contract fields — derived admin status in the users list already comes from Admin-group membership).

**Testing:** Migration tests via the `db_test.rs` instance, effective-context pure unit tests, HTTP integration tests for the guardrails / system-group rules / template authorization, and real-Docker integration tests for the instance-control guardrails. Run `apps/api/scripts/run_tests.sh` and `apps/api/scripts/check.sh` — the full suite green with zero residue.

**Blocked by:** None — can start immediately.

**Status:** ready-for-agent

- [ ] Migration expands (kind + seeds + rename + admin move + template backfill), keeps legacy objects during additive phase
- [ ] Effective context: tier = max kind-tier; admin = Admin-group membership; ceiling = max rule with unlimited highest; whitelist = group union only; pre-flight has no admin bypass
- [ ] Tier guardrails enforced on user delete, policy write, group assignment, and instance stop/delete/list
- [ ] System-group rules: undeletable/unrenamable, Admin flags fixed on, User flags fixed off, Manager flags editable
- [ ] Group-only template auth: default Admin-group grant on create; personal template whitelist removed from user-policy write
- [ ] Contract drop: `user_templates` table and `users.is_system_admin` column gone, dead code removed
- [ ] Full API suite green, zero warnings, zero residue
