# 02 — Frontend: quota UI (billing-group picker, layered Groups tab, `0`/`-1` forms)

**Track:** frontend

**What to build:** the browser UI for the quota feature. Users pick (or
implicitly default) the billing group when launching, and see quota rejections
with readable numbers; managers can view groups and edit members' resource
quotas without being admins; admins can set group pools (with a one-click
member reset) and host caps; template and policy forms speak the unified
`0` / `-1` / value language. Built against the backend schema from ticket 01.

**Blocked by:** 01 — Backend: resource quotas, attribution, and the `0`/`-1`
convention

**Status:** ready-for-agent

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
      `409` and offers a one-click "reset all member quotas to `0`" action.
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
      template-form toggles, the admin-settings tri-state, and every new
      `preflight.ts` scope + byte formatting.
