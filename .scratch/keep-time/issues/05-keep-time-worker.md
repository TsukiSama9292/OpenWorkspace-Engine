# 05 — Keep-Time worker

**What to build:** a background sweep on the existing 3-second tick finds Instances that are `running`, have `last_seen_at` set, and whose Template has a keep-time; when `now - last_seen_at >= keep_time_seconds` it executes the Template's keep-time action (`pause`/`stop`/`remove`) via the shared helpers from ticket 04. Template config is read each tick (mid-run changes take effect immediately); Instances with NULL `last_seen_at` (pre-feature) are never touched.

**Blocked by:** 01 — Keep-Time schema foundation, 03 — Instance activity tracking, 04 — Shared timeout-action helpers

**Status:** ready-for-agent

- [x] Instance repository exposes a query for `running` Instances with `last_seen_at` set
- [x] Sweep considers only Instances that are `running` + `last_seen_at` set + Template `keep_time_seconds` set
- [x] `remove` action: route deleted, VNC cache cleared, container stopped and removed, Instance row deleted
- [x] `stop` action: container stopped, status `stopped`, `last_seen_at` cleared
- [x] `pause` action: container paused, status `paused`, `last_seen_at` cleared
- [x] Not-fired cases: not yet expired (recent heartbeat); `last_seen_at` NULL (legacy); Template keep-time disabled; Instance already not `running` (no re-trigger)
- [x] Template keep-time/action changed mid-session is honored on the next tick
- [x] Sweep unit tests green (injected clock + mocked Docker service)

## Notes

Implemented by adding `list_running_with_last_seen_at` to `WorkspaceInstanceRepository` (apps/api/src/db.rs, mirrors `list_running_with_started_at`: `status = 'running'` + `last_seen_at IS NOT NULL`) and a `pub async fn check_keep_time(instance_repo, template_repo, docker: &dyn DockerService, vnc_cache, now: DateTime<Utc>) -> Result<usize, String>` in apps/api/src/health_worker.rs, wired into the 3s `run()` loop right after the `check_auto_sleep` block.

Behavior per tick: query `list_running_with_last_seen_at()`; per instance look up template (warn + continue on error/None); skip when `keep_time_seconds` is None; skip when `elapsed = (now - last_seen_at).num_seconds() < keep_time_seconds` (with a `let Some(last_seen_at) = instance.last_seen_at else { continue; }` guard); dispatch on `keep_time_action`: `remove`/`stop` → `timeout_action::{remove,stop}(..., vnc_cache, instance, "Keep-time")`, `pause` → `timeout_action::pause(..., instance, "Keep-time")` (no vnc_cache arg), anything else → warn + continue. Each success increments `triggered`; each error logs `tracing::error!("Keep-time failed for instance '{}': {}", instance.name, e)`. Returns `Ok(triggered)`. The `run()` loop labels its own block "Keep-time worker" for logging.

Added 8 tests in apps/api/tests/health_worker_test.rs (plus two `WorkerTestContext` helpers: `create_template_with_keep_time` and `create_running_instance_with_last_seen_at`): `test_keep_time_pause_fires`, `test_keep_time_stop_fires`, `test_keep_time_remove_fires`, `test_keep_time_not_yet_expired`, `test_keep_time_skips_null_last_seen_at`, `test_keep_time_skips_disabled_template`, `test_keep_time_honors_midrun_template_change` (uses `WorkspaceTemplateRepository::update`), `test_keep_time_no_retrigger_after_pause`.

Test commands run:
- `bash scripts/check.sh` (cargo test --no-run default + --features docker + cargo check --lib): all three invocations produced ZERO warnings/errors.
- `bash scripts/run_tests.sh --no-fail-fast -E '!test(test_create_container_from_template_cores_and_memory)'`: `Summary [  31.149s] 344 tests run: 344 passed, 1 skipped` — all green, including the 8 new keep-time tests.
