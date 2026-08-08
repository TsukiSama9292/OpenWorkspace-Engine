# 01 — Backend: resource quotas, attribution, and the `0`/`-1` convention

**Track:** backend

**What to build:** the entire server-side of the CPU / memory / GPU quota
feature (spec `.scratch/resource-quota-v2/spec.md`). After this ticket lands, a
launch or restart is accepted only when it fits the caller's personal cap in
the chosen billing group, that group's pool, and the host caps; every instance
records who pays for it (`owner_group_id`) and what it asked for (resource
snapshot); and admins/managers can set group pools, per-member quotas, and
host caps through the API. The numeric convention is unified platform-wide:
`0` = blocked, `>0` = the value, `-1` = unlimited — with existing data migrated
behavior-preservingly so upgrades do not lock anyone out.

**Blocked by:** None — can start immediately

**Status:** ready-for-agent

## Acceptance criteria

- [ ] Migration adds `cpu_quota` / `memory_quota` / `gpu_quota` to `groups` and
      `user_groups` (default `0`), `host_cpu_quota` / `host_memory_quota` /
      `host_gpu_quota` to `system_settings` (default `-1`), and
      `owner_group_id` (FK to `groups`, `ON DELETE SET NULL`) +
      `resource_cores` / `resource_memory` / `resource_gpu` to
      `workspace_instances`.
- [ ] Migration rewrites existing numeric-limit data behavior-preservingly:
      `0` → `-1` and `NULL` → `-1` on the decided fields
      (`templates.cores`/`memory`/`bandwidth*`/`max_run_seconds`/`keep_time_seconds`,
      `groups.max_instances`, `system_settings.host_instance_limit`), making the
      template timeout fields `NOT NULL DEFAULT -1`. `users.direct_max_instances`
      stays nullable (`NULL` = inherit group ceiling); only its `0` value maps
      to `-1`.
- [ ] Migration backfills: existing groups and memberships → `-1` (unlimited);
      existing instances → `owner_group_id` = owner's highest-tier membership
      (admin > manager > user, tie-break oldest; `NULL` if none) and the
      resource snapshot from the template's current values.
- [ ] New groups and new memberships default to `0` (blocked); new host caps
      default to `-1`.
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
      pool → host caps. `0` blocks every finite launch; `-1` skips the layer; a
      `-1` template request is accepted only where that resource's limit is
      `-1` at every layer.
- [ ] Counter queries sum over `('starting','running','paused')` with a
      negative-filtered `COALESCE(SUM(...),0)` so `-1` snapshots never pollute
      the totals; `stopped`/`error` never count.
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
- [ ] Migration sequencing: the snapshot backfill for existing instances reads
      template values **after** the `0 → -1` data rewrite, so a legacy
      `cores = 0` (unlimited) template never backfills a snapshot of `0`
      (which under the new convention would mean "blocked").
- [ ] Group create/update accepts pool quotas; lowering the pool below a
      member's finite quota → `409`; lowering any quota to a finite value while
      an active `-1`-snapshot instance is in that scope → `409`; deleting a
      group with an active attributed instance → `409`.
- [ ] Admin settings read/write the host caps with the `0`/`-1`/value
      semantics.
- [ ] All quota rejections return `409` with the existing structured
      `{ error, rejection: { scope, current, limit, requested } }` body; new
      resource scopes encode the resource in the scope string. Audit events
      cover quota changes.
- [ ] Full Rust gate green: `scripts/check.sh` silent (both feature sets, zero
      warnings) and `scripts/run_tests.sh` green, including new unit tests for
      the pure decision logic, Postgres-harness counter and migration tests,
      route tests (mocked Docker) for every error code above, and a concurrency
      test proving same-pool serialization while different groups proceed in
      parallel.
