Status: ready-for-agent

# Group & User Resource Quotas (CPU / Memory / GPU)

## Problem Statement

Today the only resource governance on the platform is instance *count*: a group's
`max_instances` ceiling, an optional personal `direct_max_instances` override,
and the host-wide `host_instance_limit`. There is no upper bound on how much
CPU, memory, or GPU a single member (or a whole group, or everyone together)
can claim. One developer can launch enough cores/RAM to starve every other
workspace and the control plane itself.

The number semantics are also inconsistent: `0` means "unlimited" in some
places (`host_instance_limit`, template bandwidth), `NULL` means "inherit" in
others (`direct_max_instances`), and `NULL` means "unlimited" in the Admin
group's `max_instances`. Neither is discoverable, and no field can express a
positive hard block.

The platform needs per-group resource pools with per-member personal caps,
explicit attribution of every instance to a billing group, and host-level
CPU/memory/GPU guards — all with one consistent numeric convention.

## Solution

Add CPU / memory / GPU quotas at three levels: **group pools** (a shared budget
drawn by instances attributed to that group), **per-member quotas** (how much of
that pool one member may personally hold — a per-group anti-hoarding control),
and **host caps** (best-effort whole-host guards). Every instance is explicitly
attributed to a **billing group** at launch and carries a **resource snapshot**,
so accounting never depends on later template edits.

A single numeric convention governs every numeric limit field: **`0` = blocked,
`>0` = the actual value, `-1` = unlimited**. Numeric config columns become
`NOT NULL` (no `NULL` for numbers).

Enforcement runs on a pre-flight pipeline that every launch and every restart
of a stopped instance walks, in a fixed order: hidden template → whitelist →
personal instance ceiling → personal resource cap (the chosen group's
membership quota) → chosen group pool → host instance limit → host resource
caps. The personal and pool layers are **precise** (row locks); the host layers
are **best-effort** (no global lock, so launches in different groups never
serialize against each other). An instance with `-1` on a template resource is
treated as *consuming the whole layer*: it is only launchable where that
resource's limit is also `-1`.

On upgrade, all existing data migrates to `-1` (unlimited), so existing
deployments keep working untouched; only newly created groups / memberships
default to `0` (blocked until an admin assigns quotas).

## User Stories

1. As an **instance user**, I want to launch an instance into a group whose
   pool has headroom, so that my request is accepted when my group still has
   budget.
2. As an **instance user**, I want my personal cap to come from the **group I
   attribute the instance to**, so that "you spend whose pool, you obey whose
   cap" — joining a high-cap group never disables a lower group's personal
   anti-hoarding rule.
3. As an **instance user**, I want to pick the billing group when I belong to
   more than one group, so that I can choose which pool my instance draws from
   (e.g. the group whose cap lets me run my big job).
4. As an **instance user**, I want the launch form to default the billing group
   to my highest-cap membership, so that I do not have to think about it when I
   do not care.
5. As an **instance user** in exactly one group, I want the API to attribute my
   instance to that group automatically when I do not specify one, so that the
   common case needs no extra input.
6. As an **instance user** in multiple groups, I want a launch without a
   billing group to be rejected with a clear `400`, so that I am never silently
   billed to an arbitrary pool.
7. As an **instance user**, I want a launch that names a group I do not belong
   to to be rejected with `403`, so that I cannot spend another group's pool.
8. As an **instance user**, I want the quota rejection to carry the current
   usage, the limit, and the resource (CPU/memory/GPU), so that I know exactly
   why my launch failed and what to free up.
9. As an **instance user**, I want my launch to be refused when the chosen
   group's pool is full (even though my personal cap is fine), so that the pool
   is a real shared budget.
10. As an **instance user**, I want the pre-flight to run again when I restart
    a stopped instance, so that a restart cannot bypass the limits a fresh
    launch would hit.
11. As an **instance user**, I want a paused instance to keep counting against
    my quota and the group pool, so that pausing cannot be used to reserve
    resources without paying for them.
12. As an **instance user**, I want a stopped or deleted instance to release
    its share of my personal cap and the group pool, so that I can reuse my
    allowance.
13. As an **instance user**, I want my instance's billing group and resource
    usage to be visible on the instance, so that I can see which pool is paying
    for it.
14. As an **instance user**, I want a template that asks for `-1` on a resource
    to only launch into a group/member/host configuration where that resource
    is also unlimited, so that an "unlimited" request is understood to consume
    the whole layer and cannot silently overshoot.
15. As a **group admin**, I want to set my group's CPU/memory/GPU pool quota
    (`0` = blocked, `-1` = unlimited), so that the group's members share one
    budget.
16. As a **group admin**, I want a newly created group to default to `0`
    (blocked) on every resource, so that no instance is launched into an
    ungoverned pool by accident.
17. As a **group admin**, I want to see every member's per-membership quota
    alongside the pool, so that I can understand who can draw how much.
18. As a **group admin**, I want to be blocked from lowering the pool below an
    existing member's finite quota, so that the invariant
    `0 ≤ member quota ≤ pool` never breaks.
19. As a **workspace manager**, I want to adjust the resource quota of a member
    in a group whose tier is below mine (reusing the existing user-management
    permission and tier rules), so that day-to-day quota assignment does not
    require an admin.
20. As a **workspace manager**, I want to be forbidden from editing the quota of
    a member at my tier or above, so that quota assignment cannot escalate
    privilege.
21. As a **workspace manager**, I want to be forbidden from setting a member's
    quota above the group pool, so that a member can never be granted more than
    the pool can hold.
22. As an **infrastructure admin**, I want host-wide CPU/memory/GPU caps in the
    admin settings, so that I can guard the physical box as a last line of
    defense.
23. As an **infrastructure admin**, I want the host caps to be best-effort
    (never a global lock), so that concurrent launches in different groups do
    not serialize against a single host-wide lock.
24. As an **infrastructure admin**, I want every existing limit field to keep
    its current meaning after upgrade (`0`-means-unlimited → `-1`, `NULL` →
    `-1` where that is behavior-preserving), so that upgrading the platform
    never changes who can do what.
25. As an **infrastructure admin**, I want the upgrade migration to backfill
    existing groups and memberships with `-1` (unlimited), so that the upgrade
    does not lock the whole platform and force re-assignment.
26. As an **infrastructure admin**, I want the migration to backfill every
    existing instance with a billing group (its owner's highest-tier
    membership) and a resource snapshot, so that pre-upgrade instances remain
    accounted after upgrade.
27. As an **API consumer**, I want the launch request to accept an optional
    billing `group_id`, so that clients can pass or omit it per the
    single/multi-group rules.
28. As an **API consumer**, I want every quota rejection to keep the existing
    structured `{ scope, current, limit, requested }` shape with new resource
    scopes, so that I need no new error branch.
29. As a **platform developer**, I want the quota decision logic to stay one
    pure function behind one rejection enum, taking a bundled context struct,
    so that it is unit-testable without a database or Docker and the check
    order stays deterministic.
30. As a **platform developer**, I want the counter queries to use
    `COALESCE(SUM(...), 0)` over the active status set
    (`starting`/`running`/`paused`), so that an empty set never surfaces a
    `NULL` that corrupts the comparison.

## Implementation Decisions

### 1. Numeric-limit convention (`0` / `-1`) and the migration

- Every numeric *limit / config* column follows: **`0` = blocked, `>0` = the
  value, `-1` = unlimited**, and is `NOT NULL`. Strings and identity columns
  (`groups.kind`, `description`, FK references) keep `NULL` where they already
  use it.
- The upgrade migration rewrites existing data behavior-preservingly:
  - `workspace_templates.cores`, `memory`, `network_bandwidth_up_mbps`,
    `network_bandwidth_down_mbps`: `0` → `-1` (today `0` = no limit).
  - `workspace_templates.max_run_seconds`, `keep_time_seconds`:
    `NULL` → `-1`, then the column becomes `NOT NULL DEFAULT -1`
    (`-1` = feature disabled). The auto-sleep / keep-time logic treats `-1`
    exactly as it treats `NULL` today.
  - `groups.max_instances`: `NULL` → `-1`, then `NOT NULL DEFAULT -1`
    (the Admin group's "unlimited" survives as `-1`).
  - `system_settings.host_instance_limit`: `0` → `-1` (unlimited).
  - `users.direct_max_instances`: `0` → `-1`. **This column stays nullable** —
    see Decision 2. `workspace_templates.gpu_count` is unchanged: `0` = zero
    GPUs is a legitimate request, not a "blocked" sentinel.
- New rows use the schema default: `0` (blocked) for new groups and new
  memberships; `-1` (unlimited) for new host caps.

### 2. `users.direct_max_instances` — deliberate NULL carve-out

- `NULL` on this column today means **"inherit the group ceiling"** (the common
  case), while `0` means unlimited. Under the new convention `-1` means
  unlimited, so backfilling `NULL → -1` would silently make every user's
  effective ceiling unlimited and **disable every group ceiling on upgrade**.
- Therefore `direct_max_instances` remains nullable: `NULL` = inherit groups,
  and the value follows the convention when set (`0` = blocked,
  `-1` = unlimited, `>0` = finite). This is the single documented exception to
  "numeric columns are `NOT NULL`", justified by the `NULL` being a
  semantically meaningful "inherit" state rather than an unset value.
- The effective-ceiling computation (the existing union/max over memberships)
  keeps treating `NULL` as "no direct override" and `-1` as unlimited.

### 3. New schema

- `groups` gains `cpu_quota INTEGER NOT NULL DEFAULT 0`,
  `memory_quota BIGINT NOT NULL DEFAULT 0`, `gpu_quota INTEGER NOT NULL
  DEFAULT 0`. Migration: all existing groups (system groups included) → `-1`.
  Group pool quotas are editable for system groups just like `max_instances`
  is today.
- `user_groups` (membership rows) gains `cpu_quota INTEGER NOT NULL DEFAULT 0`,
  `memory_quota BIGINT NOT NULL DEFAULT 0`, `gpu_quota INTEGER NOT NULL
  DEFAULT 0`. Migration: all existing memberships → `-1`. New memberships
  default to `0` (blocked until a manager/admin assigns quota).
- `system_settings` gains `host_cpu_quota INTEGER NOT NULL DEFAULT -1`,
  `host_memory_quota BIGINT NOT NULL DEFAULT -1`, `host_gpu_quota INTEGER NOT
  NULL DEFAULT -1`. The existing singleton row is backfilled to `-1`.
- `workspace_instances` gains:
  - `owner_group_id UUID NULL REFERENCES groups(id) ON DELETE SET NULL` — the
    billing group. Set on every new launch (application-enforced non-null);
    nullable at rest only so the backfill of a corner-case owner with zero
    memberships cannot fail the migration, and so a deleted group nulls its
    stopped instances' attribution (Decision 7) instead of blocking deletion.
  - `resource_cores INTEGER NOT NULL`, `resource_memory BIGINT NOT NULL`,
    `resource_gpu INTEGER NOT NULL DEFAULT 0` — the resource snapshot taken
    from the template at launch. Accounting always reads the snapshot, never
    the live template, so editing a template never rewrites history.
  - Migration backfill: `owner_group_id` = the owner's highest-tier membership
    (admin > manager > user), tie-broken by oldest membership; snapshot =
    template's current `cores` / `memory` / `gpu_count`. Instances with no
    owner membership stay `NULL` (they count against no pool; harmless while
    pools are `-1`).

### 4. Billing-group attribution contract

- `LaunchInstanceRequest` gains `group_id: Option<Uuid>`:
  - **Present** → must be one of the caller's memberships, else `403`. No
    membership row with that group → `403` (the caller does not belong).
  - **Absent** → exactly one membership: attribute to it automatically. More
    than one membership: `400` asking the caller to choose a billing group.
  - **Zero memberships** → `403` whether or not `group_id` is sent (a
    non-membership `group_id` is `403` anyway): the caller belongs to no
    billing group. Reachable via public templates even though the whitelist
    already default-denies private ones — never an `unwrap`/`500`.
- The chosen `group_id` is stored as `owner_group_id` on the instance row at
  reservation time, together with the template resource snapshot. Attribution
  is immutable for the lifetime of the instance (no re-attribution endpoint).
- The read-time `owner_group_ids` field already computed from the owner's
  current memberships (used for instance-control scoping) is unchanged and
  orthogonal to the stored billing group. The instance JSON additionally
  exposes `owner_group_id` and the snapshot so the UI can render the payer.

### 5. Pre-flight pipeline (single entry, fixed order, unified rejection)

- `pre_flight` (the existing pure function) grows a bundled
  `QuotaContext` parameter instead of a long argument list:
  - `selected_group_quota` (pool cpu/memory/gpu), `selected_group_usage`
    (active sums attributed to that group), `selected_group_member_quota` (the
    caller's membership cap in that group), `selected_group_member_usage`
    (the caller's own active instances attributed to that group), host
    `quota` + `usage`, and the template request resources.
- The check order is fixed:
  1. Hidden template → `403` (unchanged).
  2. Whitelist (private template not allowed) → `403` (unchanged).
  3. Personal instance ceiling (all active instances the user owns) →
     `409` (unchanged, precise under the user-row lock).
  4. **Personal resource cap** = the chosen group's membership quota; usage =
     the caller's active instances attributed to that group → `409` (precise
     under the user-row lock).
  5. **Chosen group pool**; usage = all active instances attributed to that
     group → `409` (precise under the group-row lock).
  6. Host instance limit → `409` (best-effort, unchanged).
  7. **Host resource caps**; usage = all active instances → `409`
     (best-effort).
- `0` at any layer blocks every finite launch into that layer. `-1` skips the
  layer.
- **`-1` request rule**: per resource, a template request of `-1` (unlimited)
  is only accepted when that resource's limit is `-1` at every checked layer
  (member cap, group pool, host cap); otherwise the launch is rejected at the
  first non-`-1` layer with `current`/`limit`/`requested` populated. A finite
  request is compared as `usage + requested ≤ limit`.
- Restart of a stopped instance runs the same pipeline (no instance-count
  self-increment problem: the restarting instance is `stopped` and not yet
  counted when the check runs). Unpause runs nothing — a paused instance keeps
  holding its reservation.
- Restarting an instance whose `owner_group_id` is `NULL` (a legacy backfill
  corner case, or a group deleted after its instances stopped) first
  **re-attributes** it to the owner's highest-tier membership (admin >
  manager > user, tie-broken by oldest membership) and **persists** the new
  value, then runs the pipeline against that group. If the owner still has no
  membership, the restart is rejected with `403` ("not in any billing group").
  This keeps every active instance attributable and makes deleting a group
  that only has stopped instances safe (Decision 7).

### 6. Concurrency and locking

- The activation transaction locks **only** the chosen `groups` row and the
  owner's `users` row, in that order (group = ancestor, then user = descendant;
  one group per transaction, so no lock cycle is possible). This serializes
  (a) the same user's launches and (b) launches into the same group pool, while
  launches into **different groups run fully in parallel**.
- `system_settings` is **not** `FOR UPDATE`-locked. The host-instance and
  host-resource checks read the singleton plus an aggregate inside the
  transaction's snapshot; overshoot is possible under concurrency and is
  accepted (best-effort), exactly like `host_instance_limit` behaves today.
- Counter queries use `COALESCE(SUM(resource), 0)` over
  `status IN ('starting','running','paused')`, with a **negative-filtered**
  sum — `COALESCE(SUM(CASE WHEN resource > 0 THEN resource ELSE 0 END), 0)`.
  A `-1` snapshot is stored literally on the instance row, and summing it as a
  negative would *reduce* usage (ten unlimited instances would make a finite
  pool look negative and even admit new launches), so negative snapshots
  contribute nothing to the sums. Paused counts for CPU, memory, **and** GPU —
  counting it everywhere prevents pause-based quota bypass and keeps one
  status set for the whole feature. `stopped` and `error` never count.

### 7. Quota editing surface and permissions

- New endpoint `PUT /api/groups/{id}/members/{user_id}/quota` with body
  `{ cpu_quota, memory_quota, gpu_quota }`. Gates:
  - actor has `can_manage_users` (admin or manager);
  - `mayManageUser(target)` — target user tier strictly below the actor's tier;
  - the membership's group tier is strictly below the actor's tier (so a
    manager cannot inflate a quota inside the Manager or Admin group).
- Validation: each value is `-1` or in `[0, group_pool]` when the pool is
  finite, or `≥ 0` (plus `-1`) when the pool is unlimited. A membership at
  `-1` is bounded only by the pool (Decision: "member `-1`" = the pool is their
  cap).
- Group pool updates (existing admin `PUT /api/groups/{id}` extended with the
  quota fields) are rejected with `409` when the new pool is finite and below
  any existing member's finite quota — the invariant
  `0 ≤ member quota ≤ pool` must never break.
- **Whole-layer consumers block tightening**: lowering any quota (a group
  pool, a member cap, or a host cap) to a *finite* value is rejected with
  `409` while an active instance with a `-1` snapshot on that resource is
  attributed in that scope. Without this, a `-1` instance (launched legally
  while the layer was unlimited) would silently stop counting once the layer
  goes finite, letting new finite launches pass beside a whole-layer consumer.
  The admin must stop those instances before imposing limits.
- Deleting a group is rejected with `409` while any active instance
  (`starting`/`running`/`paused`) is attributed to it. The `owner_group_id`
  foreign key is `ON DELETE SET NULL`, so a group holding only stopped
  instances is deletable; those instances become `NULL`-attributed and are
  re-attributed on their next restart (Decision 5).
- The group-list response gains each group's members (id, username, tier,
  per-membership quotas) so the frontend can render the layered Groups tab
  without extra round-trips. Quota changes emit audit events (target group,
  redacted before/after values) on the existing audit channel.

### 8. Admin settings

- The admin settings API and the Settings tab gain `host_cpu_quota`,
  `host_memory_quota`, `host_gpu_quota` (same `0`/`-1`/value semantics, best
  effort). No host-capacity auto-detection; the admin sets numbers.
- `host_instance_limit` keeps its semantics but `0` (unlimited today) becomes
  `-1`; the settings UI presents the `-1`/`0`/value tri-state uniformly.

### 9. Frontend

- **Groups tab** is opened to `can_manage_users` (layered): managers see the
  group list, each group's members, and the per-member quota editor
  (tier-scoped by the same rules as the API). Group entity editing — name,
  flags, whitelist, pool quotas, create/delete — stays admin-only. The pool
  quota inputs render only for admins.
- **Launch UI**: a billing-group picker appears only for multi-group users,
  defaulting to the highest-cap membership (tie-break by group tier, then
  name). Single-group users see nothing and the request omits `group_id`.
- **Template form**: cores / memory / bandwidth / auto-sleep / keep-time get
  the `-1` = unlimited semantics (an explicit unlimited toggle), `0` = blocked
  is reserved for quota fields and not offered as a template resource value;
  `gpu_count` remains `0..N` with an unlimited (`-1`) option.
- **User policy dialog**: `direct_max_instances` becomes a `-1` / `0` /
  positive tri-state.
- **Rejection rendering**: `PreflightRejectionScope` grows the new scopes
  (`user_cpu|memory|gpu`, `group_cpu|memory|gpu`, `host_cpu|memory|gpu`) and
  `preflight.ts` gains copy for each; the `RejectionNotice` keeps working
  unchanged on the `{ scope, current, limit, requested }` shape. Memory scopes
  render their three numbers through the existing byte-format helper (readable
  GiB values, never raw bytes).
- **Pool editor UX**: when an admin lowers the pool below a member's finite
  quota (or to `0`), the blocked `409` is explained in the form, with a
  one-click "reset all member quotas to `0`" action (a batched quota update)
  so a group can be blocked in one step.

### 10. Rejection contract

- All quota/count rejections are `409`; permission violations (`403` for
  hidden / whitelist / non-member billing group / tier guard) keep their codes.
- The body keeps the existing shape
  `{ "error": "...", "rejection": { "scope", "current", "limit", "requested" } }`;
  new scopes encode the resource in the scope string
  (`user_cpu`, `group_memory`, `host_gpu`, …), so no shape change is needed.
  `current` and `limit` carry the resource's unit (whole cores / bytes /
  GPU count) and `requested` the launch request's value.

### 11. Module shape (seams)

- **Pure decision** stays in the effective-context module: `pre_flight` (one
  entry) plus a private `check_resource_quotas(&input, &quota_context)` helper
  over a `ResourceQuota`/`ResourceUsage` pair of small structs — the deep
  module, no DB/Docker. Unit tests drive it directly.
- **Counter queries** are new repository methods (active per-user count,
  active host count, active sums by billing group, by owner+billing group,
  and host-wide) that all use the `COALESCE(SUM(...), 0)` + active-status-set
  rule.
- **Activation** extends the existing transactional helper: lock chosen group →
  lock user → gather counters → run `pre_flight` → reserve the instance as
  `starting` with `owner_group_id` + snapshot → commit → Docker build.
- **Routes/UI** layer on top; the frontend reuses the existing
  rejection-notice path.

## Testing Decisions

- Only external behavior is tested: a rejected launch returns the right code
  and body and leaves **no** instance row; an accepted launch reserves and then
  builds. No assertion on internal call counts.
- **Pure decision — unit tests, no DB/Docker** (prior art: the existing
  `pre_flight` unit tests in the effective-context module): every scope's
  threshold boundary (at-limit vs over-limit), `0` = blocked at every layer,
  `-1` = skip, the `-1` request-eats-the-whole-layer rule against finite
  member/pool/host limits, the fixed check order, and usage-added-to-request
  arithmetic per resource.
- **Counter queries — integration tests against the Postgres test container**
  (prior art: `tests/common/pg.rs`): `COALESCE` on an empty set returns `0`;
  the **negative-filtered** sum treats `-1` snapshots as contributing nothing
  (never negative usage); sums attribute by billing group and by
  owner+billing group correctly; `starting`/`running`/`paused` all count;
  `stopped`/`error` never count.
- **Transactional concurrency** (prior art: `instances_mock_test.rs` /
  two-process flock test): two concurrent launches into the same group at the
  pool limit → exactly one succeeds; two concurrent launches into **different**
  groups at the host limit → both pass the precise layers (no global
  serialization), with host best-effort documented as racy; the same user's
  launches serialize on the user row.
- **Route-level behavior** (`instances_mock_test.rs` with a mocked
  `DockerService`): non-member billing group → `403`; multi-group launch
  without `group_id` → `400`; single-group launch without `group_id` defaults;
  **zero-membership launch → `403` whether or not `group_id` is sent**; each
  `409` carries the right scope + numbers and leaves no row; the reservation
  stores `owner_group_id` + the resource snapshot; a restart of a stopped
  instance re-runs the pipeline and stays `stopped` on rejection.
- **Quota editing — route tests**: manager edits a lower-tier member in an
  assignable group → `200`; same/higher-tier member or higher-tier group →
  `403`; value above the pool → `400`; admin lowering the pool below a member's
  finite quota → `409`; **lowering any quota to a finite value while an active
  `-1`-snapshot instance is in scope → `409`**; system groups keep working with
  `-1` defaults.
- **Group deletion — route tests**: `409` while an active instance is
  attributed; success (billing group nulled) when only stopped instances
  remain; the nulled instance re-attributes on its next restart.
- **Restart of a `NULL`-attributed instance — route tests**: re-attributes to
  the owner's highest-tier membership and persists the new value; `403` when
  the owner has no membership.
- **Migration — Postgres harness**: after the upgrade chain, existing
  `0`-unlimited and `NULL` values read back as `-1` where decided; existing
  groups/memberships are `-1` and new rows default to `0`; existing instances
  get `owner_group_id` (highest-tier membership) and the template snapshot;
  `direct_max_instances` keeps `NULL` untouched.
- **Frontend — Vitest**: `preflight.ts` copy for every new scope;
  `preflight.test.ts` extends the scope union; component tests for the
  billing-group picker (hidden single-group / default multi-group), the
  layered Groups-tab member-quota editor (manager vs admin visibility), the
  template form `-1` toggles, the admin-settings tri-state inputs, and the user
  policy dialog's `direct_max_instances` tri-state.

## Out of Scope

- **GPU types**: quotas count GPUs only; per-type GPU allocation remains a
  roadmap idea.
- **Host-capacity auto-detection** (the phase-1 `docker info` seeding is not
  revived): the admin types host caps.
- **Quota usage dashboard** (no persistent "remaining quota" display): the
  user sees the 409 rejection notice only.
- **Dynamic reconciliation** of containers that died outside the API: the
  existing health / keep-time workers remain the only reclaim mechanisms.
- **Re-attribution**: an instance's billing group is fixed at launch; no
  endpoint moves an instance between groups.
- **Dedicated / overcommit allocation modes** (the dismantled phase-1
  `allocation_mode`) are not revived.
- **Live template editing against running accounting**: the snapshot makes
  edits safe; re-billing a running instance for a template change is out of
  scope.

## Further Notes

- The predecessor of this feature — per-user role quotas, `allocation_mode`,
  and host capacity seeding — was built and then dismantled with the flat-RBAC
  migration. Its closed spec lives at `.scratch/archive/resource-quota/spec.md`
  and is the reference for what not to rebuild.
- Units mirror the template model: `cores` and `gpu_count` are whole counts
  (`i32`), `memory` is bytes (`i64`); quota columns use the same units.
- The effective-ceiling computation and the resource checks share the MAX-over
  groups precedent only in spirit: instance ceilings keep their existing
  union/max semantics, while resource personal caps are strictly per chosen
  group (Decision: "spend whose pool, obey whose cap").
- The `-1` request rule interacts with `gpu_count = 0`: requesting zero GPUs is
  a finite, zero-cost request and is always fine; only `-1` requests consume a
  whole layer.
