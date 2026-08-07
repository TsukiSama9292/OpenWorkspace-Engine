# 02 — Frontend: Monitor panel + sparklines + flag UI

**Track:** frontend

**What to build:** The Monitor tab itself. A `MonitorPanel` renders three host
cards (CPU / RAM / Disk, each with a native-SVG sparkline) above an
"Active Instances" table (owner/instance, template, runtime badge, uptime,
CPU % and RAM usage/limit, each with a sparkline; running/starting normal,
paused rows greyed with a `[paused]` badge; stopped/errored excluded; sortable
by column). A 1h/24h range toggle drives the snapshot request; the panel polls
every 5 seconds. The sidebar item and tab gating switch from admin-only to the
new `can_view_monitoring` permission (`mayViewMonitoring`), and the group editor
gains a checkbox for the flag so admins can grant it to custom groups.

**Blocked by:** 01-be-monitor-dashboard

**Status:** completed

- [x] `EffectiveContext` / `Group` / `GroupInput` types gain
      `can_view_monitoring`; the permissions module gains `mayViewMonitoring`;
      the Monitor sidebar item and tab are shown by the flag, not by
      `is_admin` alone.
- [x] The group editor form round-trips the new flag checkbox (create and
      edit), consistent with the existing five flag controls.
- [x] A reusable `Sparkline` component maps a number series to an SVG `<path>`
      with no chart library.
- [x] `MonitorPanel` renders host cards (value + sparkline) and the active
      instance table with all agreed columns, paused greying + badge, and
      column sorting; a 1h/24h toggle re-fetches the snapshot for the chosen
      range; polling runs every 5 seconds.
- [x] API client exposes the Monitor snapshot call against the contract from
      `01-be`.
- [x] Web tests cover `Sparkline` output, `MonitorPanel` rendering (host
      cards, table columns, paused badge, range toggle, sorting),
      `mayViewMonitoring`, and the group-editor checkbox.
- [x] `pnpm check` (svelte-check + eslint) and `pnpm test` are green before
      the ticket closes.

**Notes (post-build sync):** Landed as expected — `mayViewMonitoring` in
`apps/web/src/lib/permissions.ts`, sidebar + tab gated by `$canViewMonitoring`
in `apps/web/src/routes/+page.svelte`, checkbox in `GroupPanel.svelte`,
`Sparkline.svelte` + `MonitorPanel.svelte` under `components/monitor/`,
client call `fetchMonitorSnapshot` in `lib/api/monitor.ts`. `pnpm test`
(310 tests / 25 files) and `pnpm check` (svelte-check 0 errors, eslint clean)
are green.
