# 01 — Backend: resource quotas, attribution, and the `-1` convention

**Track:** backend

**What to build:** the entire server-side of the CPU / memory / GPU quota
feature (spec `.scratch/resource-quota-v2/spec.md`), **plus the codebase-wide
flip of the "unlimited" sentinel from `0` to `-1`** so that `0` means a real
zero everywhere. After this ticket lands, a launch or restart is accepted only
when it fits the caller's personal cap in the chosen billing group, that
group's pool, and the host caps; every instance records who pays for it
(`owner_group_id`) and what it asked for (resource snapshot); admins/managers
can set group pools, per-member quotas, and host caps through the API; and
every existing numeric limit field — template resources/bandwidth/auto-sleep/
keep-time, group ceilings, the host instance ceiling, personal ceilings — uses
`-1` as its unlimited/disabled sentinel with `0` meaning a real value (blocked
for quota fields), existing data migrated behavior-preservingly so upgrades do
not lock anyone out.

> **Scope note (revision):** the runtime currently treats `0` as "unlimited"
> (`effective_context.rs` ceiling/pool resolution and pre-flight gates,
> template/group/admin validation `>= 0`, docker limit derivation `> 0`).
> This ticket flips all of it to `-1` = unlimited / `0` = real zero so the
> shipped code matches the spec's Decision 1 instead of the legacy convention.

**Blocked by:** None — can start immediately

**Status:** completed (2026-09-11) — slice-F amendment closed: `security/openapi.json`
`GroupBilling` schema hand-synced to the `9eb032b` struct (new `group_name` /
`tier` / `member_*` fields; properties alphabetical, `required` in declaration
order, matching the utoipa pattern of the neighboring schemas). The drift guard
(`committed_spec_is_in_sync`) compares parsed JSON values, so the user must
confirm with `cargo run --bin export_openapi` (no diff expected) + `bash
scripts/check.sh` + `bash scripts/run_tests.sh` — the agent shell cannot run
cargo in this environment. Prior green state (736/736) is the "Revision round
— 2026-08-10" section below.

## Slice F amendment — 2026-08-10 (picker contract)

> The 02 ticket's billing-group picker needs per-group display name, tier, and
> the user's own member cap — none of which the shipped `GroupBilling` carried.

### Done (committed `9eb032b` on `feature/resource-quotas`)

- `apps/api/src/effective_context.rs`:
  - `GroupPolicy` gains `name: String` and the user's per-membership cap
    (`member_cpu_cores` / `member_memory_mb` / `member_gpu_count`, `-1` =
    unlimited, `0` = blocked).
  - `GroupBilling` (serialized on `/auth/me` context) gains `group_name`,
    `tier` (via `group_kind_tier`), and `member_cpu_cores` / `member_memory_mb`
    / `member_gpu_count`.
  - Test helper `group()` defaults the new fields; `group_billing_surfaces_each_group_pool`
    now asserts name / tier / member cap.
- `apps/api/src/db.rs` `load_effective_context`: builds a `member_quotas`
  map from the already-loaded `user_group` membership rows and feeds each
  `GroupPolicy` its `name` + per-membership quotas.

### Remaining (for the user to run — agent shell blocks cargo)

- [x] Regenerate `apps/api/security/openapi.json` — hand-synced 2026-09-11
      (see Status); confirm with `cargo run --bin export_openapi` from
      `apps/api` (expect no diff).
- [ ] Re-run `bash scripts/check.sh` (expected silent) and
      `bash scripts/run_tests.sh` (expect 736+ / green).
- [x] Ticket 02's picker consumes the new fields (slice F web work — landed
      in `25431f4`, verified in code 2026-09-11).

### Gate state (before spec regen)

- `bash scripts/check.sh` — **silent** (both feature sets) on `9eb032b`.
- `bash scripts/run_tests.sh` — `committed_spec_is_in_sync` fails (stale spec);
  everything else compiles and the pure tests pass. Not yet re-run green.


## Revision round — 2026-08-10 (billing attribution + member-quota editing)

> **Status update (2026-08-10, closed out):** the revision round on top of the
> completed base delivery is **green**. `check.sh` is silent (zero warnings,
> both feature sets). `run_tests.sh` is **736/736 passed** (was RED at 18
> `instances_mock_test` failures; the full list is in "In flight" below).
> Ticket 01 is **complete**, including the two concurrency tests (`8b61109`)
> that close the final deferred acceptance criterion.

### Done (committed on `feature/resource-quotas`)

- `a9f8d34` — feat(api): per-member quotas, pool-tightening guards, billing
  auto-attribute (6 files, +883/-26):
  - New endpoint `PUT /api/groups/{id}/members/{user_id}/quota`
    (groups.rs:29, handler ~550) — per-member quota editing, gated by
    `can_manage_users` + target-user tier + group tier strictly below the
    actor; values `-1` / within `[0, group_pool]`; audit `GROUP_QUOTA_CHANGE`.
  - Pool-tightening 409 guards: lowering a pool below a member's finite quota
    → 409; lowering any quota to a finite value while an active `-1`-snapshot
    instance is in scope → 409; deleting a group with an active attributed
    instance → 409.
  - Billing-group resolution at launch (instances.rs:834-871): named group not
    in `context.group_ids` → 403; absent with exactly one membership →
    auto-attribute; absent with several memberships → 400 "You belong to
    several groups; choose a billing group"; zero memberships → 403.
  - `owner_group_id` + resource snapshot stored at launch and exposed in the
    instance JSON (instances.rs:116); `GET /api/groups` now returns each
    group's members with per-membership quotas (`members` array).
  - Restart re-attribution (instances.rs:1326-1371): a `NULL`-attributed
    stopped instance is re-attributed to the owner's highest-tier membership
    (persisted) before pre-flight; no membership → 403.
- `a662d3b` — chore(api): regenerate OpenAPI spec (+45/-1: `GroupMemberSchema`
  + `GroupSchema.members`); `committed_spec_is_in_sync` green.
- `a22bb6b` + `5314a87` — fix(api): flat_rbac_e2e tests launch with an explicit
  billing group + member quota grants (added helpers `grant_member_quota` via
  the new PUT endpoint and `launch_plain_in_group`); `whitelist_group.clone()`
  fix. **`flat_rbac_e2e_test` now green.**
- `e4daf6d` — fix(api): instances mock tests launch with a single billing
  group. Root cause of the bulk of the 18 failures: `/api/users` auto-adds the
  seeded User system group when `group_ids` is omitted, so helper-created users
  landed in **2 groups** (User system + the helper's group) and a launch
  without `owner_group_id` hit the "several groups" 400; and plain
  `create_user_and_token` users ended in the User system group (membership
  quota 0) → 409 `member_quota_cpu`. Fix: `create_quota_user` now seeds
  `grp-<user>` first and creates the user scoped to exactly that group
  (explicit `group_ids`) with a `-1` membership; other helper users get a
  seeded single group too. Cleared most of the 18.
- `3e1580f` — fix(api): silence cloned-ref-to-slice-refs clippy in flat-rbac
  e2e (kept the gate silent).
- `4540a8c` — fix(api): pin seeded quota-test group ceiling to the direct
  limit. Root cause of the **remaining 5** (ceiling launch, start-rejected,
  admin-restart, concurrent-same-user, restart-reattribute):
  `create_quota_user`'s auto-attribution group inherited `seed_pool_group`'s
  hardcoded `max_instances = 4`, so `effective_max_instances = max(direct, 4)`
  and the user-instance ceiling gate **never fired** for direct=1 users — the
  ceiling tests were not exercising the gate at all. Fix pins the group ceiling
  to the direct ceiling so the effective limit is exactly
  `direct_max_instances`. Also corrected
  `test_restart_reattributes_null_attributed_instance`'s Docker mocks: a
  restart of an existing stopped container inspects and *starts* it
  (`create_container_from_template` is called once at launch, not twice).

### In flight (was the reason `run_tests.sh` was red — now resolved)

- **18 `instances_mock_test` failures**, all downstream of the billing
  auto-attribute change. Root cause: `/api/users` (users.rs:253-277) auto-adds
  the migration-seeded User system group as default membership when
  `group_ids` is omitted, so:
  - `create_quota_user` users end up in **2 groups** (User system + the helper's
    `grp-<username>`) → launch without `owner_group_id` → 400 "several groups".
  - plain `create_user_and_token` users end up in **1 group** (User system,
    membership quota 0) → launch auto-attributes to it → 409
    `member_quota_cpu`.
  - Before this round, omitting the billing group self-billed with no
    membership check, so these tests never exercised a billing resolution.
- Failing tests (full list from `--no-fail-fast`):
  `test_admin_restart_accounts_against_owner_quota` (2975),
  `test_concurrent_launches_same_user_at_ceiling_exactly_one_succeeds` (3090),
  `test_concurrent_launches_different_users_both_succeed` (3150),
  `test_concurrent_launches_allocate_distinct_subnets` (757),
  `test_gate_instance_lifecycle_same_group_scope` (3468),
  `test_gate_group_manager_list_includes_same_group_instances` (3525),
  `test_heartbeat_forbidden_for_non_owner` (1811),
  `test_host_resource_cap_blocks_second_launch` (4335),
  `test_launch_auto_attributes_single_group` (3749),
  `test_launch_persistent_conflict_is_per_owner` (2231),
  `test_launch_rejected_by_ceiling_returns_409_and_leaves_no_row` (2786),
  `test_launch_rejected_by_template_whitelist_returns_403` (2824),
  `test_launch_rejected_by_host_ceiling_returns_409` (3032),
  `test_restart_reattributes_null_attributed_instance` (3938),
  `test_restart_without_membership_rejected_403` (3974),
  `test_start_infra_failure_rolls_back_to_stopped` (2906),
  `test_start_rejected_by_ceiling_leaves_instance_stopped` (2862),
  `test_user_launch_infra_failure_marks_error_and_keeps_record` (2943).
- Resolution: `e4daf6d` fixed the bulk (single billing group per test user,
  see Done); the remaining 5 were a separate latent bug — the seeded group's
  hardcoded `max_instances = 4` lifted direct=1 users' effective ceiling to 4,
  so the user-instance gate never fired — fixed by `4540a8c` (group ceiling
  pinned to the direct ceiling; reattribute restart mocks corrected). After
  both, all 18 pass.

### Gate state

- `bash scripts/check.sh` — **silent** (zero warnings, both feature sets).
- `bash scripts/run_tests.sh` — **736/736 passed** (base 715/715 + the
  member-quota / pool-tightening / re-attribute tests added this round + the
  two concurrency tests added by `8b61109`).
- The final deferred acceptance criterion — same-pool serialization while
  different groups proceed in parallel — is now **covered** by dedicated
  tests (see the acceptance boxes below).

## Implementation audit — 2026-08-09 (working tree on `feature/resource-quotas`)

> **Status update (2026-08-09, after full-suite green):** all "Remaining /
> needs change" items below are **done** and committed on
> `feature/resource-quotas`; `bash scripts/check.sh` is silent (zero warnings,
> both feature sets) and `bash scripts/run_tests.sh` is **green: 715/715
> passed**. The full gate also uncovered and fixed three real bugs (below).
>
> **Follow-up:** the 2026-08-10 revision round (billing attribution +
> member-quota editing) landed **on top of** this audit and is tracked in the
> "Revision round — 2026-08-10" section above. The 715/715 result below is the
> base-delivery gate; the revision round's gate is now **734/734 green** (see
> above).

### Done (uncommitted working tree)

- Migration `apps/api/migration/src/m20260809_000026_minus_one_sentinel.rs` created + registered
  in `migration/src/lib.rs`: rewrites legacy sentinels → `-1` —
  `workspace_templates.cores/memory/network_bandwidth_up_mbps/network_bandwidth_down_mbps`
  `0 → -1` (gpu_count untouched); `max_run_seconds`/`keep_time_seconds` `NULL → -1` +
  `NOT NULL DEFAULT -1`; `groups.max_instances` `NULL|0 → -1` + `NOT NULL DEFAULT -1`;
  `groups.pool_*` `0 → -1` + `DEFAULT -1`; `system_settings.host_instance_limit` `0 → -1` +
  `DEFAULT -1`; `system_settings.host_*` caps `0 → -1` + `DEFAULT -1`;
  `users.direct_max_instances` `0 → -1` (stays nullable). `down()` reverts defaults.
- `src/db.rs`: template entity/repo `max_run_seconds`/`keep_time_seconds` `Option<i64> → i64`;
  group pool doc → `-1`; create/update signatures take `i64`.
- `src/system_settings.rs`: host-cap docs `-1` = unlimited / `0` = blocked.
- `src/routes/workspace/instances.rs`: deadline helpers take `i64`, enable on `> 0`; instance JSON
  emits `keep_time_seconds: -1` when disabled; list/get `unwrap_or(-1)`.
- `src/routes/workspace/templates.rs`: resource validation `>= -1`; normalize `unwrap_or(-1)`.
- `src/health_worker.rs`: auto-sleep / keep-time skip on `<= 0`.
- `src/activation.rs`: resource sums clamp with `.max(0)` so `-1` snapshots contribute nothing;
  self-billed pool = `(-1,-1,-1)`.
- `src/effective_context.rs`: `-1` sentinel in context resolution + `pre_flight` resource gates
  (skip on `-1`, block on `0`, finite arithmetic, `-1` request rule, `0` = zero-cost request) +
  new unit tests (`pre_flight_host_resource_cap_*`, `pre_flight_zero_request_is_zero_cost`, …).
- `src/routes/admin_settings.rs` / `groups.rs` / `users.rs`: validation `>= -1`; group pool defaults
  `-1`; `direct_max_instances >= -1`.
- `src/docker.rs`: bandwidth doc `-1`/`0` = no cap. `src/openapi.rs`: `TemplateSchema` /
  `InstanceSchema` `i64`. `src/routes/monitor.rs`: `cpu_limit_percent`/`mem_limit_bytes` treat
  `-1`/`0` as unlimited.
- Test edits: `auth_test.rs` (context `effective_max_instances = -1`), `admin_settings_test.rs`
  (migration defaults `-1`, negative case `-2`), `db_test.rs` (create/update `-1` + asserts),
  `health_worker_test.rs` (`Some(3600)→3600`, `None→-1`), `instances_mock_test.rs`
  (`keep_time_seconds` null → `-1`), `monitor_test.rs` (`Set(None)` → `Set(-1)`),
  `templates_test.rs` (`is_null()` → `-1`), `flat_rbac_e2e_test.rs` (Admin `max_instances`
  `is_null` → `-1`, ~line 451).

### Remaining / needs change

- `tests/flat_rbac_e2e_test.rs`:
  - ~486 `assert_eq!(context["effective_max_instances"], 0)` → `-1` (Admin context is unlimited).
  - ~534 Admin-group PUT payload `"max_instances": 0` → `-1` (0 now = blocked; Admin must stay
    unlimited).
  - ~566–567 `assign_user_policy(…, Some(0))` + `assert_eq!(…, 0, "0 = unlimited wins")` →
    `Some(-1)` + assert `-1` (legacy 0-unlimited semantics).
- `tests/instances_mock_test.rs`:
  - `seed_pool_group` doc comment still says "`0` = unlimited" and call sites pass pool
    memory/gpu `0` (`(2,0,0)`, `(2,0,0)`, `(0,0,0)`, `(4,0,0)`, `(8,0,0)`) → pass `-1`.
  - ~3795–3796 shrink PUT `"pool_memory_mb": 0, "pool_gpu_count": 0` → `-1`.
  - ~3008–3010 / ~3602–3604 / ~3861–3862 admin-settings PUTs set host caps `0` intending
    "not enforced" → under new convention `0` blocks; → `-1`.
- `tests/admin_settings_test.rs` ~15–17 `settings_body` host caps `0`: confirm this is the
  accepts-zero test (0 is a valid value now) rather than legacy "unlimited" intent.
- `tests/db_test.rs` ~1542 / ~1586–1588: instance-row fixtures `host_* = 0` are instance usage
  (not a sentinel) — verify only.
- `tests/docker_lifecycle_test.rs`: not in the working-tree diff (already `-1` at HEAD or
  untouched) — verify it builds against the `i64` repo signature.
- Gates not yet run after the latest edits: `cargo check --tests`, `scripts/check.sh`,
  `scripts/run_tests.sh`.

### Full-gate findings (all fixed + committed on `feature/resource-quotas`)

- `tests/flat_rbac_e2e_test.rs` (486 / 534 / 566–567) and `tests/instances_mock_test.rs`
  (`seed_pool_group` doc + call sites `(2,-1,-1)` etc., shrink PUT, host-ceiling PUT,
  host-cap PUT `host_instance_limit`/memory/gpu → `-1`, keep cpu `2`) — **done**.
- `tests/health_worker_test.rs` (81/142/194) + `tests/db_test.rs` (757/956/981): `.create` /
  `.update` passed `None` into the new `i64` positions → **`-1`**.
- `src/effective_context.rs:164-166`: the ceiling `match` was non-exhaustive
  (`None | Some(limit) if limit < 0` — E0408/E0004) → split into
  `None` / `Some(<0)` / `Some(finite)` arms.
- **Behavior bug:** no groups + no direct ceiling resolved to `0` (blocked) instead of `-1`
  (unlimited). Fixed `calculate_effective_context`:
  `unlimited = user.direct_max_instances.is_none() && groups.is_empty()`;
  covered by `no_groups_no_direct_is_unlimited_but_default_deny`.
- **Behavior bug:** migration 000026 set `users.direct_max_instances DEFAULT -1`, so every
  fresh user was silently unlimited instead of `NULL` = inherit group ceiling (spec Decision 2).
  Removed the `SET DEFAULT -1` (kept the `0 → -1` row rewrite; `down()` already set
  `DEFAULT NULL`, confirming the intent). Fixed `tests/db_test.rs` assertions accordingly.
- **Outdated test:** `test_create_template_rejects_negative_network_bandwidth` treated `-1`
  as invalid; under the convention `-1` = unlimited is valid, only `< -1` is rejected.
  Split into `accepts_unlimited_bandwidth` (200) + `rejects_negative_network_bandwidth`
  (`-5` → 400).
- **Outdated assertion:** `tests/db_test.rs` ~276 fresh custom group ceiling now defaults to
  `-1` (not `2`); ~406 Admin is still `None` in the stop-at-000020 test (migration 000026
  rewrites it to `-1` only in the full chain).
- **OpenAPI spec:** regenerated (`apps/api/security/openapi.json`) for the `-1` sentinel doc
  changes; `committed_spec_is_in_sync` green.
- Gates: `bash scripts/check.sh` **silent**; `bash scripts/run_tests.sh` **715/715 passed**.

## Acceptance criteria

### Migration (spec Decision 1 — data rewrite, behavior-preserving)

- [x] Migration adds `cpu_quota` / `memory_quota` / `gpu_quota` to `groups` and
      `user_groups` (default `0` = blocked), `host_cpu_quota` /
      `host_memory_quota` / `host_gpu_quota` to `system_settings`
      (default `-1` = unlimited), and `owner_group_id` (FK to `groups`,
      `ON DELETE SET NULL`) + `resource_cores` / `resource_memory` /
      `resource_gpu` to `workspace_instances`.
- [x] Migration rewrites existing numeric-limit data behavior-preservingly so
      `-1` becomes the universal unlimited/disabled sentinel and `0` means a
      real zero:
      - `templates.cores`, `templates.memory` (legacy `0` = no Docker limit),
        `templates.network_bandwidth_up_mbps`, `templates.network_bandwidth_down_mbps`:
        `0` → `-1` (unlimited). `templates.gpu_count` unchanged — `0` GPUs is a
        real zero request.
      - `templates.max_run_seconds`, `templates.keep_time_seconds`:
        `NULL` → `-1`, then `NOT NULL DEFAULT -1` (`-1` = feature disabled).
      - `groups.max_instances`: `NULL` and `0` → `-1`, then `NOT NULL DEFAULT -1`
        (the Admin group's "unlimited" survives as `-1`).
      - `system_settings.host_instance_limit`: `0` → `-1` (unlimited), then
        `NOT NULL DEFAULT -1`.
      - `users.direct_max_instances`: `0` → `-1`; the column **stays nullable**
        (`NULL` = inherit the group ceiling, spec Decision 2).
- [x] Migration backfills: existing groups and memberships → `-1` (unlimited);
      existing instances → `owner_group_id` = owner's highest-tier membership
      (admin > manager > user, tie-break oldest; `NULL` if none) and the
      resource snapshot from the template's current values — **after** the
      `0 → -1` rewrite, so a legacy `cores = 0` template never backfills a
      snapshot of `0` (which under the new convention means a zero-core
      request, not unlimited).
- [ ] New groups and new memberships default to `0` (blocked) on the quota
      columns — **OPEN (code-review 2026-09-11, not fixed blind):** the
      implementation does the opposite through every live path: `GroupInput`
      serde defaults are `default_unlimited_*` (`-1`), migration `000026`
      sets the pool columns `SET DEFAULT -1`, and the web create form defaults
      to Unlimited. Only raw-SQL membership inserts (migration `000027`
      `DEFAULT 0`) match the spec. Group creation is admin-only, which bounds
      the exposure, but spec Decision 1 / Story 16 say `0`. Changing three
      layers (serde, migration, web form + its tests, which explicitly assert
      Unlimited defaults) without a test run was judged too risky here —
      follow-up ticket with a green gate run must decide: either flip all
      three layers to `0`, or amend the spec. New host caps (`-1`) and new
      templates (2 cores / 4 GiB) match the spec. (Pre-existing, untouched:
      `default_max_instances()` serde default `2` predates this feature —
      commit `19ca5a5`, flat-RBAC revamp.)

### Runtime convention (the `-1` flip in code, not just data)

- [x] Every runtime gate, entity doc, and validation treats `-1` as the
      unlimited/disabled sentinel and `0` as a real value:
      - `calculate_effective_context` resolves unlimited ceilings and pools to
        `-1` (`effective_max_instances = -1`; aggregate pool `-1` when any
        membership pool is unlimited), not `0`. `direct_max_instances`: `-1` =
        unlimited, `0` = blocked, `NULL` = inherit.
      - `pre_flight` gates: a limit of `-1` skips the layer; `0` blocks every
        finite request; `> 0` is a finite limit. Ceiling gates
        (`effective_max_instances`, `host_instance_limit`) use the same rule.
      - The **`-1` request rule** keys off exactly `-1`: a template resource of
        `-1` is accepted only when that resource's limit is `-1` at every
        layer; a request of `0` is a **finite zero-cost request** (adds nothing
        to usage, never rejected by arithmetic). No value below `-1` is valid.
      - `QuotaContext` / `ResourceUse` doc comments and the self-billed
        (`target_group_id = None`) pool use `-1` for unlimited.
- [x] Validation accepts `>= -1` and rejects `< -1` on every limit field:
      template `cores` / `memory` / `gpu_count` / bandwidth, group
      `max_instances`, admin-settings `host_instance_limit` + host caps, and
      the membership-quota endpoint. (Replaces the current `>= 0` /
      `0 = unlimited` checks.)
- [x] Auto-sleep / keep-time: entities, routes, the health worker, and the
      deadline helpers read `max_run_seconds` / `keep_time_seconds` as
      `i64` with `<= 0` = disabled (the migrated `-1`), replacing the
      `Option::None` sentinel.
- [x] Docker layer: container CPU/memory limits derive from the template with
      `<= 0` → no container limit (a real `0` core/byte request and the `-1`
      unlimited marker both map to "no limit" — the forms never offer `0` for
      cores/memory); bandwidth `-1`/`0` → no `tc` shape, `> 0` → shape.
- [x] Counter queries sum over `('starting','running','paused')` with a
      negative-filtered sum so `-1` snapshots never pollute the totals and `0`
      snapshots contribute nothing; `stopped`/`error` never count. (As
      implemented the sums fold active rows with a per-resource `.max(0)` in
      `activation.rs` rather than a SQL `COALESCE`; the behavior matches.)
      **Code-review 2026-09-11:** `sum_resources_for_user_in_group` was missing
      the `.max(0)` its three siblings had — an active `-1` instance *reduced*
      the pre-flight member usage (quota bypass). Fixed (3 lines) — this is
      the function that feeds `member_used` at `activation.rs` reservation
      time, so the bypass was live.

### Feature behavior

- [x] Launch request accepts an optional billing `group_id`: present and not a
      membership → `403`; absent with exactly one membership → auto-attribute;
      absent with multiple memberships → `400`; zero memberships → `403`
      (never a 500).
- [x] Launch stores `owner_group_id` + the resource snapshot on the instance
      row; the instance JSON exposes both plus the existing read-time
      `owner_group_ids`.
- [x] `pre_flight` grows a bundled quota context (member cap + usage, group
      pool + usage, host caps + usage, template request) and runs the resource
      checks in the fixed order after the instance ceiling and before the host
      ceiling: personal cap (chosen group's membership quota) → chosen group
      pool → host caps.
- [x] Activation transaction locks the chosen group row then the owner's user
      row (no `system_settings` lock); the host checks are best-effort so
      launches into different groups run in parallel.
- [x] Restart of a stopped instance re-runs the same pre-flight; a
      `NULL`-attributed instance is first re-attributed to the owner's
      highest-tier membership (persisted), and rejected `403` if the owner has
      no membership.
- [x] New endpoint edits a membership's quota, gated by `can_manage_users` +
      target-user tier + group tier strictly below the actor; values are `-1`
      or within `[0, group_pool]`.
- [x] `GET /api/groups` (whose list response now includes each group's members
      and per-membership quotas) is readable by any `can_manage_users` holder
      — admin or manager — so the frontend's layered Groups tab renders for
      managers, not just admins.
- [x] Group create/update accepts pool quotas; lowering the pool below a
      member's finite quota → `409`; lowering any quota to a finite value while
      an active `-1`-snapshot instance is in that scope → `409`; deleting a
      group with an active attributed instance → `409`. **Host-cap tightening
      added in code-review 2026-09-11** (`admin_settings.rs` + new
      `test_host_cap_tightening_blocked_by_unlimited_snapshot`): lowering a
      host cap to finite while an active `-1` snapshot exists → `409` (the
      `count_active_unlimited_snapshots(None, None)` helper was already built
      for this scope but never called).
- [x] Admin settings read/write the host caps and `host_instance_limit` with
      the `-1`/`0`/value semantics.
- [x] All quota rejections return `409` with the existing structured
      `{ error, rejection: { scope, current, limit, requested } }` body; new
      resource scopes encode the resource in the scope string. Audit events
      cover quota changes.
- [x] Full Rust gate green: `scripts/check.sh` silent (both feature sets, zero
      warnings) and `scripts/run_tests.sh` green (736/736), including new unit
      tests for the flipped gates (`-1` skip, `0` blocked, finite arithmetic,
      the `-1` request rule with `0` = zero-cost), migration-harness tests for
      the new sentinels/defaults (`db_test.rs` runs the migration chain), and
      route tests (mocked Docker) for every error code above plus validation
      (`-1` accepted, `< -1` rejected on every field).
- [x] **Concurrency (`8b61109`):** same-pool serialization and cross-group
      parallelism now have dedicated tests.
      `test_concurrent_launches_same_group_pool_exactly_one_succeeds` — two
      *different* users billing against one 2-core pool concurrently: exactly
      one `200`, the sibling `409 group_pool_cpu`. No user-row lock is shared,
      so only the group-row lock can serialize pool consumption.
      `test_concurrent_launches_different_groups_proceed_in_parallel_at_host_cap` —
      a Barrier inside the docker-create mock times out (failing the test) if
      either launch is held at a cross-group gate; both `200`, committed cores
      sum exactly to the host cap. Same-user serialization stays covered by
      `test_concurrent_launches_same_user_at_ceiling_exactly_one_succeeds`.
      See spec Testing Decisions.

## Code review — 2026-09-11 (two-axis, Standards + Spec subagents)

Fixed point `main`; `git diff main` (67 files, committed + uncommitted).

**Standards verdict: no documented-standard violations.** No `#[allow]` /
`unsafe` in the diff, no gitignored paths, no `docs/` changes at all (so no
user-guide API-ban risk), no `apps/vnc-ui` references. Judgement-call smells
noted but not actioned (structural refactors without a test run): duplicated
fold shapes in `activation.rs` (now consistent after the `.max(0)` fix),
triplicated pool-tightening blocks in `update_group`, `TriStateInput` vs
`UnlimitedInput` overlap + scattered `-1/0` describe helpers on web,
`billing_model: String` + per-resource scope tables (Primitive Obsession /
Repeated Switches), `GroupPolicy`/`GroupBilling` field clumps (Data Clumps).

**Spec findings fixed this session** (see the amended checkboxes above):
missing `.max(0)` on the member-usage sum (live bypass); missing host-cap
tightening guard; memory-scope `requested` rendered raw in `preflight.ts`
(now through the byte-format helper; `preflight.test.ts` updated).

**Spec divergences accepted and documented (not fixed blind):**
- *Pre-flight order* (Decision 5): code runs host-instance-count before the
  resource loop and iterates resource-major (CPU member→pool→host, then
  memory, then GPU) instead of layer-major. Observable only when several
  layers bind at once (still a `409` either way); reordering risks 30+ unit
  tests with no runner available. Spec Decision 5 should be amended to the
  implemented order, or a follow-up ticket reorders with a green gate.
- *`billing_model` label* (Decision 3 / Out of Scope): `shared|dedicated`
  (migration `000025`, `GroupBilling`, snapshot, UI) never branches any
  sum or check — informational metadata only, sums intentionally
  model-agnostic. Kept; if the label must mean something, that is new scope.
- *New-group defaults* (Decision 1 / Story 16): see the OPEN checkbox above.
