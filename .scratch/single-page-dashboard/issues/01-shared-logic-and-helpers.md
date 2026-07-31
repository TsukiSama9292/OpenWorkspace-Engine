# 01 — Shared template-form module & dashboard view helpers

**What to build:** A single shared module that both the create and edit flows use to manage template form state, plus the pure helpers that encode the dashboard's view state and unsaved-change detection. Pure, unit-tested infrastructure — no UI changes yet.

**Blocked by:** None — can start immediately

**Status:** resolved

- [x] One shared form-state module provides create-initial state, edit-state-from-template, load template, submit (create), and update (edit) — with no page-navigation side effects
- [x] Hash parse/serialize helpers cover every dashboard view: `#instances` (default, including empty/unknown hash), `#templates`, `#templates/new`, `#templates/edit/<id>`, `#sessions`, `#users`
- [x] Dirty-snapshot helper compares persisted fields only (excluding UI state such as `showAdvanced`/`loading`/`error`), deep-compares nested env vars and volume mappings, and normalizes numeric fields
- [x] Unit tests pass (`pnpm test`) covering form round-trips through the mocked API client, hash round-trips, and dirty detection
