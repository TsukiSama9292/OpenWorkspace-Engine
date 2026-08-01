# 02 — Template keep-time config (API + form)

**What to build:** a Template owner can set and edit 閒置保持時間 and 閒置回收操作 on a Template, in both the API and the template form. The API validates a minimum of 60 seconds and only the three actions (`pause` default), persists them, and returns them so the edit form can prefill.

**Blocked by:** 01 — Keep-Time schema foundation

**Status:** ready-for-agent

- [x] POST/PUT Templates accept and return `keep_time_seconds` (nullable) and `keep_time_action`
- [x] Invalid input rejected with 400: `keep_time_seconds` < 60, or `keep_time_action` not in `remove`/`stop`/`pause`
- [x] Template form (create + edit) has keep-time duration and action fields; edit prefills from the Template
- [x] Off sends `null`; action defaults to `pause` when unset
- [x] API route tests + form unit tests green

## Notes

- API: `CreateTemplateRequest`/`UpdateTemplateRequest` gained `keep_time_seconds: Option<i64>` (`#[serde(default)]`) and `keep_time_action: String` (default `"pause"` via new `default_keep_time_action`). New `validate_keep_time` mirrors `validate_auto_sleep` (400 if seconds < 60 or action not in remove/stop/pause) and runs in both `create_template` and `update_template`. The repo `create`/`update` calls now pass `input.keep_time_seconds` / `&input.keep_time_action` instead of the previous hardcoded `None`/`"pause"`. `template_to_json` emits both fields.
- Frontend: `Template.keep_time_seconds`/`keep_time_action`, `TemplateFormState.keepTimeSeconds`/`keepTimeAction` added; `createInitialFormState` defaults to `null`/`'pause'`; `buildTemplateBody` sends `keep_time_seconds`/`keep_time_action`; `formStateFromTemplate` prefills (`?? null` / `?? 'pause'`). `TemplateResources.svelte` adds a bindable keep-time block (enable checkbox + number input min 60/step 60 + action select) modeled on the Usage Limit/Timeout Action block; `TemplatePanel.svelte` binds both.
- Tests: 5 new API route tests (create with values, default off/pause, update round-trip incl. clearing, <60 → 400, bogus action → 400) and frontend tests for defaults, disabled/enabled body, and prefill.
- Verified: `cargo check --lib` (+`--features docker`) clean; `scripts/check.sh` all three sections no output; `cargo test --no-run --features docker` compiles all binaries; `pnpm check` 0 errors/0 warnings; `pnpm vitest run src/tests/template-form.test.ts` 21/21 and `dashboard-view.test.ts` 26/26. Did not run `scripts/run_tests.sh` (per instructions).
