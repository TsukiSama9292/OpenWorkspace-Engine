# 01 — Admin account & group protection: API guards

**Track:** backend

**What to build:** The `admin` account becomes undeletable and un-demotable through the API. `delete_user` returns 403 (FORBIDDEN) when the target is a member of the Admin system group — covering self-delete, admin-delete-admin, and any future admin member — so the system can never be left without its root account by a single destructive request. `update_user` returns 403 when the target is currently an Admin-group member and the payload's `group_ids` would drop the Admin group — so no one, including the admin themselves, can strip Admin membership. Routine account management on the admin account (username/password changes, personal ceiling, membership of non-admin groups) keeps working. Adding users *into* the Admin group remains forbidden (existing `can_assign_groups` rule) and gains explicit regression coverage.

**Blocked by:** None — can start immediately

**Status:** completed

- [x] `delete_user` returns 403 when the target is an Admin-group member (resolved by the system group with `kind = 'admin'`, not the literal username); returns 403 for admin self-delete and admin-delete-another-admin
- [x] `update_user` returns 403 when the target is currently an Admin-group member and the payload's `group_ids` omit the Admin group (including an empty list); applies to all actors, so admin self-demotion and admin demoting another admin are both blocked
- [x] The guards are additive to the existing tier escalation guardrail; username/password edits and `direct_max_instances` on the admin account remain allowed (200)
- [x] Admin-member membership list is immutable via the API (any `group_ids` payload for an Admin member is rejected — `validate_assignable_groups` blocks the Admin-group-id payload, the new guard blocks the drop); *deviates from the original "non-admin groups editable on an Admin member" wording, which is not achievable without weakening the no-promotion rule. Non-admin users' memberships remain fully editable.*
- [x] `create_user` / `update_user` attempting to place a user into the Admin group returns 403 (existing rule, now covered by tests)
- [x] New HTTP integration tests land in the API suite: admin self-delete → 403 with admin still present, admin deletes another admin → 403, non-admin deletes admin → 403, plain-user delete by admin → 204, admin self-demotion → 403 with admin still `is_admin`, admin demotes another admin → 403, non-admin removes an admin's Admin membership → 403, plain-user membership edit → 200, create/update into Admin group → 403
- [x] `check.sh` silent (zero warnings, both feature sets); full nextest suite green (609/609)
