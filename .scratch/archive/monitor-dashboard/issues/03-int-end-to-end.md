# 03 — Integration: Monitor end-to-end against the real stack

**Track:** integration

**What to build:** A full-stack verification that the Monitor tab works end to
end against a live dev stack: the sampler is collecting real per-instance data
through the real `container_stats` path, the snapshot endpoint serves it, and
the panel renders it with sparklines. Also verifies the permission boundary in
the browser — admin and a flag-holding manager see the tab, a user without the
flag does not, and the API rejects them.

**Blocked by:** ~~02-fe-monitor-dashboard~~ (completed — unblocked, next ticket)

**Status:** completed

- [x] Launches a real instance on the dev stack, waits for at least one
      sampler pass, and asserts the Monitor tab shows live host cards and the
      instance row with non-empty CPU/RAM sparklines.
- [x] Verifies the 1h/24h range toggle re-fetches and renders a series in both
      ranges.
- [x] Verifies a paused (auto-slept) instance appears greyed with a `[paused]`
      badge while the instance is paused.
- [x] Verifies the permission boundary: admin sees the tab; a manager whose
      group carries `can_view_monitoring` sees it; a plain user does not see
      the tab and receives a 403 on the snapshot endpoint.
- [x] Teardown restores the dev stack to its prior state (no leaked
      containers/networks) and the run is reproducible from a clean dev stack.

**Notes (post-build sync):** Shipped as `e2e/tests/monitor.full.spec.ts`
(3 tests, full project, run via `pnpm run test:e2e:full`). The spec is
self-provisioning: it creates the `e2e-monitor-template` (local
`tsukisama9292/ow-kasmvnc-ubuntu:jammy`, which auto-whitelists the Admin
group), launches/waits for the instance via the real API, asserts host cards +
the instance row with non-empty CPU/RAM sparklines (≥2 sampler samples), the
24h range re-fetch, the paused `[paused]` badge, and the RBAC boundary
(manager-with-flag sees the tab + 200, plain user no tab + 403). A third test
asserts no leftover template/users/groups/instances. All 3 pass and the suite
is reproducible from a clean dev stack (verified twice; no leaked containers or
networks). Deviation from the original checklist: on a fresh stack the Tier-2
(24h) aggregates are empty until the first 5-minute fold, so the range-toggle
assertion checks the `?range=24h` re-fetch fires and the panel re-renders host
cards + the instance row, rather than a non-empty 24h sparkline.
