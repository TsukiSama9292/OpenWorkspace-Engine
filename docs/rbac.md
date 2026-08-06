# Role-Based Access Control (RBAC)

## Overview

OpenWorkspace uses a **flat, group-based RBAC model**. There is no role hierarchy
and no per-user role column. Instead:

- Permissions live on **groups**. A group carries five boolean permission flags,
  a per-group instance ceiling, and a template whitelist.
- Users **belong to any number of groups**. Their effective permissions are the
  union (highest value wins) of everything their groups grant, plus an optional
  personal instance ceiling.
- Three **system groups** — Admin, Manager, User — are seeded by the database
  migration and give the model its hierarchy. A user's **tier** is derived from
  their memberships (Admin = 2, Manager = 1, everything else = 0), and tier
  comparisons are used to keep lower tiers from managing or escalating above
  their own.
- Admin is **Admin-group membership**, not a hidden boolean. The seeded
  `admin` account is a member of the Admin group.
- The effective context (flags, whitelist, ceiling, tier) is **recomputed from
  the database on every request**, so a permission change takes effect on the
  very next request — never on re-login.

The original three-tier `role` column, per-user CPU/RAM quota columns, the
host dedicated/shared-pool capacity fields, and template `allocation_mode` were
all removed. The only remaining limits are instance counts: a per-user
effective ceiling and the global host ceiling.

## Core concepts

### Groups

A group is a policy container with:

- a **name** and description (names are cosmetic — identity is by `kind`);
- five **permission flags**: `can_create_template`, `can_manage_users`,
  `can_manage_group_instances`, `can_manage_docker`, `can_manage_registry`;
- a group **`max_instances`** ceiling (NULL/0 = unlimited);
- a **template whitelist** (the set of templates members may launch).

Groups are one flat tier: a group cannot contain another group.

### Users and memberships

- A user may belong to any number of groups.
- A user may have a personal **`direct_max_instances`** override. It can only
  *raise* the user's effective ceiling above their groups' maxima, never lower
  it — the effective ceiling is the **maximum** of the personal value and every
  group ceiling.
- New accounts are placed in the **User** group by default.

### Effective context

On every request the API resolves a user's **effective context**:

- **Flags** are OR-ed across all the user's groups.
- **Whitelist** is the union of every member group's whitelist — group-only
  authorization, with no personal whitelist and no "creator always has access"
  exception.
- **Effective instance ceiling** is the maximum of the personal ceiling and
  every group ceiling, where `0`/NULL = unlimited is the highest value.
- **Tier and admin status** are derived from group memberships (see below).

The pure decision logic (context computation and the launch pre-flight) lives
in one module with no database or Docker dependencies, so the whole policy
engine is unit-testable in isolation.

## System groups

The migration seeds three system groups, identified by a machine `kind`
column: `admin`, `manager`, `user`. Custom groups have `kind = NULL`.
Group names are cosmetic; custom groups named "Admin"/"Manager"/"User" are
ordinary custom groups and are allowed.

| System group | `kind` | Permission flags | Default ceiling | Rules |
|---|---|---|---|---|
| **Admin** | `admin` | all five **TRUE**, fixed | unlimited (NULL) | cannot be renamed or deleted; flags fixed; ceiling editable by an admin |
| **Manager** | `manager` | all five **TRUE** on seed, editable | 2 | cannot be renamed or deleted; flags editable by an admin; ceiling editable |
| **User** | `user` | all five **FALSE**, fixed | 1 | cannot be renamed or deleted; flags fixed; ceiling editable |

Membership in the **Admin** group is what makes a user an admin (`is_admin`).
The seeded `admin` account is a member of it.

## Tiers and tier guardrails

A user's **tier** is the maximum kind tier across their group memberships:

| Membership | Tier |
|---|---|
| member of the Admin group | 2 |
| member of the Manager group | 1 |
| everything else (User group, custom groups, no groups) | 0 |

A user in both Manager and User groups is tier 1, and their ceiling is the
maximum of both groups' ceilings — the User group's ceiling of 1 never drags a
Manager down. Effective privileges always resolve to the **highest** available
value across memberships.

Tier guardrails are enforced on top of the flags, in the API layer:

| Action | Guardrail |
|---|---|
| Delete a user | actor's tier must be **strictly greater** than the target's (admin is exempt). Only an admin can delete an admin; a manager cannot delete a manager or an admin. |
| Write a user's policy (group memberships / personal ceiling) | actor's tier must be **strictly greater** than the target's (admin exempt). A non-admin can never write their own policy. |
| Assign a user to groups | the actor may place the target only into groups whose tier is **strictly below** the actor's — a manager cannot assign anyone to Manager/Admin groups, and an admin cannot assign anyone to the Admin group. |
| Control an instance owned by someone else | owner==self and admin always allowed; a group-instance holder may control instances owned by a **strictly lower** tier even when a group is shared. |
| Create/edit/delete groups | admin only. |

## Permission gates

| Area | Action | Requirement |
|---|---|---|
| **Instances** | Launch | effective whitelist + pre-flight (see below) |
| | View | everyone sees their own; admins see all; a group-instance holder also sees instances owned by same-group users of a strictly lower tier |
| | Control (start / stop / pause / unpause / delete) | owner, admin, or a group-instance holder whose target owner shares a group and is of a strictly lower tier |
| **Templates** | Browse the catalog | any authenticated user (a global browsable catalog) |
| | Create | `can_create_template` (or admin) |
| | Edit / delete | own templates for a `can_create_template` holder; any for an admin |
| | Launch | gated by the group whitelist + template visibility (see below) |
| **Users** | List / get | `can_manage_users` (admin included); a plain user may get their own record |
| | Create / edit / delete | `can_manage_users`, plus the tier guardrails above |
| | Reset own password | any user (identity/role changes to self require admin) |
| **Groups** | List | `can_manage_users` (admin included) |
| | Create / edit / delete | admin only |
| **Registry** | Manage (view / sync / set URL) | `can_manage_registry` (admin included) |
| **Docker** | Raw container surface (list / create) | `can_manage_docker` (admin included) |
| **Persistent volumes** | View orphans / thorough cleanup | `can_manage_users` (admin included), never scoped by group |
| **System settings** | Read / update (e.g. host ceiling) | admin only |

The `can_manage_*` flags never exceed the admin tier: an admin always passes
every flag gate.

## Launch authorization (pre-flight)

A launch attempt runs a short, ordered pre-flight. Every check is a fast
decision — no pool arithmetic, no locks beyond the one described below:

1. **Template visibility** — a `hidden` template is rejected for everyone,
   admins and owners included (absolute off-switch). A `public` template skips
   the whitelist check.
2. **Template whitelist** — for a `private` template (the default), the template
   must be in the user's effective whitelist. **No tier is exempt** — admins are
   authorized group-only too, so an empty whitelist is default-deny for every
   tier. Rejected with `403`.
3. **Per-user effective ceiling** — the user's active instance count must stay
   below their effective ceiling. Rejected with `409`.
4. **Global host ceiling** — the global active count must stay below
   `host_instance_limit` (0 = unlimited). This check runs for **every** tier,
   admins included, and admin instances still count toward it. Rejected with
   `409`.

An **active** instance is one in `running`, `starting`, or `paused` state;
`stopped` and `error` instances never count against a ceiling.

**Concurrency semantics:** the per-user ceiling is **exact**. The launch path
runs inside a transaction that takes a `SELECT … FOR UPDATE` on the single user
row, gathers that user's active count, runs the pre-flight, and only then
writes the reservation — concurrent rapid-fire launches from the same account
can never overshoot, and a single-row lock cannot deadlock. The global ceiling
is **best-effort**: a non-locking count read at check time, so racing launches
from *different* users may momentarily overshoot by one or two. This keeps
cross-user launches lock-free.

Rejections carry a structured body (a short message plus a `rejection` object
with a scope such as `template_not_allowed`, `user_instance`, or
`host_instance` and the relevant counts), so the client can render the exact
reason.

## Template authorization

Template access has exactly **one** mechanism: **group-only authorization**.

- The effective whitelist is the union of the whitelists of the user's groups
  (stored in the group ↔ template join table).
- A new template whitelists the **Admin group** by default (no other group), so
  it is immediately admin-usable. The migration backfilled the Admin group onto
  all existing templates so removing the old admin bypass did not cut off access.
- **Creators have no automatic access** to templates they built — including
  admins. Access is granted by whitelisting one of the user's groups on the
  template. The admin-facing control point is the group-management view's
  template whitelist editor.
- An admin can launch a template only when the Admin group (or another of their
  groups) is whitelisted on it, and can revoke it by editing the whitelist.

### Template visibility

A per-template `visibility` field sits above the group whitelist and applies
only to future launches:

- **`public`** — every authenticated user may launch it, whitelist skipped
  (ceilings still apply: public grants permission, not quota).
- **`private`** (default) — only users whose groups are whitelisted may launch.
- **`hidden`** — nobody may launch it: not whitelisted users, not the owner, not
  admins. It is an absolute off-switch; the owner/admin can edit it to bring the
  template back.

Hidden templates are also excluded from the effective whitelist the API returns,
so clients can treat the whitelist as "everything I may launch".

## Authentication and session

- Login returns the user's effective context; the dashboard reads it from the
  **`/auth/me`** endpoint, which recomputes and returns the context.
- The **JWT carries only identity** (user id and expiry) — no role or permission
  claims. A stale token can never outlive a permission change because every
  request resolves the effective context fresh from the database.
- The session cookie is `ow_token`; logout clears it. A `change-password`
  endpoint lets any user rotate their own password.

## Persistent volumes (data safety)

The persistent-volume registry (see [Persistent Storage](persistent-storage.md))
interacts with RBAC:

- Every persistent launch upserts a registry row keyed by the resolved host
  path; the row is `active` while at least one active instance references it
  and flips to **`orphaned`** when the last referencing instance is deleted.
  Deleting a user nulls the row's owner rather than deleting it.
- The platform **never auto-deletes** persistent data. The only destructive
  path is the manual, **double-confirmed "thorough cleanup"**: available to
  admins and `can_manage_users` holders, for any orphaned volume regardless of
  group; it empties the host directory, removes the Docker volume, and deletes
  the registry row. Still-referenced (`active`) volumes are not cleanable.

## Admin account

- Seeded at first startup with username `admin` and the password from
  `ADMIN_PASSWORD`; seeding adds the account to the Admin group (or is a no-op
  once an admin-group member exists).
- Admin is **Admin-group membership** everywhere: permission gates, tier
  guardrails (admins are exempt from tier comparisons), and the `is_admin`
  flag reported by the API.
- An admin is still subject to the **global host ceiling** and to **template
  authorization** — admin status is a management override, not a launch bypass.

## Migration path

Upgrades from the legacy three-tier model were performed by the flat-RBAC
migrations:

- `role` was dropped from users; former `admin` accounts became Admin-group
  members and former `manager` accounts were moved into the Managers group
  (renamed to **Manager** on upgrade). Plain user-role accounts were left
  without a group; fresh accounts default into the User group.
- `users.instance_limit` was copied into `direct_max_instances`; the CPU/RAM
  quota columns (`instance_limit`, `max_cpu_cores`, `max_ram_bytes`), the
  template `allocation_mode` column, and the host-capacity fields in
  `system_settings` were dropped. The only remaining global knob is
  `host_instance_limit`.
- The per-user template whitelist (`user_templates`) and the `is_system_admin`
  boolean were dropped once the Admin group and the group-only whitelist were
  in place; the Admin group was backfilled onto every existing template.

## Related docs

- [Persistent Storage](persistent-storage.md) — volume lifecycle and the orphaned-volume cleanup
- [System Architecture](architecture.md) — instance lifecycle, routing, DB schema
- [API Reference](api-reference.md) — endpoint payloads and auth headers
