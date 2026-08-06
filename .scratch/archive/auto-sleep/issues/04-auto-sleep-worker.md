# 04 — Auto-sleep worker

**What to build:** a background sweep on the existing 3-second tick finds Instances that are `running`, have `started_at` set, and whose Template has a duration; when `now - started_at >= max_run_seconds` it executes the Template's timeout action: `remove` (full cleanup + delete the Instance), `stop`, or `pause`. Template config is read each tick (mid-run changes take effect immediately); Instances with NULL `started_at` (pre-feature) are never touched.

**Blocked by:** 02 — Template auto-sleep config, 03 — Instance started_at lifecycle

**Status:** completed

- [ ] Sweep considers only Instances that are `running` + `started_at` set + Template `max_run_seconds` set
- [ ] `remove` action: route deleted, VNC cache cleared, container stopped and removed, Instance row deleted
- [ ] `stop` action: container stopped, status `stopped`, `started_at` cleared
- [ ] `pause` action: container paused, status `paused`, `started_at` cleared
- [ ] Not-fired cases: not yet expired; `started_at` NULL (legacy); Template duration disabled; Instance already not `running` (no re-trigger)
- [ ] Template duration/action changed mid-session is honored on the next tick
- [ ] Sweep unit tests green (injected clock + mocked Docker service)
