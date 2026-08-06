Status: completed — delivered across `.scratch/archive/admin-protection/issues/01-be-admin-protection.md`, `02-fe-admin-protection.md`, `03-int-end-to-end.md` (all closed). Code-review findings folded back in: the demote guard was refactored to a tier check shared with the escalation guard, the harness pre-flight guard is section-scoped and the post-run check cannot be silently skipped or "N/A"–defeated, and the FE admin-row policy dialog omits `group_ids` so the ceiling stays editable.

# Admin Account & Group Protection + Fuzzer Hardening

## Problem Statement

The security-fuzzing work (`.scratch/archive/security-fuzzing/`, complete) exposed a real product gap that the fuzzer itself cannot be trusted to keep invisible:

1. **The `admin` account is deletable.** RBAC's tier guardrail in `delete_user` blocks only *non-admins* from deleting an admin (`!auth.is_admin() && tier <= target`); an admin can delete any admin — including themselves. On 2026-08-06 Schemathesis's unexpected-method probing executed `DELETE /api/users/{id}` with the admin session and permanently deleted the `admin` user. There is no self-delete guard and no last-admin guard.

2. **The `admin` account can be demoted.** The escalation guard in `update_user` also skips admins, so an admin can send `group_ids` that omit the Admin system group and strip their own (or any admin's) Admin membership — a self-join-style identity change that leaves the system with no admin, permanently.

3. **The fuzzer can silently self-destruct.** The `unexpected-methods = []` fix lives in `schemathesis.toml` — a harness config file. If that line is ever removed or regressed, the fuzzer regains the power to mutate state (delete users/templates) with zero warning, exactly as the 08:16 incident did. Nothing fails loudly when the harness's own safety assumptions are violated.

Separately, the web UI currently *offers* the delete action on the admin's own row (Delete button renders for admins on any target), inviting the destructive path even though the API will soon forbid it.

## Solution

Enforce admin immutability at the API layer (the source of truth), mirror the outcome in the UI (stop offering the impossible action), and harden the fuzz harness so a regression of its safety config (or any unexpected state mutation) fails the run loudly instead of destroying data silently.

- `delete_user` returns **403** when the target is a member of the Admin system group (resolved by `kind = 'admin'`, not by hardcoding the `admin` username). This covers self-delete, admin-delete-admin, and any future admin-member case in one rule.
- `update_user` returns **403** when the target is currently a member of the Admin system group and the payload's `group_ids` would drop the Admin group. Admin membership becomes non-removable by anyone, including the admin themselves. Adding a user *into* the Admin group is already impossible (`can_assign_groups`: a tier-2 actor can only assign groups of tier < 2) — this stays as-is and gains test coverage.
- The `UserManagementPanel` hides the **Delete** button on rows whose user `is_admin`. **Edit** remains available (personal ceiling + non-admin group toggles are still meaningful; the Admin group checkbox is already absent from `assignableGroups`, so the UI cannot strip Admin membership anyway).
- `security_api.sh` gains two guards:
  1. **Pre-flight config guard**: the run dies if `schemathesis.toml` no longer contains `unexpected-methods = []` — the original self-destruction mode can never silently come back.
  2. **Post-run integrity check**: before Pass 1, snapshot admin integrity (the `admin` user exists and is a member of the Admin group) plus `workspace_templates` / `workspace_instances` row counts; after Pass 2, re-verify and die loudly on any mismatch.

System-group protections that already exist (Admin/User groups cannot be renamed, deleted, or have flags changed; no one can be *added* to the Admin group) are out of the new-code scope but gain regression tests where cheap.

## User Stories

1. As a system admin, I want the `admin` account to be undeletable through the API (by anyone, self included), so that the system can never be left without its root account by a single destructive request.
2. As a system admin, I want `delete_user` to return 403 (not 204) when the target is an Admin-group member, so that the API contract reflects the invariant rather than silently succeeding.
3. As a system admin, I want the delete guard keyed on Admin **group membership** (resolved by kind) rather than the literal username `admin`, so that the rule holds even if the seed account is renamed or membership is granted some other way in the future.
4. As a system admin, I want `update_user` to return 403 when a payload would remove the Admin group from a current Admin-group member, so that no one — including the admin themselves — can demote the last admin.
5. As a system admin, I want password/username edits on the admin account to keep working, so that the immutability guards block only identity-destroying operations, not routine account management.
6. As a system admin, I want the Admin group membership of an existing member to be non-removable, so that the single-admin invariant holds. *Revised during code-review:* an Admin member's whole membership list is immutable via the API — `validate_assignable_groups` rejects any payload carrying the Admin group id, and the new guard rejects any payload that would drop it — so "other groups stay freely editable" (original wording) is not achievable without weakening the no-promotion guard; the correct contract is "membership list frozen, identity/ceiling editable".
7. As a system admin, I want the web UI to hide the Delete button on the admin user's row, so that users are not offered an action the API forbids.
8. As a system admin, I want the Edit action on the admin row to remain available, so that the personal instance ceiling can still be adjusted without exposing a destructive path. *Refined:* the admin-row policy dialog hides the group-toggles section behind a "membership protected" note and omits `group_ids` from the save payload (the API would 403 on it), leaving the ceiling editable.
9. As a developer, I want `pnpm run security:api` to abort with a clear error if `schemathesis.toml` no longer disables unexpected-method probing, so that a harness-config regression cannot silently reintroduce the destructive fuzz mode.
10. As a developer, I want the fuzz run to snapshot admin integrity and template/instance counts before fuzzing and verify them unchanged afterwards, so that any unexpected state mutation is caught and the run fails loudly instead of leaving the dev DB damaged.
11. As a developer, I want the post-run integrity check to fail with a message identifying the damaged resource (admin missing / no longer admin / template count changed / instance count changed), so that the failure is actionable.
12. As a developer, I want integration tests covering: admin self-delete → 403, admin delete another admin → 403, non-admin delete admin → 403, plain-user delete still works → 204, admin self-demotion → 403, admin demoting another admin → 403, non-admin removing an admin's membership → 403, plain-user membership edits still work → 200, and adding a user into the Admin group (create + update) → 403, so that every branch of the new guards is locked in at the HTTP seam.
13. As a developer, I want a web component test asserting the admin row shows no Delete button (while non-admin rows keep theirs), so that the UI mirror is locked in at the component seam.

## Implementation Decisions

- **Guard predicate — group membership, not username.** Admin-ness is "member of the system group whose `kind = 'admin'`", exactly as the rest of the system derives `is_admin`/tier (db.rs `derive_tier`; FE `user.is_admin`). No hardcoded `admin` string.
- **`delete_user`** (users.rs): after the existing existence check and the existing non-admin tier guardrail, add: if the target's derived tier is Admin (already loaded via `PolicyRepository::load_user_tier`), return 403. Because tier 2 is only reachable through Admin-group membership, the already-computed `load_user_tier` value is the exact membership test — no extra query, no new seam.
- **`update_user`** (users.rs): load `load_user_tier(id)` once when a policy write is present (shared with the escalation guard, so no extra round-trip), then — in addition to the existing escalation rule — if the target's derived tier is Admin and the payload carries `group_ids`, return 403. Because `validate_assignable_groups` rejects any payload that includes the Admin group id, a payload reaching this point necessarily drops the Admin group; `>= TIER_ADMIN` + `group_ids.is_some()` is the complete demotion test. Guard applies to all actors (admin and non-admin), so admin self-demotion and admin-demotes-admin are both blocked. An *empty* `group_ids` list also triggers it.
- **Escalation-guard interaction**: the demotion check shares the single `load_user_tier` call with the existing tier escalation guard (users.rs) — for a non-admin actor the existing rule already forbids policy writes to an equal/higher-tier target; the new rule additionally forbids anyone (admin included, whom the escalation guard exempts) from rewriting an Admin member's memberships. Keying both guards off the derived tier removes the `GroupRepository::find_by_kind("admin")` / `list_group_ids` lookups the original draft used, and eliminates the fail-open branch when the admin group lookup returns `None`.
- **Only membership is immutable.** The `update_user` guard fires solely on "current Admin member + payload carries group_ids." Username/password changes and `direct_max_instances` on the admin account remain allowed (user story 5; verified live: `PUT {direct_max_instances}` on the admin row returns 200). Per user story 6 (revised), an Admin member's membership *list* is not rewritable through the API at all; memberships of non-admin users remain fully editable.
- **Status codes**: both new guards return **403 FORBIDDEN** (RBAC semantics, matching the surrounding handlers). No new error bodies.
- **No schema/migration change.** Admin immutability is enforced in the route layer; the DB schema and the `user`/`group` tables are untouched. No new columns.
- **FE mirror** (`UserManagementPanel.svelte`): the actions cell currently renders under `canManage && mayManageUser(...)`. The Delete button's condition additionally requires `!user.is_admin`. Edit stays as-is. For an admin row the policy dialog replaces the group-checkbox section with a "Admin membership is protected — memberships cannot be changed here" note, and `submitUserPolicy` is called with `omitGroupIds` so the save payload carries only `direct_max_instances` (sending `group_ids` would 403 — the Admin group is not in `assignableGroups` but the form row still holds it). Create-user flow is unchanged (Admin group already excluded from the group picker by `assignableGroups`).
- **Harness — pre-flight config guard** (`security_api.sh`): after the fail-fast health ping, an awk pass checks that an uncommented `unexpected-methods = []` line sits inside the `[phases.coverage]` section of `schemathesis.toml`, and `die`s with an actionable message if absent. Section-scoping (added after code-review) closes the "relocate the line to a dead section" bypass — a plain `grep` over the whole file would let the destructive mode come back silently.
- **Harness — post-run integrity check** (`security_api.sh`): before Pass 1, capture (a) the admin user's existence + `is_admin` flag via `GET /api/users` and (b) row counts for `workspace_templates` / `workspace_instances` via the API list endpoints (part of the 17-endpoint fuzz surface). After both passes, re-fetch and compare; on mismatch, `die` identifying the damaged resource (per user story 11). Hardening added after code-review: the two `run_pass` invocations capture their exit codes so the integrity check **always runs** even when a pass fails hard (`set -e` would otherwise abort before it), and an unreadable count endpoint yields `N/A`, which is treated as an integrity **failure** — "N/A == N/A" can never silently pass. The fuzz-user self-provisioning creates one user and no templates/instances, so it does not disturb the snapshot.
- **Drift-check implications**: none of the two API guards change responses of any of the 17 fuzzed endpoints (users writes are outside the fuzz surface), so the committed spec and the fuzz passes stay green unchanged.

## Testing Decisions

One seam per layer, all pre-existing — no new seams introduced:

1. **API (primary) — HTTP integration tests in `apps/api/tests/users_test.rs`**, running against a real Postgres test container through `TestContext` (`ctx.login_admin()`, `ctx.post/put/delete`, real cookie sessions). Prior art: `test_delete_user_created`, `test_delete_user_forbidden_for_non_admin`, `test_manager_can_manage_user`. New tests (each a user story in the list above):
   - admin self-delete → 403, and the admin still exists afterwards (`GET /api/users/{id}` → 200);
   - admin deletes another admin (a second user placed in the Admin group) → 403;
   - non-admin (plain user) deletes admin → 403 (already true, kept as regression);
   - plain-user delete by admin → 204 (existing `test_delete_user_created` behavior, still passes);
   - admin self-demotion (`PUT` with `group_ids` omitting Admin) → 403, admin still `is_admin`;
   - admin demoting another admin → 403;
   - non-admin removing an admin's Admin membership → 403;
   - a plain user's membership edit → 200 (existing path, still passes);
   - `create_user` with `group_ids` containing the Admin group id → 403, and `update_user` adding a user into the Admin group → 403 (existing guard, now locked by tests).
   - What makes a good test here: external behavior only — status codes and post-request state, never implementation internals. Assert the *invariant* (admin still exists / still admin after a rejected call).
2. **Web — component tests in `apps/web/src/tests/user-panel.test.ts` and `user-policy-form.test.ts`** (vitest + happy-dom + `@testing-library/svelte`). Prior art: `hides Edit/Delete on equal-or-higher-tier users for a manager`. New tests: rendering the panel with an admin user in the list shows no Delete button on that row while a non-admin row keeps Delete; Edit still renders on the admin row; opening the admin row's policy dialog shows the "membership protected" note and no group checkboxes; `submitUserPolicy(..., { omitGroupIds: true })` sends a payload without `group_ids`.
3. **Harness — end-to-end verification against the running dev stack (03-int ticket)**: run `pnpm run security:api` and require both passes green *with* the new pre-flight and post-run guards active. Then mutation-style verifications: (a) temporarily remove `unexpected-methods = []` from `schemathesis.toml` → the script must `die` at the pre-flight guard before fuzzing; (b) relocate the line into a dead `[unused.*]` section → the section-scoped guard must still `die`; (c) temporarily break the integrity snapshot (delete the `admin` row directly in the dev DB mid-run) → the post-run check must `die` identifying the admin (and the template/instance counts surface as failures too). Revert all, re-run green.

Acceptance: `check.sh` silent; API nextest suite (including the new users_test cases) green; `pnpm test` (web, including the new user-panel case) green; `pnpm run security:api` green with the guards armed and provably firing under the two mutations.

## Out of Scope

- Last-admin / self-delete logic beyond the Admin-membership rule (no separate "cannot delete self" or "cannot delete the last admin" checks — the membership rule subsumes them given the single-admin invariant).
- Making the Admin group fully read-only in the UI (flags/name edit surfaces) — already forbidden server-side; no new UI work.
- DB-level constraints/triggers enforcing admin immutability — the route-layer guards are the chosen enforcement point for now.
- Changing what the fuzz surface covers (still the 17 safe endpoints; users writes remain excluded from the spec).
- Wiring `security:api` into CI / `turbo run test`.
- Any schema migration or API contract change beyond the two 403 responses.

## Further Notes

- The Admin system group is seeded at API startup; the dev DB currently holds `admin` (Admin), `user` (User), `fuzz-user` (User). The integrity snapshot's "admin exists" component is the canary for the exact 08:16 incident.
- The post-run integrity check must tolerate the fuzz-user self-provisioning (one new user, zero templates/instances) — it snapshots only admin integrity and template/instance counts, never the user count.
- The `update_user` demotion guard reuses the `PolicyRepository::load_user_tier` read already in the handler's blast radius (the escalation guard uses the same call), so no new repository surface is added — the group-kind lookup approach was dropped in code-review.
- This feature is the first outcome of the security-fuzzing loop: the fuzzer found a real RBAC gap (admin deletion), we fixed the harness so it cannot be silenced, and we are closing the product gap the fuzzer exposed.
