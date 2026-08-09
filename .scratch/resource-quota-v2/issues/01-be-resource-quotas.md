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

**Status:** ready-for-agent

## Implementation audit — 2026-08-09 (working tree on `feature/resource-quotas`)

### Done (uncommitted working tree)

- Migration `apps/api/migration/src/m20260809_000026_minus_one_sentinel.rs` created + registered
  in `migration/src/lib.rs`: rewrites legacy sentinels → `-1` —
  `workspace_templates.cores/memory/network_bandwidth_up_mbps/network_bandwidth_down_mbps`
  `0 → -1` (gpu_count untouched); `max_run_seconds`/`keep_time_seconds` `NULL → -1` +
  `NOT NULL DEFAULT -1`; `groups.max_instances` `NULL|0 → -1` + `NOT NULL DEFAULT -1`;
  `groups.pool_*` `0 → -1` + `DEFAULT -1`; `system_settings.host_instance_limit` `0 → -1` +
  `DEFAULT -1`; `system_settings.host_*` caps `0 → -1` + `DEFAULT -1`;
  `users.direct_max_instances` `0 → -1` (stays nullable, `DEFAULT -1`). `down()` reverts defaults.
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

## Acceptance criteria

### Migration (spec Decision 1 — data rewrite, behavior-preserving)

- [ ] Migration adds `cpu_quota` / `memory_quota` / `gpu_quota` to `groups` and
      `user_groups` (default `0` = blocked), `host_cpu_quota` /
      `host_memory_quota` / `host_gpu_quota` to `system_settings`
      (default `-1` = unlimited), and `owner_group_id` (FK to `groups`,
      `ON DELETE SET NULL`) + `resource_cores` / `resource_memory` /
      `resource_gpu` to `workspace_instances`.
- [ ] Migration rewrites existing numeric-limit data behavior-preservingly so
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
- [ ] Migration backfills: existing groups and memberships → `-1` (unlimited);
      existing instances → `owner_group_id` = owner's highest-tier membership
      (admin > manager > user, tie-break oldest; `NULL` if none) and the
      resource snapshot from the template's current values — **after** the
      `0 → -1` rewrite, so a legacy `cores = 0` template never backfills a
      snapshot of `0` (which under the new convention means a zero-core
      request, not unlimited).
- [ ] New groups and new memberships default to `0` (blocked) on the quota
      columns; new host caps default to `-1`; new templates keep their positive
      defaults (2 cores / 4 GiB).

### Runtime convention (the `-1` flip in code, not just data)

- [ ] Every runtime gate, entity doc, and validation treats `-1` as the
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
- [ ] Validation accepts `>= -1` and rejects `< -1` on every limit field:
      template `cores` / `memory` / `gpu_count` / bandwidth, group
      `max_instances`, admin-settings `host_instance_limit` + host caps, and
      the membership-quota endpoint. (Replaces the current `>= 0` /
      `0 = unlimited` checks.)
- [ ] Auto-sleep / keep-time: entities, routes, the health worker, and the
      deadline helpers read `max_run_seconds` / `keep_time_seconds` as
      `i64` with `<= 0` = disabled (the migrated `-1`), replacing the
      `Option::None` sentinel.
- [ ] Docker layer: container CPU/memory limits derive from the template with
      `<= 0` → no container limit (a real `0` core/byte request and the `-1`
      unlimited marker both map to "no limit" — the forms never offer `0` for
      cores/memory); bandwidth `-1`/`0` → no `tc` shape, `> 0` → shape.
- [ ] Counter queries sum over `('starting','running','paused')` with a
      negative-filtered `COALESCE(SUM(...),0)` so `-1` snapshots never pollute
      the totals and `0` snapshots contribute nothing; `stopped`/`error` never
      count.

### Feature behavior

- [ ] Launch request accepts an optional billing `group_id`: present and not a
      membership → `403`; absent with exactly one membership → auto-attribute;
      absent with multiple memberships → `400`; zero memberships → `403`
      (never a 500).
- [ ] Launch stores `owner_group_id` + the resource snapshot on the instance
      row; the instance JSON exposes both plus the existing read-time
      `owner_group_ids`.
- [ ] `pre_flight` grows a bundled quota context (member cap + usage, group
      pool + usage, host caps + usage, template request) and runs the resource
      checks in the fixed order after the instance ceiling and before the host
      ceiling: personal cap (chosen group's membership quota) → chosen group
      pool → host caps.
- [ ] Activation transaction locks the chosen group row then the owner's user
      row (no `system_settings` lock); the host checks are best-effort so
      launches into different groups run in parallel.
- [ ] Restart of a stopped instance re-runs the same pre-flight; a
      `NULL`-attributed instance is first re-attributed to the owner's
      highest-tier membership (persisted), and rejected `403` if the owner has
      no membership.
- [ ] New endpoint edits a membership's quota, gated by `can_manage_users` +
      target-user tier + group tier strictly below the actor; values are `-1`
      or within `[0, group_pool]`.
- [ ] `GET /api/groups` (whose list response now includes each group's members
      and per-membership quotas) is readable by any `can_manage_users` holder
      — admin or manager — so the frontend's layered Groups tab renders for
      managers, not just admins.
- [ ] Group create/update accepts pool quotas; lowering the pool below a
      member's finite quota → `409`; lowering any quota to a finite value while
      an active `-1`-snapshot instance is in that scope → `409`; deleting a
      group with an active attributed instance → `409`.
- [ ] Admin settings read/write the host caps and `host_instance_limit` with
      the `-1`/`0`/value semantics.
- [ ] All quota rejections return `409` with the existing structured
      `{ error, rejection: { scope, current, limit, requested } }` body; new
      resource scopes encode the resource in the scope string. Audit events
      cover quota changes.
- [ ] Full Rust gate green: `scripts/check.sh` silent (both feature sets, zero
      warnings) and `scripts/run_tests.sh` green, including new unit tests for
      the flipped gates (`-1` skip, `0` blocked, finite arithmetic, the `-1`
      request rule with `0` = zero-cost), migration-harness tests for the new
      sentinels/defaults, route tests (mocked Docker) for every error code
      above plus validation (`-1` accepted, `< -1` rejected on every field),
      and a concurrency test proving same-pool serialization while different
      groups proceed in parallel.
