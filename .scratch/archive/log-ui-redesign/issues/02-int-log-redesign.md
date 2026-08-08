# 02 — Log UI Redesign: end-to-end verification

**Track:** integration

**What to build:** full-stack verification that the redesigned log surfaces work against a running dev stack — the UI changes of ticket 01 in the same browser session against the real API, plus regression of the existing Playwright smoke suite so the dashboard as a whole stays healthy. Verification-focused; the only backend change in the feature is the template-update fix (template update accepts omitted config fields like create) that lets the fixture update a template it created without resending every field.

- **Redesigned Audit Logs page (against live data)**
  - Filter bar: six filters render in an aligned grid with the date pair adjacent; Apply/Clear and the entry count sit on the right-aligned action row; applying an action/actor/outcome filter and clearing it round-trips against the real audit query endpoint.
  - Rows show compact timestamps; an edit event's chevron expands to reveal the redacted before/after diff and toggles back; the toggle is operable by keyboard.
  - "Load more" walks at least one cursor page on a populated trail.
- **Redesigned Container Log modal (against a live instance)**
  - Open a running instance's logs: follow pins to the newest line; scrolling up pauses follow (indicator reflects it) and scrolling back to the bottom resumes it; the Wrap toggle switches between wrapped and alignment-faithful horizontal-scroll modes; A−/A+ changes the font size and the choice survives reopening the modal; the fullscreen toggle expands and restores.
  - Open a stopped instance's logs: the tail renders plus the ended-state banner with its reason.
- **Regression**: the existing Playwright smoke (login / dashboard / permission-gated tabs) stays green against the dev stack.

**Blocked by:** 01 — Log UI Redesign: frontend

**Status:** completed

- [x] Audit filter grid, action row, chevron diff expansion, and cursor pagination verified against live data.
- [x] Container log follow pause/resume, Wrap toggle, font-size persistence, and fullscreen toggle verified against a live instance; stopped-instance ended state verified.
- [x] Existing Playwright smoke suite stays green.
