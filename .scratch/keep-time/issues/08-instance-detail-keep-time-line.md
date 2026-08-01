# 08 — Instance detail keep-time line

**What to build:** the Instance detail page shows one line describing the configured keep-time policy (e.g. 閒置 15 分鐘後暫停), so the user can see at a glance that an Instance will be reclaimed if left idle. Hidden when keep-time is disabled on the Template.

**Blocked by:** 06 — Instance deadline + frontend keepalive

**Status:** ready-for-agent

- [x] `/instances/[id]` shows the keep-time line (duration + action) when configured
- [x] Line hidden when keep-time is disabled (no `keep_time_action`/deadline on the Instance)
- [x] Page test green

## Notes

### API — `keep_time_seconds` on the instance JSON
- `apps/api/src/routes/workspace/instances.rs`: added `"keep_time_seconds": keep_time_seconds` to the `instance_to_json` `json!` map. The field is the template's raw `keep_time_seconds` (Option<i64>), independent of running status; `None` → JSON `null`. All call sites (list, get, launch ×2) already threaded `keep_time_seconds` in from ticket 06, so no signature changes were needed.
- `apps/api/tests/instances_mock_test.rs`: new `test_keep_time_seconds_in_instance_json` — template with keep-time configured (600s/`stop`) → `GET /api/instances/{id}` returns `keep_time_seconds == 600` and `keep_time_action == "stop"` even when not running; template without keep-time → `keep_time_seconds` and `keep_time_action` are `null`.

### Frontend
- `apps/web/src/lib/types.ts`: added `keep_time_seconds?: number | null` to the `Instance` interface (next to the existing `keep_time_deadline` / `keep_time_action`).
- `apps/web/src/lib/countdown/countdown.ts`: added pure helper
  `keepTimePolicyLine(keepTimeSeconds: number | null | undefined, keepTimeAction: TimeoutAction | null | undefined): string | null`.
  - Returns `null` when `keepTimeSeconds` is null/undefined/`<=0`, or when `keepTimeAction` is null/undefined.
  - Unit selection: hours when `seconds % 3600 === 0`, else minutes when `seconds % 60 === 0`, else seconds.
  - **90-second edge decision: falls back to the seconds branch → `閒置 90 秒後暫停`** (90 is not divisible by 3600 or 60; no mixed-unit formatting in v1).
  - Output shape: `閒置 <N> 小時後<label>` / `閒置 <N> 分鐘後<label>` / `閒置 <N> 秒後<label>`; label from `TIMEOUT_ACTION_LABELS` (暫停/停止/移除), falling back to 暫停 for an unknown action string.
- `apps/web/src/lib/components/instances/KeepTimeLine.svelte` (new): tiny presentational component — props `keepTimeSeconds`/`keepTimeAction`, derives the line via `keepTimePolicyLine`, renders an `info-item`/`info-label`(Keep Time)/`info-value` row when non-null, renders nothing otherwise. Uses its own scoped copies of the page's info-* styles (Svelte style scoping).
- `apps/web/src/routes/instances/[id]/+page.svelte`: imported `<KeepTimeLine>` and placed it as the last row inside the existing `.info-grid` (after Container), feeding `instance.keep_time_seconds` + `instance.keep_time_action`.

### Tests
- `apps/web/src/tests/countdown.test.ts`: added `keepTimePolicyLine` unit tests — disabled cases (null/undefined/0/negative → null), missing action → null, exact minutes (60→1 分鐘, 900→15 分鐘, 1800→30 分鐘), exact hours (3600→1 小時, 7200→2 小時), 90s edge → `閒置 90 秒後暫停`, all three action labels, unknown action defaults to 暫停.
- `apps/web/src/tests/keep-time-line.test.ts` (new): component test — shows `閒置 15 分鐘後暫停` and `閒置 2 小時後停止`; renders empty when disabled / props absent / action missing.

### Verification results (all green)
- `apps/api && bash scripts/check.sh`: both invocations (default + `docker` feature) ZERO output.
- `apps/api && bash scripts/run_tests.sh --no-fail-fast -E '!test(test_create_container_from_template_cores_and_memory)'`: **349 tests run: 349 passed, 1 skipped** (the excluded cgroupv2 test). New test `test_keep_time_seconds_in_instance_json` PASS.
- `apps/web && pnpm check`: **0 errors and 0 warnings** (two consecutive runs; an earlier one-off `CountdownOverlay.svelte:141` error proved transient — it disappeared once the file set was consistent, and that file was untouched by this ticket).
- `apps/web && pnpm test`: **9 files passed, 128 tests passed**.

### Deviations
- None material. Helper placed in `apps/web/src/lib/countdown/countdown.ts` (recommended home); keep-time line rendered via a new `KeepTimeLine.svelte` component (recommended approach for a real, green page test). No changes to any forbidden file.

