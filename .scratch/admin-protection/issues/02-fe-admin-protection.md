# 02 — Admin account & group protection: UI mirror

**Track:** frontend

**What to build:** The user-management panel stops offering the delete action on the admin's own row. The Delete button is hidden for any user who `is_admin`, so no one is offered an action the API now forbids; the Edit action stays available so an admin's personal ceiling can still be adjusted. *Refined in code-review:* for an admin row the policy dialog hides the group-toggles section behind a "membership protected" note and saves without `group_ids` (the API rejects any membership rewrite of an Admin member), leaving the ceiling editable. Non-admin rows keep their Delete button exactly as before.

**Blocked by:** 01 — Admin account & group protection: API guards

**Status:** completed

- [x] Delete button hidden on rows whose user `is_admin`; non-admin rows keep Delete
- [x] Edit button still renders on the admin row; saving it omits `group_ids` (ceiling stays editable)
- [x] Admin-row policy dialog shows a "membership protected" note instead of group checkboxes
- [x] Component tests added: an admin user in the list shows no Delete on that row, a non-admin row keeps Delete, Edit renders on the admin row; editing an admin row shows the protected note and no group toggles; `submitUserPolicy(..., { omitGroupIds: true })` sends no `group_ids`
- [x] `pnpm check` green (typecheck + lint); Vitest suite green including the new cases (290/290)
