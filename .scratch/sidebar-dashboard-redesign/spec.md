Status: ready-for-agent

## Problem Statement

A general user opening the dashboard sees two piles of small grey boxes: session cards distinguished only by a tiny status dot and a row of equally-weighted action pills, and a template grid of small squares distinguished only by an emoji glyph and an Allowed or Not allowed tag. The global navigation is a narrow rail that only reveals its labels on hover, shows no counts, and ends in a hardcoded placeholder identity. The result feels purely utilitarian: first-time users cannot tell where to go or what to launch, returning users cannot scan the state of their sessions at a glance, and nobody gets any sense of craft. The platform promise is that old hardware can still deliver a modern experience, but the most-visited surface does not deliver it.

## Solution

Redesign the two highest-traffic surfaces together as one coherent language: a pinnable navigation rail plus a three-stage dashboard of greeting hero, personal session collection, and template catalog. Sessions become rich cards with a clear primary action and a visible state story. Templates become large catalog cards with genuine operating-system and language marks, a one-line description, resource facts, and an explicit launch action including a locked explanation when permission is missing. All marks are self-hosted vector artwork served with long-lived caching from the static asset tier, never emoji and never a third-party network dependency. The visual language deepens the existing dark foundation with per-family duotone covers and subtle texture rather than switching themes.

## User Stories

1. As a general user, I want the navigation rail to show readable labels by default, so that I do not have to guess what each mark means.
2. As a returning user, I want to collapse the rail to a narrow strip and have it stay collapsed, so that I gain content width without re-doing the choice on every visit.
3. As a touch user, I want the rail to become an overlay drawer behind a menu control, so that hover-only expansion does not lock me out.
4. As a general user, I want hoverborn labels and counts on the collapsed rail, so that the compact mode remains navigable.
5. As a general user, I want a running-session count next to the session entries, so that I know at a glance whether anything needs attention.
6. As a signed-in user, I want the bottom of the rail to show my real name and tier rather than a placeholder, so that I trust whose session I am operating.
7. As a signed-in user, I want password change and sign-out reachable from the identity control, so that account actions live in one predictable place.
8. As a keyboard user, I want every rail entry and drawer control reachable and dismissible by keyboard, so that I can operate without a pointer.
9. As a new user with no sessions, I want an illustrated empty state pointing me at the catalog below, so that the blank page teaches rather than confuses.
10. As a returning user, I want my sessions listed above the catalog, so that the first screen answers whether my machines are alive.
11. As a general user, I want a greeting hero with a single search box filtering both sessions and templates, so that I can find things once the lists grow.
12. As a general user, I want my quota usage and available template count visible in the hero, so that a refused launch never surprises me.
13. As a general user, I want three small totals for running, stopped, and available templates, so that the page feels like a dashboard rather than two loose grids.
14. As a session owner, I want the primary open action to dominate the session card, so that the most frequent intent costs one glance and one click.
15. As a session owner, I want secondary actions grouped behind an overflow control, so that pause, stop, logs, and removal do not compete visually with opening.
16. As a cautious user, I want removal to ask for confirmation, so that a misclick cannot destroy a session.
17. As a session owner, I want state plus remaining budgeted time shown as a legible story with progress, so that I do not have to decode a colored dot.
18. As a session owner, I want the card to name its source template and short identity, so that lookalike sessions stay distinguishable.
19. As a general user, I want each template card to show a genuine family mark on a distinctive cover, so that Ubuntu, Python, PyTorch, Rust, and Jupyter are recognizable without reading.
20. As a comparing user, I want each template card to carry a one-line description plus processor, memory, and disk facts, so that I can choose without opening an editor.
21. As a general user, I want a clear launch action per template card, so that starting a session does not require discovering a hidden click target.
22. As an unauthorized user, I want a locked card to explain why it is locked and what happens on click, so that I know whom to ask instead of seeing a dead tile.
23. As a denied user, I want the existing denial explanation to appear when I attempt a locked template, so that the refusal path stays consistent.
24. As a general user, I want locked cards visually distinct yet still informative, so that the catalog teaches the permission model.
25. As an offline-environment user, I want all marks to load from the platform itself with no external network calls, so that air-gapped hosts render identically.
26. As a tablet user, I want the rail to rest in its compact form with labels on demand, so that the middle breakpoint stays usable.
27. As a phone user, I want session and template cards to stack in a single column, so that nothing overflows or crushes actions.
28. As a motion-sensitive user, I want pulsing and sliding effects suppressed under the platform reduced-motion preference, so that the redesign does not cause discomfort.
29. As a low-vision user, I want visible focus rings and contrast-preserving state colors, so that the dark深化 remains legible.
30. As an administrator, I want every management surface to keep its current arrangement and only adopt the new skin tokens, so that the redesign cannot regress admin workflows.
31. As a manager, I want permission gating of navigation and actions to behave exactly as before, so that the visual change carries no authorization change.
32. As a reviewer, I want zero emoji glyphs remaining on the redesigned surfaces, so that the craft bar is mechanically verifiable.

## Implementation Decisions

- Scope is frozen to the global rail plus the personal dashboard collection and catalog. All management surfaces keep their arrangement and only adopt shared skin tokens such as color, type scale, spacing, and radius.
- The navigation state lives in a small deep module whose interface is collapsed versus expanded plus drawer open versus closed, with persistence of the rail preference in local browser storage. Hover becomes a transient peek, never the source of truth.
- Navigation structure and permission gating are unchanged: the same workspace, access-control, and server groupings with the same visibility rules derived from the effective context. Added depth is limited to tooltips in compact mode, count badges on session entries, and a truthful identity control.
- The dashboard keeps a single scrolling page with three stages in fixed order: greeting hero, personal session collection, template catalog. No tab split is introduced, so the existing hash-based view model is untouched.
- The hero is a presentational module combining greeting, global search, quota chip, and three totals. Host-level resource metrics are explicitly excluded from the hero and remain in the monitoring surface.
- Session filtering including full-text search plus running versus stopped chips lives in a pure filtering module operating over the already-loaded session and template collections. The module is kept deep: a small interface accepting query plus filter state and returning ordered views, with all normalization hidden inside for leverage across the collection and the catalog.
- The session rich card is a presentational module whose interface exposes state, remaining budget, source template name, short identity, persistence mark, primary open intent, overflow intents, and removal confirmation. All actions keep their existing authorization checks and lifecycle behavior; only grouping and emphasis change.
- The template catalog card is a presentational module whose interface exposes family mark, cover treatment, name, one-line description, resource facts, launch intent, and locked state with reason. Locked attempts reuse the existing denial-notice behavior rather than inventing a new refusal path.
- The artwork system is a deep icon-resolution module extending the existing name-matching seam: normalized template naming maps to a curated local vector identity with a generic fallback. The first round performs frontend mapping only with no backend schema change; a future per-template artwork picker is deferred to its own round.
- The static asset tier serves a curated self-hosted vector collection under a dedicated long-cached immutable route. No external content network is introduced, preserving offline and air-gapped rendering and the fully static build output.
- The accessible primitive set is reused rather than rebuilt: button, card, badge, dialog, dropdown, and tooltip adapters in the spirit of the chosen open-source headless collection. Existing hand-rolled overlays adopt these adapters for focus management, dismissal, and labeling.
- The visual language deepens rather than replaces the dark foundation: the existing dark base and primary accent are kept, per-family duotone covers plus fine grid or noise texture, generous radius, and soft shadow supply the craft layer. No light theme is introduced in this round.
- The responsive contract is three breakpoints: wide shows the expanded rail, middle shows the compact rail with on-demand labels, narrow shows the drawer plus single-column cards. Motion reduction and keyboard operation are baseline requirements, not enhancements.
- Dependency discipline follows the constitution: no dependency upgrade for its own sake, and the existing component collection stays installed even where unused, so the change is additive.

## Testing Decisions

- A good test for this effort asserts externally observable behavior through the module interface, never internal markup or class names. Icon tests assert resolved identity plus fallback for unknown names. Filter tests assert visible collections for given queries. Component tests assert reachable labels, actions, counts, and denial paths with mocked effective contexts.
- The highest existing seams are preferred. The icon-resolution seam and the permission and countdown helpers remain the primary pure seams. The new filtering logic gets its own pure seam at the same level so it can be exercised without rendering. Presentational modules are verified through the established component-test setup with mocked contexts and actions.
- Prior art consulted includes the existing pure-helper tests for permissions, countdown math, billing labels, and dashboard view parsing, plus the established component-test suite for panels and the static-build plus live-stack verification habits. Static-asset serving is verified as a build artifact plus a live fetch asserting success and immutable caching, not as a unit test.
- Human visual review is an explicit acceptance seam: desktop, tablet, and phone captures of the rail, hero, session collection, catalog, empty state, and locked state are inspected for the intended engineered-yet-crafted feel. Mechanical checks guard zero remaining emoji marks on the redesigned surfaces and zero arrangement regressions elsewhere.

## Out of Scope

- Any backend schema change including a per-template artwork field, migration, or artwork picker in the template editor.
- Any rearrangement of the sessions table or the template management, monitoring, audit, group, user, volume, and settings surfaces beyond adopting shared skin tokens.
- A light theme or theme switcher, host-level metrics on the personal dashboard, announcements, recent-activity feeds, or new analytics.
- Any permission-model change: group flags, template whitelists, visibility values, ceilings, and effective-context resolution behave exactly as before.
- Forking a complete third-party dashboard template, adopting a paid design system, loading artwork from an external network, or upgrading dependencies for their own sake.
- Multi-host, virtual-machine, non-NVIDIA accelerator, or enterprise-identity work per the standing anti-goals.

## Further Notes

- The crudeness diagnosis from exploration: collapsed-only navigation with no memory, equally-weighted action pills where the most frequent and most dangerous intents look identical, small tiles that cannot carry real information so emoji fills the gap, a single accent on flat grey with no cover or illustration layer, and placeholder identity copy. Each maps directly to one decision above.
- Artwork licensing is part of definition of done: every curated mark must be traceable to a permissive grant suitable for self-hosted redistribution, with the grant recorded alongside the collection. Full-color brand collisions are avoided by rendering marks in a unified treatment over the per-family cover.
- Visual references are Linear-style rail navigation and Vercel-style dark covers, used as language inspiration only. No layout is copied wholesale, keeping the existing routing, hash views, and authorization model intact.
