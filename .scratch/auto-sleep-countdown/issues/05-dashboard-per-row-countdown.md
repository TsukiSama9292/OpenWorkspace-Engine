# 05 — Dashboard per-row countdown

**What to build:** the dashboard instance list lets users and managers see at a glance when an Instance will auto-sleep. Each row of a running Instance that has a deadline shows a compact「剩 23:45」(formatted by the shared `formatRemaining` helper); rows without a deadline show nothing. The value re-syncs with the instance data already loaded by the dashboard.

**Blocked by:** 02 — Countdown module + VNC overlay

**Status:** ready-for-agent

- [x] Running Instances with a deadline show `剩 23:45` on their row
- [x] Instances without a deadline (disabled duration, or not running) show no countdown text
- [x] Display uses the shared `formatRemaining` helper (no duplicated formatting)
- [x] `pnpm check` and `pnpm build` pass
