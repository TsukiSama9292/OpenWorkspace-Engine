Status: completed

# Log UI Redesign — Audit Logs page + Container Log modal

## Problem Statement

The two log surfaces work functionally but their frontend layout quality is poor, which makes them feel unfinished next to the rest of the dashboard:

1. **Audit Logs page** (`#logs` tab): the filter bar is a `flex-wrap` strip cramming six filter fields plus Apply/Clear buttons plus an entry count into one container. Control widths are inconsistent (fixed-width text inputs, self-sized selects, native date inputs), so the strip wraps into ragged, misaligned rows and leaves dead space on wide screens. The table forces horizontal scrolling because timestamps render with verbose `toLocaleString()` output and the shared table chrome pins a 190px minimum on the first column. Edit-event (diff) rows are expandable by clicking the whole row, but the only affordance is a tiny 16px `+`/`−` glyph beside the action chip — users cannot discover that rows expand, and the interaction has no keyboard support or `aria-expanded` state.
2. **Container Log modal** (per-instance "Logs" button): the follow toggle streams new lines but the viewport never scrolls to the newest line, so follow is effectively broken. Long lines are forcibly wrapped (`pre-wrap` + `word-break: break-word`), destroying the alignment of terminal output, with no horizontal-scroll alternative. The modal header can overflow on narrow windows because the title has no ellipsis/truncation and the header cannot wrap. Type is small (≈0.76rem) with cramped 14×16px `O`/`E` stream letterboxes.
3. **Test gap**: neither surface has any frontend test coverage, unlike the group/user/monitor/template panels.

## Solution

Redesign both surfaces structurally, preserving the existing dark glassmorphism + zinc/indigo visual language (Skeleton wintry theme tokens, indigo `#6366f1` accent) — this is a layout/interaction overhaul, not a re-theme.

- **Audit Logs page**: the filter bar becomes a CSS grid (`repeat(auto-fit, minmax(160px, 1fr))`) so fields are equal-width and align across wrapped rows; Apply/Clear plus the entry count move out of the field grid into a separate right-aligned action row; the After/Before date fields sit adjacent as one paired group. The shared `.filter-bar` styles are upgraded with this grid (the Sessions page filter bar is visually upgraded by the same change; its behavior is unchanged). Timestamps render in a compact `YYYY-MM-DD HH:MM` format with the full `toLocaleString()` value in a `title` tooltip; the first-column minimum width constraint is lifted for the audit table. On narrow viewports (< ~900px) the low-priority IP column is hidden; the remaining table no longer needs horizontal scrolling on desktop. Diff rows gain a dedicated chevron toggle button in the Event cell (`aria-expanded`, keyboard-operable); the row body is no longer clickable. The "Load more" button is retained, now with an explicit loading state.
- **Container Log modal**: the modal is wider by default and gains a fullscreen toggle. The header truncates long instance names with an ellipsis and wraps gracefully. A Wrap toggle (default on) switches between the current wrapping mode and `white-space: pre` with horizontal scrolling for alignment-faithful output. Follow auto-scrolls to the newest line only while the viewport is pinned to the bottom; scrolling up pauses follow, scrolling back to the bottom resumes it. Each line gets a line number and a left gutter color-stripe distinguishing stdout (blue) from stderr (red), replacing the `O`/`E` letterboxes. The base font size rises to ~0.8rem with an A−/A+ control (12–16px) persisted in `localStorage`.

## User Stories

### Audit Logs page — filter bar

1. As a viewer, I want the six audit filters to sit in an evenly-aligned grid, so that the filter bar reads as one tidy strip instead of a ragged flex-wrap pile.
2. As a viewer, I want the After/Before date fields to sit adjacent to each other as one paired group, so that a date range is visually one thing.
3. As a viewer, I want Apply/Clear and the entry count on a separate right-aligned action row, so that actions never get stranded mid-wrap among the fields.
4. As a viewer, I want the filter controls to share consistent widths and vertical alignment across wrapped rows, so that no row looks broken or misaligned.
5. As a viewer, I want the Sessions page filter bar to receive the same grid treatment automatically, so that the dashboard does not contain two inconsistent filter-bar styles.

### Audit Logs page — table

6. As a viewer, I want timestamps shown compactly (`2026-08-08 15:14`) with the full locale time available on hover, so that the Time column stops forcing horizontal scrolling.
7. As a viewer, I want the audit table to fit its container on desktop without horizontal scrolling, so that Outcome and IP are visible at a glance.
8. As a viewer on a narrow screen, I want the low-priority IP column hidden rather than the table overflowing, so that the core row data stays readable without sideways scrolling.

### Audit Logs page — diff expansion

9. As a viewer, I want a clearly visible chevron toggle (`▸`/`▾`) in the Event cell of edit rows, so that I can discover that rows expand.
10. As a keyboard user, I want the chevron to be a real button that is focusable and operable with Enter/Space and announces its expanded state via `aria-expanded`, so that the audit trail is accessible without a mouse.
11. As a viewer, I want only the chevron to toggle expansion, so that clicking a row body does not accidentally expand or collapse it.
12. As a viewer, I want expanded diff content to show field name, struck-through before value, arrow, and after value, so that a template/settings edit remains traceable at a glance.

### Audit Logs page — loading & pagination

13. As a viewer, I want the "Load more" button to remain the pagination control with an explicit "Loading…" state while the next cursor page is fetched, so that loading long histories stays predictable.

### Container Log modal — follow & scrolling

14. As an instance owner, I want follow to auto-scroll to the newest line while the viewport is pinned to the bottom, so that a live build or script visibly progresses without manual scrolling.
15. As an instance owner, I want follow to pause automatically when I scroll up to read older output, so that the viewport does not yank back down while I am reading.
16. As an instance owner, I want follow to resume automatically when I scroll back to the bottom, so that the stream continues without a manual restart.
17. As an instance owner, I want a visible indicator that follow is active (and paused), so that I always know whether new output will auto-arrive.

### Container Log modal — line rendering

18. As an instance owner, I want a Wrap toggle (default on) that switches between wrapped lines and alignment-faithful `white-space: pre` with horizontal scrolling, so that long build logs and stack traces can be read in either mode.
19. As an instance owner, I want each line prefixed with a line number, so that I can reference "line N" when debugging.
20. As an instance owner, I want stderr distinguished from stdout by a colored left gutter stripe (and red text tint) instead of an `O`/`E` letterbox, so that error output is scannable at a distance.

### Container Log modal — size, type, states

21. As an instance owner, I want the modal to open larger by default and offer a fullscreen toggle, so that long output benefits from more viewing area.
22. As an instance owner, I want long instance names in the modal header truncated with an ellipsis (full name on hover), so that the controls never get pushed out of view on narrow windows.
23. As an instance owner, I want an A−/A+ font-size control (12–16px) whose choice persists across sessions, so that I can tune density to my display.
24. As an instance owner, I want the empty / streaming / ended / error states to keep the existing clear messaging with the redesigned chrome, so that a dead session is still debuggable.

## Implementation Decisions

### 1. Design direction (structure-first, existing language)
- Keep the dark glassmorphism + zinc palette + indigo accent and the Skeleton wintry tokens. The redesign touches layout and interaction only. Dials: `DESIGN_VARIANCE 4`, `MOTION_INTENSITY 3`, `VISUAL_DENSITY 7`.
- Motion is limited to the existing live-dot pulse and a short expand/collapse transition on diff rows; all animation respects `prefers-reduced-motion`.
- The shared `.filter-bar` styling is upgraded to the new grid so the Sessions page filter bar inherits the same alignment (behavior unchanged). All other surfaces are untouched.

### 2. Audit filter bar → CSS grid + separated action row
- The filter fields move into a CSS grid with `repeat(auto-fit, minmax(160px, 1fr))` tracks and a consistent vertical gap; the date pair sits adjacent within one group.
- Apply/Clear and the entry count render in a separate right-aligned action row (no longer flex items among the fields).
- Controls share a single width/font/height vocabulary (text inputs, selects, date inputs sized consistently).

### 3. Audit timestamp formatting (pure function)
- New pure helper formats an audit timestamp as `YYYY-MM-DD HH:MM` for display and keeps the full locale string for the `title` attribute.
- The audit table no longer applies the shared first-column minimum width; columns size to content so desktop fits without horizontal scroll.

### 4. Responsive column strategy
- Below ~900px the audit table hides the IP column via a `matchMedia('(max-width: 899px)')`-driven `ip-hidden` class on the table (the column markup remains in the DOM for screen readers). This is a documented deviation from the original "pure CSS media query" wording: a JS-driven class is directly assertable in jsdom, whereas a CSS-only rule cannot be tested there — and this SPA always runs JS anyway. No stacked-card transform; the table keeps its structure, matching the dashboard's other tables.

### 5. Diff expansion → dedicated chevron toggle
- Edit rows carry a chevron button in the Event cell; only the button toggles expansion. It is a native `<button>` with `aria-expanded`/`aria-controls`, focusable and keyboard-operable. The row body is not clickable. Expanded diff content renders beneath the row (colspan) with before/after value styling retained.

### 6. Container log modal — size & fullscreen
- Default size grows to about `min(900px, 92vw)` wide by `min(82vh, …)` tall; a header toggle expands the modal to near-fullscreen and back. The header is allowed to wrap/truncate long instance names (ellipsis + `title`).

### 7. Follow → pinned-to-bottom autoscroll (pure helper)
- A pure helper (`shouldAutoscroll`) decides whether the viewport is pinned to the bottom (within a small threshold). The scroll handler feeds it into a `pinned` state; a reactive effect scrolls to the newest line whenever the line list changes while `pinned` is true, and re-anchors when the Wrap toggle or font size changes. Scrolling manually to the bottom re-pins and resumes autoscroll.
- The streaming indicator reflects the state accurately: "streaming" while following, "paused — scroll to bottom to resume" only when follow is on but the viewport was scrolled up, and "static" whenever the stream is not following (follow switched off, or the stream ended).

### 8. Wrap toggle + horizontal scroll
- A header toggle (default on) drives a line-layout mode: wrap mode keeps `white-space: pre-wrap` without mid-token breaking; no-wrap mode uses `white-space: pre` and enables horizontal scrolling on the log body so column alignment is preserved.

### 9. Line chrome: numbers + stream gutter
- Each line renders a line number (computed client-side from the running index) and a left gutter stripe colored by stream (stdout blue / stderr red), replacing the `O`/`E` letterboxes. stderr lines keep a red text tint.

### 10. Font-size control (persisted)
- A header A−/A+ control adjusts the log body font size across a 12–16px range, persisted in `localStorage` under a single key shared by all instances' log modals.

### 11. Minimal backend touch (E2E support)
- The SSE log payload (`stream` + `text`) is unchanged; timestamps per log line remain out of scope (would require backend changes). The audit query API is unchanged.
- One backend fix landed to support E2E verification: template update now accepts omitted config fields (`gpu_count`, `run_config`, `exec_config`, `volume_mappings`) just like create does, so Playwright fixtures can update the template they created without resending every field.

## Testing Decisions

A good test asserts externally observable behavior — the formatted time string, the autoscroll decision, the rendered grid/chevron/font-size, the toggled wrap mode — never component internals.

- **Pure helper tests (highest seam)**: `formatAuditTime` (compact format, `title` full format, invalid-input fallback), the autoscroll helper (bottom-pinned → true; above threshold → false; threshold boundary), and the font-size persistence helper (default, clamp range, round-trip through `localStorage`). Prior art: the pure unit tests of the ansi / format / sse modules.
- **Component tests (fills the current zero-coverage gap)**: render the audit-logs panel and the container-log modal with `@testing-library/svelte` and assert: the filter bar renders the grid fields plus a separate action row; an edit row shows a chevron button that toggles the diff content and flips `aria-expanded`; the compact time format appears in a row; the narrow-viewport rule hides the IP column; in the modal, appending lines at the bottom keeps the viewport pinned while an upward scroll pauses follow; the Wrap toggle switches the line-layout mode; the A−/A+ control changes the font size and persists. Prior art: the group-panel / monitor-panel / user-panel component tests. The narrow-IP and wrap-mode assertions are class-level (jsdom cannot compute CSS rules); the CSS itself is verified visually in the manual pass.
- **Gates**: web `pnpm test` and `pnpm check` (typecheck + eslint) must stay green; the Playwright smoke suite is re-run to confirm no dashboard regression. No backend test changes (no backend change).
- **Manual verification**: with the dev stack up, open a long-running instance's logs, enable follow, scroll up and confirm follow pauses, then back to bottom and confirm it resumes; open a stopped instance's logs and confirm the ended state and tail render.

## Out of Scope

- Per-line log timestamps in the SSE payload (would require a backend API change).
- Re-theming the dashboard or introducing a new palette/design system.
- Redesigning the other panels (Templates editor, Monitor, Groups, Users, Volumes, Settings) or the login / instance-detail / VNC viewer pages.
- Changing the Sessions filter behavior (visual upgrade via the shared styles only).
- Light-mode support.
- Infinite scroll or paged-number navigation for the audit list.

## Further Notes

- **Doc touchpoints on delivery**: `docs/user-guide/frontend.md` ("Viewing the audit trail" and "Reading a session's output" sections), `roadmap.md`, `CHANGELOG.md`.
- **Accessibility**: chevron buttons are real buttons with `aria-expanded`; the pause-follow indicator is conveyed not only by color; the live-dot pulse and diff transition honor `prefers-reduced-motion`; control text keeps WCAG AA contrast against the dark surface.
- **Tickets**: split as `01-fe-log-redesign` (all frontend work + the new vitest coverage) and `02-int-log-redesign` (Playwright regression verification), per the agreed two-ticket shape for this pure-frontend change.
