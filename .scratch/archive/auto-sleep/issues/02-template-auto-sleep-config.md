# 02 — Template auto-sleep config (API + form)

**What to build:** a Template owner can set and edit 使用時長 and 超時操作 on a Template, in both the API and the template form. Durations are chosen from an hours dropdown (Off disables; stored as seconds). The API validates a minimum of 60 seconds and only the three actions (`remove` default), persists them, and returns them so the edit form can prefill.

**Blocked by:** 01 — Auto-Sleep schema foundation

**Status:** completed

- [ ] POST/PUT Templates accept and return `max_run_seconds` (nullable) and `timeout_action`
- [ ] Invalid input rejected with 400: `max_run_seconds` < 60, or `timeout_action` not in `remove`/`stop`/`pause`
- [ ] Template form (create + edit) has 使用時長 dropdown (Off/1h/2h/4h/8h/12h/24h) and 超時操作 dropdown (remove/stop/pause); edit prefills from the Template
- [ ] Off sends `null`; action defaults to `remove` when unset
- [ ] API route tests + form unit tests green
