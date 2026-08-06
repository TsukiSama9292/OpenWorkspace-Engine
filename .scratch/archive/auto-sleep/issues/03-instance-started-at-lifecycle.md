# 03 — Instance started_at lifecycle

**What to build:** an Instance records when its current running session started: set when entering `running` (startup health check passes, or resume from pause), cleared when leaving `running` (pause/stop). The Instance API response exposes `started_at` so the lifecycle can be verified end-to-end.

**Blocked by:** 01 — Auto-Sleep schema foundation

**Status:** completed

- [ ] Promotion to `running` (health check passes) sets `started_at` to now
- [ ] Resume (`unpause`) sets `started_at`; pause and stop clear it
- [ ] Instance API responses include `started_at`
- [ ] Route tests green
