# 02 — Frontend: rail plus hero, rich session cards, and true-mark catalog

**Track:** frontend

**What to build:** A general user gets a pinnable navigation rail with an overlay drawer on narrow screens, tooltips and count badges in compact mode, and a truthful identity control. The personal dashboard reads as three stages: a greeting hero with global search, quota chip, and three totals; a session collection of rich cards with a dominant open intent, grouped secondary intents, confirmed removal, and a legible state-plus-budget story; and a template catalog of large cards with genuine family marks on distinctive covers, descriptions, resource facts, and explicit launch or locked-with-reason states. All marks are self-hosted vectors served with long-lived immutable caching and no external network calls. Other management surfaces keep their arrangement and only adopt the shared skin tokens. Narrow, middle, and wide viewports plus keyboard operation and reduced-motion handling all work.

**Blocked by:** 01 — Backend baseline: prove no API change is needed.

**Status:** completed (2026-09-29) — TDD vertical slices on `feature/sidebar-dashboard-redesign`, all gates green.

- [x] Rail defaults expanded on wide screens, persists the collapsed preference, peeks with labels on demand, and becomes a drawer on narrow screens.
- [x] Hero search filters both collections, quota and totals reflect the existing effective context values, and the empty state teaches the next step.
- [x] Session cards emphasize open, group secondary intents, confirm removal, and show state plus budget progress without changing lifecycle behavior.
- [x] Template cards show true marks, cover treatments, descriptions, resource facts, and locked-with-reason behavior reusing the existing denial path.
- [x] No emoji marks remain on the redesigned surfaces and every mark loads locally with immutable caching.
- [x] Typecheck plus lint and the frontend test suite are green with no arrangement regressions elsewhere.

Notes:
- New deep modules: `src/lib/dashboard/filter.ts` (query+status→views), `src/lib/dashboard/artwork.ts` (family+`/icons/*.svg`), `src/lib/dashboard/navigation.ts` (collapsed pref in localStorage). Hover is replaced by an explicit toggle (source of truth) + `title` on-demand labels + drawer on narrow.
- `+page.svelte`: hero (greeting/search/totals/chips), session rich cards (primary Open + dropdown overflow menu + state story, same lifecycle handlers), catalog cards (SVG cover, facts via `formatMemory`, locked click reuses `launchInstance`→`RejectionNotice`).
- Static tier: `static/icons/{ubuntu,python,pytorch,rust,jupyter}.svg` are official Simple Icons brand paths (CC0) in monochrome treatment + in-house `generic.svg`; grants/risks recorded in `static/icons/README.md`; nginx `/icons/` immutable 30d, `pnpm build` emits `build/icons/`. Covers brightened (white 52px mark) after review.
- Gates: `pnpm check-types` 0 errors, `pnpm lint` clean, `pnpm test` 37 files / 460 passed (new: dashboard-filter/artwork/navigation/redesign).

Code-review fixes (2026-09-29, Standards 6 + Spec 12 → all addressed):
- Standards: `budget*` renamed to `state*`; `countSessionsByStatus` in
  `filter.ts` backs both hero totals and rail badges; `quickLaunchTemplates`
  no longer pre-filters hidden (single owner: `filterDashboard`);
  `dashboardStatus` reuses `DashboardStatusFilter`; `familyCoverClass`
  centralizes cover styling. `navigation.ts` kept as the test seam.
- Spec(a): hero quota chip (`used/ceiling sessions used`); catalog facts add
  persistent/ephemeral storage (the Template model has no disk-size field);
  empty state carries the generic mark; every rail entry has a collapsed
  `title` (+ aria counts on Instances/Sessions); Escape closes drawer and
  account panel via `svelte:window`; Sessions entry carries the running
  badge; `static/icons/README.md` records CC0 in-house authorship.
- Spec(b): locked click uses the current `launchPersistence`/`launchGroup`
  and only ever surfaces the existing `RejectionNotice` (no `alert`);
  `stopped` chip now means non-running so paused/error/starting sessions
  stay visible.
- Spec(c): the fabricated `role="progressbar"` is removed — the Instance
  payload carries no total duration, so an honest percentage is
  uncomputable; the state story (`sleepLabel ?? status`) remains. The 1024px
  force-collapse override is removed so the persisted preference is the
  source of truth at every width (drawer still takes over ≤640px). Locked
  reason names the whitelist cause (the hidden branch was dead:
  `filterDashboard` never shows hidden templates).
- Final review (branch vs main, two-axis): Standards 0 hard + 5 smells
  (fixed: shared bool-env parser, shared cookie builder, named fixture
  struct, `contains_case_insensitive` rename); Spec accepted divergences
  recorded — Story 17 progress omitted (uncomputable), Story 20 disk
  substituted with storage kind, Story 26 middle-breakpoint compact dropped
  in favor of the persisted preference, Story 28 reduced-motion covers
  rail transitions + all animations only, Story 5 badge shows only when
  count > 0, dropdown menu is hand-rolled (Escape + scrim + aria roles, no
  focus trap).
