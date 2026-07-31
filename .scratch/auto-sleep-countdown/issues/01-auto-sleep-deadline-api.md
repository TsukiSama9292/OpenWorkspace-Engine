# 01 — Auto-sleep deadline API contract

**What to build:** every Instance returned by the API (both the list and the single-instance endpoints) includes two extra fields so the frontend can count down without doing any arithmetic itself: `auto_sleeps_at` — the server-computed deadline (`started_at + max_run_seconds`), present only while the Instance is `running` with a duration set, otherwise `null` — and `timeout_action` — the Template's `remove`/`stop`/`pause` action, `null` when the Template is missing. The deadline is computed in one shared place so it can never drift from what the Auto-Sleep worker judges.

**Blocked by:** None — can start immediately

**Status:** ready-for-agent

- [x] Instance list and single-instance responses both include `auto_sleeps_at` and `timeout_action`
- [x] `running` + `started_at` set + Template `max_run_seconds` set → `auto_sleeps_at` equals `started_at + max_run_seconds`
- [x] `auto_sleeps_at` is `null` when: no duration set, or Instance is paused/stopped (not `running`)
- [x] `timeout_action` passes through the Template's value; `null` when the Template is missing
- [x] Deadline computation is shared between the list and single-instance paths (single source of truth)
- [x] No real (route/Postgres) API tests — the deadline helper gets a pure unit test only (no DB), and both `cargo test --no-run` warning checks produce no output
