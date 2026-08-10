# 02 — Frontend: quota UI (billing-group picker, layered Groups tab, `-1` convention forms)

**Track:** frontend

**What to build:** the browser UI for the quota feature. Users pick (or
implicitly default) the billing group when launching, and see quota rejections
with readable numbers; managers can view groups and edit members' resource
quotas without being admins; admins can set group pools (with a one-click
member reset) and host caps; template and policy forms speak the unified
`-1` = unlimited / `0` = blocked / value language — **including the flip of the
existing form fields that today use `0` as their unlimited sentinel**
(`template-form.ts` "0 = unlimited", `group-form.ts` max instances,
`AdminSettings.svelte` host instance limit). Built against the backend schema
from ticket 01.

**Blocked by:** 01 — Backend: resource quotas, attribution, and the `-1`
convention

**Status:** ready-for-agent (blocked — ticket 01 revision round in progress)

## Status — 2026-08-10

Not started. No `apps/web/` changes exist in the working tree; all backend work
is in ticket 01. Ticket 01's base delivery is committed; its 2026-08-10
revision round (billing attribution + member-quota editing) is committed but
its test suite is not yet green (18 `instances_mock_test` failures in flight),
so 02 must not start until 01 closes green.

Backend contract confirmed for this ticket to build against (from the 01 audit):

- Template `max_run_seconds` / `keep_time_seconds` are now non-nullable `i64` in
  the JSON (`-1` = disabled; `0` = real value for keep-time/auto-sleep is still
  valid, deadlines only trigger on `> 0`). Web `Template` type still models them
  as `number | null` — must become `number` with `-1` rendering as "off".
- Group pool (`pool_cpu_cores` / `pool_memory_mb` / `pool_gpu_count`) and
  `max_instances` use `-1` = unlimited / `0` = blocked. `group-form.ts`
  "0 = unlimited" sentinel for max instances must flip to `-1`.
- Host settings (`host_instance_limit` / `host_cpu_cores` / `host_memory_mb` /
  `host_gpu_count`) use `-1` = unlimited / `0` = blocked. `AdminSettings.svelte`
  legacy `0` = unlimited for the instance limit must flip to `-1`.
- `PreflightRejectionScope` resource scopes (`user_cpu|memory|gpu`,
  `group_cpu|memory|gpu`, `host_cpu|memory|gpu`) exist backend-side; web
  contract types + `preflight.ts` copy + byte-format rendering still to do.
- `direct_max_instances` stays nullable (`NULL` = inherit group ceiling);
  `-1` = unlimited; `0` = blocked.
- New (revision round, backend committed): launch carries an optional billing
  `owner_group_id` — present-and-not-a-membership → `403`; absent with exactly
  one membership → auto-attribute; absent with several → `400`; zero
  memberships → `403`. `Instance` JSON exposes `owner_group_id` plus the
  resource snapshot; `Group` JSON includes a `members` array with per-membership
  quotas; managers (not just admins) can edit a member's quota via the member
  quota endpoint. The frontend picker (multi-group only, default highest-cap
  membership) and the manager Groups-tab quota editing build against these.

## Acceptance criteria

- [ ] Contract types extend: `PreflightRejectionScope` gains the resource
      scopes (`user_cpu|memory|gpu`, `group_cpu|memory|gpu`,
      `host_cpu|memory|gpu`); `Instance` gains `owner_group_id` and the
      resource snapshot; `Group` gains pool quotas and member lists with
      per-membership quotas; the launch action can carry a billing `group_id`.
- [ ] `preflight.ts` has human copy for every new scope and renders the
      memory scopes' `current` / `limit` / `requested` through the existing
      byte-format helper (readable units, never raw bytes).
- [ ] Launch flow shows a billing-group picker only for multi-group users,
      defaulting to the highest-cap membership (tie-break by tier then name);
      single-group users see nothing and the request omits `group_id`. The
      resulting `400` (multi-group, no pick) and `403` (not a member) are
      surfaced clearly.
- [ ] Every instance — single-group users included, who never see a picker —
      shows its billing group (and resource usage) as a label on the instance
      card / detail view, so attribution is always visible.
- [ ] Groups tab opens to `can_manage_users` (not just admins): managers see
      the group list and each group's members and can edit a member's quotas
      when the group tier and the member's tier are both below theirs; the
      create / edit / delete group and pool-editing controls stay admin-only.
- [ ] Group pool editor lets admins set each resource quota; when a value
      would be blocked by the member-quota invariant, the form explains the
      `409` and offers a one-click "reset all member quotas to `0`" action
      (`0` = blocked).
- [ ] **Existing forms flip their unlimited sentinel from `0` to `-1`**:
      `template-form.ts` (cores / memory / bandwidth `-1` = unlimited toggle,
      `0` no longer offered for those fields; GPU count stays `0..N` with a
      `-1` unlimited option; auto-sleep / keep-time `-1` = disabled),
      `group-form.ts` (max instances `-1` / `0` / positive tri-state),
      `AdminSettings.svelte` (host instance limit tri-state), and the user
      policy dialog (`direct_max_instances` `-1` / `0` / positive tri-state,
      `NULL` inherit untouched). Validation messages change from
      "`>= 0` (0 = unlimited)" to the `-1` language and reject `< -1`.
- [ ] Template form exposes `-1` = unlimited (explicit toggle) for
      cores / memory / bandwidth / auto-sleep / keep-time and a `0..N` or
      unlimited option for GPU count; the user-policy dialog presents
      `direct_max_instances` as a `-1` / `0` / positive tri-state.
- [ ] Admin Settings tab gains the three host-cap inputs with the same
      `-1` / `0` / value semantics and submits through the admin settings API.
- [ ] Full web gate green: `pnpm check` (svelte-kit sync + svelte-check +
      eslint) and the Vitest suite pass, including new tests for the picker
      (hidden single-group / default multi-group), the layered Groups tab
      (manager vs admin visibility), the pool editor reset action, the
      template-form `-1` toggles (and the removed `0`-as-unlimited paths),
      the admin-settings tri-state, and every new `preflight.ts` scope + byte
      formatting.
