# 02 — Countdown module + VNC overlay

**What to build:** a shared countdown module (pure functions) and a `CountdownOverlay` component, and the VNC session shows the countdown. The overlay floats at the top-right, never intercepts mouse or keyboard input (`pointer-events: none`), shows remaining time as `23:45` (or `1:23:45` past an hour) with a small caption stating what happens on expiry (暫停/停止/移除; time only when the action is unknown), turns amber under 10 minutes and red under 60 seconds, and shows a「已到期」state at zero. The overlay owns its lifecycle in the lightest way: it ticks client-side every second and re-syncs with the Instance API on load, on returning to the tab (`visibilitychange`), and every 30 seconds, driven by a callback the page provides (no duplicated polling logic in pages). Instances with no deadline show no overlay. The frontend `Instance` type is extended to carry the new fields.

**Blocked by:** 01 — Auto-sleep deadline API contract

**Status:** ready-for-agent

- [x] Pure functions: `remainingMs` (null = no countdown), `formatRemaining` (`23:45` / `1:23:45`), `severity` (warning < 10 min, critical < 60 s), `wrapperUrl` and `iframeSrc` for ttyd (`/ttyd/{token}/`) and jupyter (`/jupyter/{token}/lab?token=…`)
- [x] `CountdownOverlay` renders remaining time, action caption, severity color classes, `pointer-events: none`, and「已到期」at zero
- [x] VNC session shows the overlay for a running Instance with a deadline; nothing for one without
- [x] Re-sync owned by the overlay (30 s interval + `visibilitychange`), driven by a page-provided callback; ticking continues between re-syncs
- [x] Frontend `Instance` type includes `auto_sleeps_at` and `timeout_action`
- [x] Module + component unit tests green (`pnpm test`), `pnpm check` and `pnpm build` pass
