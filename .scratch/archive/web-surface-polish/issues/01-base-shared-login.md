# 01 — Shared foundation, login, and dashboard gap-fill

**Track:** frontend

**What to build:** the shared design language every later milestone builds on: one button weight system, one filter-bar pattern, one empty-state voice, one confirmation pattern, and one denial explanation — plus the login page in the same skin and the dashboard's deferred gap-fill (budget progress story, overflow-menu focus handling, single middle-breakpoint rule). After this ticket, any page can be polished by composition instead of invention.

**Blocked by:** None — can start immediately.

**Status:** completed — implemented on `feature/web-surface-polish`, all gates green (vitest 481, `pnpm check` clean, E2E 5/5 + captures).

- [ ] Primary/secondary button weight is consistent wherever actions appear, and the most frequent intent dominates
- [ ] Every filter bar follows the single shared pattern (grouped filters, predictable action row, visible result count)
- [ ] Empty states teach (what belongs here, where to go next) instead of rendering blank lists
- [ ] All destructive intents in scope share one confirmation pattern; the denial explanation is identical everywhere
- [ ] Login speaks the product's visual language with authentication behavior unchanged
- [ ] Dashboard gap-fill lands: remaining budget as a legible story with progress, keyboard-correct overflow menu, one middle-breakpoint rule
- [ ] Unit tests over every new pure seam plus component tests for the shared elements and login
- [ ] Full-stack browser spec with desktop, tablet, and phone captures, all green
- [ ] Zero emoji on touched surfaces; keyboard reachability; reduced-motion honored; self-hosted artwork only
- [ ] Repository hard gates green (web typecheck plus lint, full unit suites, zero-warning compilation)
