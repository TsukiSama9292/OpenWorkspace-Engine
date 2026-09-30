# 04 — Light-touch polish plus the in-session viewer family

**Track:** frontend

**What to build:** visual unification of the two already-polished surfaces (monitor keeps its interactive time-series and sparklines, logs keep their filter/diff/follow interactions — behavior byte-for-byte) and the viewer family (toolbar weight, status presentation, shared skin) with connection, clipboard, and settings behavior exactly preserved. Scheduled last because it is the only surface where a defect can sever a live connection.

**Blocked by:** 03 — Administration pages: groups, users, settings.

**Status:** completed — implemented on `feature/web-surface-polish`, all gates green (vitest 507, `pnpm check` clean, analysis at baseline). Live checklist (dev stack, 2026-09-29): monitor.full 3/3 (detail modal through the shared Modal), observability.full 5/5, log-redesign.full 6/6, live-instance.full 1/1 (real container → websockify → canvas → teardown). Known flake: monitor teardown DELETE of a paused container hung twice transiently (backend, untouched by this branch; manual pause→delete verified fast; clean re-run green). Accepted divergences: no new pure seams to unit-test (light-touch milestone); audit empty copy teaches like every other surface; detail-modal chrome follows the round's uniform dialog behavior (charts/views untouched). Review fixes (StatusBar NAV_BTN + dominant intent, Clipboard handler uniformity) are in.

- [ ] Monitor behavior unchanged (time-series, sparklines, detail views); only skin tokens align
- [ ] Audit-trail and container-log interactions unchanged (filters, diff expansion, follow mode); only skin tokens align
- [ ] Viewer toolbar gives the most frequent intent dominant weight; clipboard, status, and settings behave exactly as before
- [ ] Viewer indicators respect the reduced-motion preference
- [ ] Unit tests over new pure seams plus component tests; existing monitor/log specs still green
- [ ] Manual live-connection checklist executed and recorded (no automated live-connection tests)
- [ ] Zero emoji on touched surfaces; keyboard reachability; reduced-motion honored; self-hosted artwork only
- [ ] Repository hard gates green; no API, migration, permission, or lifecycle change anywhere in the diff
