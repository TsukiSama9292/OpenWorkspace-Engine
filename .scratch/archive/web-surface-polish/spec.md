Status: completed — all five tickets closed on `feature/web-surface-polish`, squash-merged to `main`. Accepted divergences and out-of-scope companions are recorded per ticket.

## Problem Statement

The personal dashboard now feels crafted, but every other surface still speaks the old utilitarian dialect: dense tables with equally-weighted actions, filter bars that wrap awkwardly, empty states that say nothing, removal flows that rely on a single careless click, and a login page that never received the new skin. A user who learns the dashboard's language — dominant primary action, plain-words state story, search-everything hero — loses that fluency the moment they open Templates, Sessions, Volumes, Groups, Users, Monitor, Logs, or Settings. First-time users cannot transfer what the dashboard taught them; returning users pay a re-orientation tax on every tab; and the in-session viewer family was never part of any polish pass at all, so the most fragile surface (an open, connected session) is also the least considered.

## Solution

Extend the dashboard's proven design language across every tab and every component, one milestone at a time, without touching the permission model, the API contracts, or any lifecycle semantics. A shared foundation (buttons with clear primary/secondary weight, one filter-bar pattern, one empty-state voice, one confirmation pattern, one denial explanation) lands first together with the login page and the dashboard's deferred gap-fill; then the highest-traffic pages, then the dense administration pages, then the already-polished surfaces plus the in-session viewer family. Each milestone is independently reviewable and reversible. When the round ends, every page a user can reach feels like the same product.

## User Stories

### Shared foundation and login

1. As a general user, I want buttons across all pages to carry the same primary/secondary weight, so that the most frequent intent is always the most prominent one.
2. As a general user, I want every filter bar to share one layout pattern (filters grouped, apply/clear on a predictable action row, result count visible), so that I never re-learn filtering per tab.
3. As a new user facing an empty list, I want an empty state that explains what belongs here and where to go next, so that a blank page teaches rather than confuses.
4. As a cautious user, I want every destructive action behind the same confirmation pattern, so that a misclick cannot destroy anything anywhere.
5. As a denied user, I want the same denial explanation everywhere a permission blocks me, so that refusals stay consistent across the whole product.
6. As a first-time visitor, I want the login page to speak the same visual language as the product inside, so that signing in already feels like arriving.

### Dashboard gap-fill (Instances)

7. As a session owner, I want the remaining budgeted time shown as a legible story with progress, so that I do not have to decode a colored dot.
8. As a keyboard user, I want the overflow menu to trap and return focus correctly, so that I can operate session actions without a pointer.
9. As a tablet user, I want the middle breakpoint to follow a single predictable rule, so that the layout never surprises me between phone and desktop.

### Templates

10. As a general user, I want the template catalog to keep its recognizable family marks and resource facts, so that choosing a template stays glanceable.
11. As a template creator, I want the template editor's sections (basics, resources, advanced, environment, volumes) to read as one guided flow, so that creating a template does not feel like filling disconnected forms.
12. As a comparing user, I want resource inputs to share one unlimited/blocked/custom vocabulary, so that numeric conventions never need re-decoding per field.
13. As an unauthorized user, I want locked templates to explain why they are locked and whom to ask, so that the catalog teaches the permission model.

### Sessions (all-sessions table)

14. As a session manager, I want the all-sessions table to lead with state in plain words plus owner identity, so that I can scan who needs attention.
15. As a session manager, I want user and status filters to behave like every other filter bar, so that finding one session among many costs one glance.
16. As a manager acting on someone else's session, I want the available actions to reflect exactly what I am allowed to do, so that the visual change never implies new authority.

### Volumes

17. As a general user, I want orphaned persistent data listed with clear ownership and size, so that I understand what is kept and why.
18. As a cautious user, I want the thorough cleanup flow to keep its double confirmation, so that reclaiming storage never happens by accident.

### Groups

19. As an administrator, I want group management (flags, template whitelist, ceilings) to keep its current information but read as one coherent editor, so that group setup stops feeling like separate panels.
20. As a manager, I want the per-member quota view to stay tier-gated exactly as before, so that the polish carries no authorization change.

### Users

21. As a user manager, I want the user list to surface group memberships and personal ceilings at a glance, so that account review does not require opening every row.
22. As a user manager, I want account creation and membership edits to confirm before committing, so that identity changes stay deliberate.

### Settings

23. As an administrator, I want server-wide options to read as labeled settings with plain-words explanations, so that a host limit is never a bare number.
24. As an administrator, I want dangerous settings to ask for confirmation, so that a slip cannot reconfigure the platform.

### Monitor and logs (light touch)

25. As an operator, I want the monitor surface to keep its interactive time-series and sparklines untouched in behavior, so that the polish cannot regress measurement.
26. As an auditor, I want the audit-trail filters, diff expansion, and follow-mode log viewer to keep their current interactions, so that the previous polish pass is preserved and only visually unified.

### In-session viewer family

27. As a session user, I want the viewer toolbar (open desktop, terminal, notebook) to share the product's button weight, so that the most frequent intent dominates.
28. As a session user, I want clipboard, status, and viewer settings to keep their exact behavior with a unified skin, so that nothing about a live connection changes meaning.
29. As a motion-sensitive user, I want the viewer's indicators to respect the reduced-motion preference, so that a live session never causes discomfort.

### Cross-cutting

30. As a phone user, I want every table to degrade gracefully to stacked or horizontally safe layouts, so that nothing overflows or crushes actions.
31. As a keyboard user, I want every dialog, drawer, and menu across all pages reachable and dismissible by keyboard, so that I can operate without a pointer.
32. As a low-vision user, I want visible focus rings and contrast-preserving state colors on every surface, so that the dark language stays legible.
33. As a motion-sensitive user, I want pulsing and sliding effects suppressed under the reduced-motion preference on every surface, so that no page causes discomfort.
34. As an offline-environment user, I want all marks on every page to load from the platform itself, so that air-gapped hosts render identically.
35. As a reviewer, I want zero emoji glyphs on every polished surface, so that the craft bar stays mechanically verifiable.
36. As an operator, I want all quality gates green after every milestone, so that polish never trades away stability.

## Implementation Decisions

- Scope is layered in two tiers. Tier one covers the eight management surfaces plus the login page and the shared presentational elements. Tier two is the in-session viewer family, scheduled last because it is the only surface where a defect can sever a live connection. The personal dashboard is not redesigned; it receives only its deferred gap-fill items.
- Depth is defined as structure-may-change, flow-frozen. Outer arrangement (column order, filter-bar composition, card versus row treatment, empty states, confirmation steps) may change; permission derivation, request/response contracts, and lifecycle semantics (launch, start, stop, pause, resume, removal, persistence handling) are frozen and keep their existing checks.
- The visual language deepens the established dark foundation (existing color, type scale, spacing, and radius tokens) rather than introducing a new theme. Per-surface accent treatments follow the dashboard precedent instead of inventing new dialects.
- Execution runs in four milestones with a strict dependency order: foundation plus login plus dashboard gap-fill first, highest-traffic pages second, dense administration pages third, already-polished surfaces plus the viewer family last. Each milestone lands on its own branch and merges independently, so any milestone is reviewable and revertible on its own.
- Seams follow the established deep-module vocabulary. Existing pure seams (dashboard filtering, family artwork resolution, rail preference persistence, denial-notice composition, countdown formatting) are reused wherever a surface needs the same behavior. New seams are introduced only where no existing seam fits, at the highest level possible: one table-view seam (query plus filter state in, ordered views out), one form-section seam (field state in, validated drafts out), and one confirmation-dialog seam (intent in, confirmed-or-cancelled out). The ideal remains one seam per surface; shared seams are always preferred over per-page copies.
- Navigation structure and permission gating are unchanged throughout. Added depth is limited to presentational concerns: tooltips, count badges, truthful identity, and explanatory locked states.
- The login surface adopts the shared skin and button weight but keeps its authentication behavior byte-for-byte, including session lifetime and redirect semantics.
- The viewer family keeps its connection, clipboard, and settings behavior exactly; only toolbar weight, status presentation, and skin tokens change.

## Testing Decisions

- A good test targets externally visible behavior (filtering results, rendered states, action availability, confirmation gating) rather than component internals or markup structure.
- Tiered verification was agreed per milestone. The foundation milestone carries the highest bar: unit tests over every new pure seam, component tests for the shared elements and login, plus a full-stack browser spec with desktop, tablet, and phone captures. The traffic and administration milestones each carry unit tests plus a per-page browser smoke including a phone-viewport capture. The already-polished surfaces carry unit tests plus regression runs of their existing specs. The viewer family carries unit tests plus a manual live-connection checklist, with no automated live-connection tests.
- Prior art: the dashboard round's pure-seam tests (filtering, artwork, navigation preference) plus component tests are the template for all new unit coverage; the dashboard full-stack browser spec is the template for the foundation milestone's browser coverage; the audit-log and monitor specs are the regression baseline for the light-touch surfaces.
- Four rules apply to every milestone with no exceptions: zero emoji glyphs on polished surfaces, keyboard reachability and dismissal for every dialog/drawer/menu, reduced-motion suppression, and self-hosted artwork with no third-party network dependency.
- The repository's hard gates (zero-warning compilation across feature sets, typecheck plus lint for the web layer, full unit suites) must be green before any milestone merges.

## Out of Scope

- Any change to the permission model, group flags, tier derivation, template whitelists, or ceiling enforcement.
- Any API contract, payload, migration, or lifecycle-semantics change; this round is presentation-only.
- New product features (new settings, new filters with backend support, new viewer capabilities, new resource dimensions).
- Theme switching, internationalization, or a new visual identity beyond the established dark language.
- Automated live-connection tests for the viewer family (covered by the manual checklist instead).
- Backend, packaging, deployment, or infrastructure changes of any kind.

## Further Notes

- Work is tracked as one spec with five tickets: foundation (shared elements, login, dashboard gap-fill), traffic pages, administration pages, polish plus viewer family, and a final cross-page regression round that runs only after all four milestones have merged.
- Each ticket is implemented in one continuous pass on its own branch and squash-merged after review; the reporter accepts each ticket independently before the next begins.
- If a milestone uncovers a defect in an already-merged milestone, the fix lands as a forward fixup on the current branch rather than rewriting merged history.
- Captures from the browser specs serve as the visual record of the round and live alongside the specs.
