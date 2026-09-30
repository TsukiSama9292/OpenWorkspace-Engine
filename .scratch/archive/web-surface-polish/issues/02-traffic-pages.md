# 02 — Highest-traffic pages: templates, sessions, volumes

**Track:** frontend

**What to build:** the three pages users touch most, rebuilt on the ticket-01 foundation without changing any permission, contract, or lifecycle behavior: a guided template editor with a unified resource vocabulary and explanatory locked states; an all-sessions table that leads with plain-words state plus owner identity behind the shared filter pattern; and an orphaned-volumes list with clear ownership plus its double-confirmed cleanup intact.

**Blocked by:** 01 — Shared foundation, login, and dashboard gap-fill.

**Status:** completed — implemented on `feature/web-surface-polish`, all gates green (vitest 497, `pnpm check` clean, E2E 3/3 + phone captures per page). Accepted divergence: volume size is uncomputable from the payload (no size field), so the table gained a Status column instead; review fixes (shared statusBadgeClass/familyArtwork seams, UnlimitedInput unification for time limits, reduced-motion animation kill, per-page phone captures) are in.

- [ ] Template editor sections read as one guided flow (basics, resources, advanced, environment, volumes)
- [ ] Resource inputs share one unlimited/blocked/custom vocabulary; locked templates explain why and whom to ask
- [ ] All-sessions table surfaces state in plain words plus owner identity; user/status filters match the shared pattern
- [ ] Manager actions on others' sessions reflect exactly the existing authority — no implied new power
- [ ] Volumes list shows ownership and size plainly; thorough cleanup keeps its double confirmation
- [ ] Per-page unit tests plus a per-page browser smoke including a phone-viewport capture, all green
- [ ] Zero emoji on touched surfaces; keyboard reachability; reduced-motion honored; self-hosted artwork only
- [ ] Repository hard gates green; no API, migration, permission, or lifecycle change anywhere in the diff
