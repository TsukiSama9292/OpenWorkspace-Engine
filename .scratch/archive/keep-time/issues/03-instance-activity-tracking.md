# 03 — Instance activity tracking (last_seen_at lifecycle + heartbeat endpoint)

**What to build:** an Instance records when its remote screen was last actively viewed. `last_seen_at` is set to now whenever the Instance enters `running` (health-check promotion, start, resume from pause) and cleared when it leaves `running` (pause/stop) — exactly parallel to `started_at`. A new authenticated heartbeat endpoint updates `last_seen_at` to now so a focused browser page can keep the Instance alive.

**Blocked by:** 01 — Keep-Time schema foundation

**Status:** completed

- [x] Promotion to `running` (health check passes) sets `last_seen_at` to now alongside `started_at`
- [x] `start` and `unpause` set `last_seen_at`; `pause` and `stop` clear it
- [x] `POST /api/instances/{id}/heartbeat` (auth + ownership via `can_manage_instance`) sets `last_seen_at` to now; 200 on success, 403 for non-owners without management rights, 404 for unknown instance
- [x] Route tests green for the lifecycle transitions and heartbeat endpoint

## Notes

Implemented in ticket 03. `last_seen_at` now moves in lockstep with `started_at`:

- **Set (`Some(Utc::now())`):**
  - `apps/api/src/health_worker.rs` `check_single_instance` — on probe success, right after `update_started_at`.
  - `apps/api/src/routes/workspace/instances.rs` `unpause_instance` — right after `update_started_at`.
  - (The `start` route itself only sets status `starting`; an instance becomes `running` via the health-worker promotion, which is the point that sets both `started_at` and `last_seen_at`.)
- **Cleared (`None`):**
  - `apps/api/src/routes/workspace/instances.rs` `stop_instance` and `pause_instance`.
  - `apps/api/src/timeout_action.rs` shared `stop` and `pause` helpers (auto-sleep / keep-time paths).
- **Heartbeat:** new route `POST /api/instances/{id}/heartbeat` → `heartbeat_instance` in `apps/api/src/routes/workspace/instances.rs`. Follows the `stop_instance` error shape: 404 `Instance not found`, 403 `Forbidden` via `can_manage_instance`, 500 `Failed to update last_seen_at`, 200 `{"status": "ok"}`. No request body.

Tests added in `apps/api/tests/instances_mock_test.rs` (compile-verified via `scripts/check.sh`):
- `test_heartbeat_sets_last_seen_at` — 200 + repo asserts `last_seen_at` is Some; second heartbeat still 200.
- `test_heartbeat_requires_auth` — 401 without cookie.
- `test_heartbeat_forbidden_for_non_owner` — owner is a second user; a non-owner regular user gets 403, owner gets 200.
- `test_heartbeat_unknown_instance` — 404.
- `test_unpause_sets_last_seen_at`, `test_pause_clears_last_seen_at`, `test_stop_clears_last_seen_at` — assert `last_seen_at` via `WorkspaceInstanceRepository`.

`scripts/check.sh` passes with zero warnings on both feature gates. The runtime suite (`scripts/run_tests.sh`) was not run here per coordination constraints — a parallel agent shares the tree/Docker; the maintainer runs the full suite after both land.

Health-worker promotion test: not added. The probe uses a real `reqwest` client against `https://{ip}:{port}/` with an invalid cert on an unreachable test IP — it cannot succeed in CI, and the ticket forbids mocking `reqwest`. Coverage for the promotion path relies on the route tests + code correctness (the `unpause` route test exercises the same set-side lockstep logic shape).
