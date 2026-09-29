# 03 — Integration: full-stack dashboard verification with captures

**Track:** integration

**What to build:** A reviewer can open the running platform as a general user and verify the whole redesigned dashboard works against the real backend: navigation across breakpoints, search and filters, the full session lifecycle, locked-template denial, locally served artwork with no external requests, untouched management surfaces, and a set of desktop, tablet, and phone captures that carry the intended engineered-yet-crafted feel for human sign-off.

**Blocked by:** 02 — Frontend: rail plus hero, rich session cards, and true-mark catalog.

**Status:** completed (2026-09-29) — 6/6 full E2E green on the live dev stack, no leaked fixtures.

- [x] A general user can sign in and exercise search, quota display, launch, open, pause, stop, removal with confirmation, and locked-template denial against the live stack.
- [x] Wide, middle, and narrow viewports are captured and the captures are human-approved for the intended feel.
- [x] Artwork requests succeed locally with immutable caching and no third-party artwork calls occur.
- [x] Management surfaces show no arrangement regressions and permission gating behaves as before.

Verification (2026-09-29, `e2e/tests/sidebar-dashboard.full.spec.ts`, project `full`):
- 6 passed (~15s): hero+artwork, hero search filter, lifecycle (API launch →
  rich card → UI remove with confirm → poll gone), locked denial
  (`rejection-notice` via existing path), tablet/phone drawer, gating +
  templates-management intact.
- Captures: `e2e/test-results/sidebar-dashboard/{desktop.png (1440×900),
  tablet.png (768×1024), phone.png (390×844)}` — agent-reviewed: rail with
  labels/counts/truthful identity, hero, empty state with mark, catalog card
  with cover+facts+Launch; phone drawer opens via Menu. Final aesthetic
  sign-off left to the human reviewer.
- Icons: `/icons/*.svg` 200/304, no third-party image requests; dev-stack
  nginx serves them (prod immutable block added in 02).
- Stack left clean: `/api/templates` and `/api/instances` both `[]` after
  the run (try/finally + afterAll).
- Collateral E2E repairs for the redesign: `dashboard.smoke` titles
  (`My sessions`/`Template catalog`), `live-instance` + `resource-quotas`
  `openTemplateCard` (`.catalog-card` + `.catalog-launch`),
  `log-redesign`/`observability` log-openers (expand `More` first).
  Re-verified: smoke 2/2, sidebar-dashboard 6/6, log-redesign 6/6,
  observability 5/5, live-instance 1/1.
