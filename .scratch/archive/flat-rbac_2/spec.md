# flat-rbac_2 — Default System Groups + Tier Guardrails + Group-Only Template Authorization

Status: completed

## Problem Statement

Flat RBAC (flat-rbac) replaced the 3-tier role model with groups of flags, but the
deployment has no built-in hierarchy. "Admin" is still a user-level boolean
(`is_system_admin`), "Manager" is a hand-seeded group, and there is no "User" tier.
Admins therefore cannot express "these people are admins", "managers default to
everything but must never touch each other or the admins", or "regular users launch
one instance from what they're granted". User creation also offers no way to pick a
user's group at creation time, so every new account starts ungrouped.

Separately, template authorization is inconsistent: admins bypass the template
whitelist entirely (and per-user whitelists add a second, redundant authorization
channel). There is no way to revoke a template from admins, and per-user grants
cannot be expressed as a group.

## Solution

Seed three fixed **system groups** — Admin, Manager, User — each with a machine
identity (`groups.kind`). Admin-group membership *is* the definition of admin
(replacing the `is_system_admin` boolean, which is dropped). Users derive a **tier**
from their memberships (Admin=2, Manager=1, everything else=0). All user-management
actions and instance control are **tier-guarded**: you can only manage users of a
strictly lower tier. Effective privileges always resolve to the **highest** available
value across a user's memberships.

**Template authorization is group-only.** The whitelist is the union of the
user's groups' whitelists (`group_templates`); the per-user whitelist
(`user_templates`), the personal whitelist editor, and the creator self-whitelist are
all removed. Admins are not exempt: a template is launchable by an admin only when
the Admin group (or another of the admin's groups) is whitelisted on it. Every newly
created template whitelists the Admin group by default, and the migration backfills
the Admin group onto all existing templates. To authorize a user for a template, an
admin whitelists a group on it and adds the user to that group. The Create User form
gains a tier-filtered group picker, and the UI hides management actions for
equal-or-higher tier targets.

## User Stories

1. As the seeded admin, I want to be a member of the Admin group, so that my admin
   powers come from a real, visible group membership rather than a hidden boolean.
2. As an admin, I want the Admin group to have all permissions fixed (not editable),
   so that admins can never be accidentally de-privileged.
3. As an admin, I want to see which permissions the Admin group grants me, so that I
   can audit my own effective privileges.
4. As an admin, I want to set the Admin group's maximum instance count (the only
   admin-controlled admin ceiling), so that I can cap how many instances admins run.
5. As an admin, I want the Admin group to start with no cap (unlimited), so that
   admins are not unintentionally limited out of the box.
6. As an admin, I want to be the only one who can manage the Admin group, so that
   nobody else can change the admin tier's limits.
7. As an admin, I want to edit the Manager group's flags, so that I can tune managers'
   permissions down from "everything" when needed.
8. As an admin, I want to manage Manager and User accounts and their instances, so
   that the admin tier can govern the whole hierarchy.
9. As an admin, I want to create/edit/delete (custom) groups, so that I can shape the
   permission landscape.
10. As an admin, I want to delete the Admin, Manager, or User accounts of anyone, so
    that I can decommission users.
11. As a manager, I want to start with all permissions by default, so that I can run
    the day-to-day without asking an admin first.
12. As a manager, I want to create and manage User accounts (including their ceiling
    and group assignments), so that I can onboard regular users.
13. As a manager, I want to stop and delete instances owned by Users, so that I can
    manage the fleet I'm responsible for.
14. As a manager, I want to be unable to edit, delete, or re-assign Admin accounts,
    so that managers can never tamper with the top tier.
15. As a manager, I want to be unable to delete fellow Manager accounts, so that
    managers can't remove each other.
16. As a manager, I want to be unable to stop/delete instances owned by Admins or
    fellow Managers, so that the tier guardrail also covers instance control.
17. As a manager, I want the create-user group picker to only offer groups I'm
    allowed to assign (User/custom groups, never Admin or Manager), so that I can't
    promote someone through the create form.
18. As a user, I want to be in the User group by default with no permission flags, so
    that new accounts start safe.
19. As a user, I want to launch instances only from templates whitelisted on the
    groups I belong to, so that template access has a single, group-based mechanism.
20. As a user, I want at most one running instance by default, so that the User tier
    is cheap to host.
21. As a user, I want the UI to hide edit/delete/stop actions on users and instances
    of equal or higher tier, so that I don't see actions I can't take.
22. As any member of multiple groups, I want my effective privileges to resolve to
    the highest available value (flags OR, whitelist union, ceiling = max), so that
    membership in a stronger group always wins.
23. As an admin, I want a personal ceiling to only ever raise a user's ceiling above
    their group's, never lower it below, so that group entitlements can't be
    unilaterally shrunk.
24. As an admin, I want the `is_system_admin` column gone and my admin status derived
    from Admin-group membership, so that there is a single source of truth.
25. As a deployer, I want existing admin users migrated into the Admin group and the
    old "Managers" group renamed to Manager on upgrade, so that nothing is lost.
26. As a deployer, I want the global host instance ceiling to still apply to every
    tier, so that the host is protected regardless of group.
27. As an admin, I want the three system groups to be undeletable and unrenameable,
    so that the hierarchy cannot be broken by accident.
28. As an admin, I want a newly created template to whitelist the Admin group by
    default, so that admins can use new templates without extra steps.
29. As an admin, I want to launch a template only when the Admin group (or another of
    my groups) is whitelisted on it, so that even admins respect template
    authorization.
30. As an admin, I want to revoke the Admin group from a template's whitelist, so
    that I can make a template unusable by the admin tier.
31. As an admin, I want to authorize a user for a template by whitelisting a group
    and adding the user to that group, so that authorization stays group-based.
32. As a template creator, I want no automatic access to templates I created, so that
    the launch rule is uniform (authorization is group-only, no creator exception).
33. As a deployer, I want existing templates backfilled onto the Admin group's
    whitelist during migration, so that admin access to existing templates survives
    the removal of the admin bypass.
34. As an admin, I want the per-user template whitelist and its UI removed, so that
    template access has exactly one mechanism.

## Implementation Decisions

1. **Migration `000020`** (new, after `000019`):
   - Add `groups.kind VARCHAR` — `'admin' | 'manager' | 'user' | NULL` (custom groups
     keep NULL).
   - Rename existing `Managers` group → `Manager`, set `kind='manager'`, keep its 5
     flags on, set `max_instances=2`. Its members are preserved.
   - Create `Admin` (`kind='admin'`, 5 flags all TRUE, `max_instances=NULL`) and
     `User` (`kind='user'`, 5 flags all FALSE, `max_instances=1`).
   - Move every user with `is_system_admin=TRUE` into the Admin group (removing them
     from Manager if present).
   - Backfill: insert `(admin_group_id, template_id)` into `group_templates` for every
     existing template (so removing the admin bypass doesn't cut off existing access).
   - `DROP TABLE user_templates` (per-user whitelist is gone).
   - `DROP COLUMN users.is_system_admin`.
   - The migration is the single place that seeds/backfills; the existing flat-rbac
     migration test pattern proves it out.

2. **Group kinds own the system-group rules** (identity by `kind`, never by name):
   - System groups (`kind IS NOT NULL`) cannot be deleted or renamed.
   - Admin group flags are fixed (all TRUE); User group flags are fixed (all FALSE).
   - Manager group flags are seeded all-TRUE but editable by an admin.
   - `max_instances` is editable for all three (group-edit is admin-only, so "only
     admins manage the Admin group" falls out for free).
   - Custom groups behave exactly as today.

3. **Tier derivation** (pure function in the effective-context module): a user's tier
   is the maximum kind tier among their group memberships — admin=2, manager=1,
   everything else (user/custom/none)=0. Admin membership also yields
   `is_admin`/root.

4. **Effective context changes**:
   - `is_admin`/root = "is a member of the Admin group" — replaces every
     `is_system_admin` read.
   - **No admin bypass of the template whitelist.** `allowed_template_ids` is the
     union of the user's groups' whitelists only — no personal whitelist
     (`user_templates` dropped), no creator self-whitelist (`owned_template_ids`
     dropped from the context inputs). The pre-flight template check runs for
     everyone, admins included; the `is_system_admin` exemptions in the whitelist
     and per-user ceiling checks are removed entirely.
   - The per-user ceiling bypass for admins is removed: admins are capped by their
     group ceilings like everyone else (Admin group starts unlimited).
   - `effective_max_instances = max(direct_max_instances, max over group
     max_instances)`, where 0/NULL = unlimited is the highest. This replaces the
     current "direct first, else group max" rule and is the "choose the highest"
     resolution.
   - Flags stay OR'd across memberships.
   - `host_instance_limit` continues to apply to every tier (unchanged).

5. **Template authorization is group-only**:
   - The whitelist union is computed from `group_templates` for the user's groups.
   - Creating a template inserts the Admin group into its `group_templates` by
     default. No other group is auto-whitelisted.
   - The sole management control is the group → template whitelist editor (existing
     Group management page). No per-template group editor is added.
   - Launch pre-flight rejects any template not in the union (403
     `template_not_allowed`), for every tier including admins.

6. **Tier guardrails** (all enforced in the API layer, on top of flags):
   - Delete user: actor.tier must be > target.tier. This resolves the open flat-rbac
     finding ("delete any account with can_manage_users") — nobody but an Admin can
     delete an Admin, Managers can't delete Managers/Admins.
   - Policy write (personal ceiling / group memberships): actor tier must be >
     target tier. The personal *template* whitelist is no longer part of the policy.
   - Group assignment: additionally, the actor may only place a target into groups
     with tier < actor tier (a Manager cannot assign anyone to Manager/Admin groups).
   - Instance control (stop/delete, and the shared-group instance listing): a
     `can_manage_group_instances` holder may control instances owned by users of a
     strictly lower tier, even when a group is shared. Owner==self and root always
     allowed.
   - Group CRUD (create/update/delete group) remains root-only.

7. **API contracts**:
   - `/auth/me` and the users list expose a derived `is_admin` (and the user's tier /
     group memberships) instead of the dropped column.
   - `POST /users` already accepts `group_ids`; it stays, now meaningfully used by the
     create form. Existing validation keeps applying (the tier guardrail for
     assignment is checked server-side, not just in the UI).
   - The user policy payload drops `template_ids` (per-user whitelist removed).
   - `UserPolicy` drops the personal/owned template-id inputs; the effective-context
     builder takes only the user's groups and their `group_templates`.

8. **Frontend**:
   - Create User modal: a multi-select group dropdown, defaulting to User selected;
     options filtered to groups the actor may assign (tier < actor tier). Sends
     `group_ids` on create.
   - The user policy editor loses its "personal template whitelist" section (ceiling
     and group memberships remain).
   - Permission/visibility helpers become tier-aware: edit/delete on a user and
     stop/delete on an instance are hidden when the target's tier >= the actor's tier
     (owner-self and root exceptions preserved).
   - Admin self-audit: the existing permissions/effective-privileges display shows the
     Admin group's grants to the actor.

9. **No name-reservation hack**: custom groups named "Admin"/"Manager"/"User" remain
   allowed; they are ordinary custom groups because identity is by `kind`.

## Testing Decisions

A good test asserts observable behavior over the API or migration outcome, not
implementation details: e.g. "a Manager cannot DELETE another manager's account" not
"delete_user checks tier 1 < 1".

- **Migration semantics** — in the existing DB migration test binary: apply `000020`
  on top of a real Postgres, assert the `kind` column, the three seeded groups'
  flags/limits, the Managers→Manager rename, admin users moved into Admin, the
  Admin-group backfill over all existing templates, `user_templates` dropped, and the
  `is_system_admin` column dropped. Prior art: `flat_rbac_migration_backfills_legacy_roles_and_limits`.
- **Pure logic** — unit tests inside the effective-context module for tier derivation,
  the `max` ceiling rule (including 0/NULL-unlimited-beats-finite), and the
  group-only whitelist (union of `group_templates`, no personal/owned inputs, admin
  not exempt in pre-flight). Prior art: existing `#[cfg(test)]` units in that module
  (which currently assert the admin bypass and will be rewritten).
- **BE API behavior** — HTTP integration tests against real Postgres following the
  flat-rbac E2E prior art: seed Admin/Manager/User, then assert tier guardrails
  (delete/policy/assignment), the group-CRUD restrictions for system groups, the
  derived `is_admin`, and pre-flight (a User capped at their group max, an admin
  blocked from an un-whitelisted template, an admin allowed once the Admin group is
  whitelisted, a group-only grant flowing to a member). The instance stop/delete tier
  slice uses real Docker (same prior art as flat-rbac's instance tests) so it
  exercises the actual control path. The flat-rbac E2E that granted a member access
  via the per-user whitelist is reworked to grant via a group.
- **Frontend** — vitest for the tier-aware visibility helpers, a Create User modal
  component test asserting the multi-select group picker (filtered options, default
  User selected, `group_ids` sent), and removal of the personal-whitelist section in
  the policy editor. Prior art: existing `permissions.test.ts` and component tests
  under `apps/web/src/tests/`.
- Test hygiene per repo rules: unique names/`ow_*` containers, self-cleaning `/tmp`
  roots, zero residue, zero compiler warnings.

## Out of Scope

- Changing `host_instance_limit` semantics or its admin exemption (admins remain
  subject to it).
- Changing the 5 permission flags or whitelist default-deny (empty whitelist = no
  template launchable, for every tier).
- A dedicated "view my permissions" page beyond surfacing the Admin group's grants in
  the existing effective-privileges display.
- Assigning a user to zero groups via the create form (the picker always leaves the
  User group assigned).
- Making the Admin group's flags or the User group's flags editable.
- Adding a per-template "authorized groups" editor (the Group page is the control
  point).

## Further Notes

- The flat-rbac "admin not exempt from the host ceiling" decision stands.
- The tier guardrail subsumes the previously-parked "forbid deleting any admin
  account" review finding.
- A user in both Manager and User groups is tier 1 and their ceiling is the max of
  both groups' ceilings; the User group's `max=1` does not drag a Manager down
  (choose-the-highest).
- Template creators — including admins — have no automatic launch access; access is
  granted by whitelisting one of the user's groups. The Admin group is whitelisted by
  default on new templates, so the common admin case works without extra steps.
- `groups.kind` is the lookup key for root detection and the system-group rules; group
  names are cosmetic.
