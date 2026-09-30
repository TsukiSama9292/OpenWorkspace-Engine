# 03 — Administration pages: groups, users, settings

**Track:** frontend

**What to build:** the three densest administration pages, unified on the ticket-01 foundation with behavior frozen: group management reading as one coherent editor with tier-gating untouched; a user list that surfaces memberships and personal ceilings at a glance with deliberate, confirmed identity changes; and server-wide settings with plain-words explanations plus confirmation on dangerous options.

**Blocked by:** 02 — Highest-traffic pages: templates, sessions, volumes.

**Status:** completed — implemented on `feature/web-surface-polish`, all gates green (vitest 500, `pnpm check` clean, E2E 3/3 + phone captures per page). Accepted divergences: create/policy editors keep modal-submit as their deliberate step (no stacked dialogs); group editor needed no regrouping (already sectioned with hints). Review fixes (shared PendingConfirm type, modal-form chrome in the shared sheet, newlyBlockedLabels rename) are in.

- [ ] Group editor (flags, template whitelist, ceilings) reads as one coherent surface; per-member quota view stays tier-gated exactly as before
- [ ] User list surfaces group memberships and personal ceilings without opening every row; creation and membership edits confirm before committing
- [ ] Settings options carry plain-words explanations; dangerous settings ask for confirmation
- [ ] Per-page unit tests plus a per-page browser smoke including a phone-viewport capture, all green
- [ ] Zero emoji on touched surfaces; keyboard reachability; reduced-motion honored; self-hosted artwork only
- [ ] Repository hard gates green; no API, migration, permission, or lifecycle change anywhere in the diff
