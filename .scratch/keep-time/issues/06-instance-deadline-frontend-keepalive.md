# 06 — Instance deadline + frontend keepalive

**What to build:** the Instance API response exposes the keep-time deadline and action so the frontend can show idle countdown; a new keepalive module on the remote screen pages keeps the Instance alive while the browser tab is visible and focused, and stops the moment it is not.

**Blocked by:** 01 — Keep-Time schema foundation, 03 — Instance activity tracking

**Status:** done

- [x] Instance API responses include `keep_time_deadline` (last_seen_at + keep_time_seconds, only when `running` and configured) and `keep_time_action`
- [x] Keepalive module: posts a heartbeat to `/api/instances/{id}/heartbeat` every 10 seconds only while the tab is visible and focused; stops on blur/hide; sends immediately on refocus; failures are silently ignored (retried next tick)
- [x] Keepalive only active on the two remote screen pages (`/kasmvnc/[token]/` and `/open/[token]/`) and only while the Instance is `running`
- [x] Countdown deadline helper (compute remaining from `keep_time_deadline`) unit tests green
- [x] Keepalive module tests green (jsdom + fake timers: focus-gated scheduling, refocus fires immediately)

## Notes

- API: `instances.rs` threads the template's `keep_time_seconds`/`keep_time_action` through the template-lookup map into `instance_to_json`; `keep_time_deadline` = `last_seen_at + keep_time_seconds` (ISO), emitted only when `running` + configured + `last_seen_at` set; `keep_time_action` emitted when configured (independent of running state); both `null` otherwise. 5 new API tests (incl. heartbeat-refreshes-deadline).
- Frontend: new `apps/web/src/lib/keepalive/keepalive.ts` — `startKeepalive(instanceId, { intervalMs?, isActive? })` → cleanup; `isActive()` = `visibilityState==='visible' && document.hasFocus()`; immediate send on start/refocus; 10s interval sends only while active; failures ignored; SSR no-op; `cleanup()` clears interval + listeners (focus/blur/visibilitychange). Wired into `/kasmvnc/[token]/` + `/open/[token]/` only while `running` (incl. the poll-transition to running).
- `countdown.ts` gained `deadlineRemaining` alias; `countdown.test.ts` covers keep-time deadline remaining; new `keepalive.test.ts` (5 focus-gating tests, fake timers + fetch stub).
- Verification: `check.sh` zero warnings; `run_tests.sh` (excl. environmental cgroupv2) 348 passed; `pnpm check` 0 errors; `pnpm test` all green.
