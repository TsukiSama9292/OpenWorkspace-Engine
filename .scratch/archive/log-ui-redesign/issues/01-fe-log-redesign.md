# 01 — Log UI Redesign: frontend

**Track:** frontend

**What to build:** the full frontend redesign of the two log surfaces — the Audit Logs page and the per-instance Container Log modal — so that both render with clean, aligned layout and usable interactions while keeping the existing dark glassmorphism + zinc/indigo visual language. The shared filter-bar styles are upgraded as part of this so the Sessions page filter bar inherits the same alignment. No frontend re-theme; the SSE log payload (`stream` + `text`) and the audit query API stay exactly as they are. The only backend touch in the feature is the template-update fix supporting E2E verification (recorded in ticket 02).

- **Audit Logs page**
  - Filter bar → CSS grid (`repeat(auto-fit, minmax(160px, 1fr))`) with consistent control widths and vertical alignment across wrapped rows; Apply/Clear plus the entry count move to a separate right-aligned action row; the After/Before date fields sit adjacent as one paired group.
  - Timestamps render compact (`YYYY-MM-DD HH:MM`) with the full locale string in the `title` attribute.
  - The audit table no longer applies the shared first-column minimum width, so desktop fits without horizontal scrolling.
  - Below ~900px the low-priority IP column is hidden via CSS (markup stays in the DOM for screen readers).
  - Edit (diff) rows gain a dedicated chevron toggle button in the Event cell — native button, `aria-expanded`/`aria-controls`, keyboard-operable; the row body is no longer clickable. Expanded diff content keeps the before/after value styling.
  - "Load more" retained as the pagination control with an explicit "Loading…" state.
- **Container Log modal**
  - Default size grows to about `min(900px, 92vw)` wide by `min(82vh)` tall, plus a header fullscreen toggle.
  - Header truncates long instance names with an ellipsis (`title` for the full name) and wraps gracefully on narrow windows.
  - A Wrap toggle (default on): wrap mode keeps readable wrapping without mid-token breaks; no-wrap mode uses `white-space: pre` with horizontal scrolling on the log body.
  - Follow auto-scrolls to the newest line only while the viewport is pinned to the bottom; scrolling up pauses follow, scrolling back to the bottom resumes it, with the streaming indicator reflecting the paused state.
  - Each line renders a line number and a left gutter color-stripe (stdout blue / stderr red), replacing the `O`/`E` letterboxes; stderr keeps a red text tint.
  - A header A−/A+ font-size control (12–16px) persisted in `localStorage` under one shared key.
  - Empty / streaming / ended / error states keep their messaging with the redesigned chrome.
- **Tests (fills the current zero coverage on both components)**
  - Pure helper tests: compact time formatting, the pinned-to-bottom autoscroll decision, and the font-size persistence (default, clamp, round-trip).
  - Component tests (mirroring the group-panel / monitor-panel precedents): filter grid renders fields plus a separate action row; chevron toggles the diff and flips `aria-expanded`; compact time appears in rows; narrow-viewport rule hides the IP column; appended lines keep the viewport pinned at the bottom while an upward scroll pauses follow; the Wrap toggle switches line layout; A−/A+ changes the font size and persists.
- **Accessibility**: chevron buttons are real buttons; the follow-paused state is conveyed beyond color alone; the live-dot pulse and diff transition honor `prefers-reduced-motion`; control text keeps WCAG AA contrast on the dark surface.

**Blocked by:** None — can start immediately.

**Status:** completed

- [x] Filter bar renders as a CSS grid with a separated, right-aligned action row; Sessions filter bar inherits the same treatment.
- [x] Audit rows show compact `YYYY-MM-DD HH:MM` timestamps with full format in `title`; desktop table fits without horizontal scroll; IP column hidden below ~900px (via a `matchMedia`-driven class, not a pure CSS media query — documented in spec Decision 4).
- [x] Edit rows expose a keyboard-operable chevron with correct `aria-expanded`; only the chevron toggles the diff.
- [x] Log modal: follow pins to the newest line at the bottom, pauses on upward scroll, resumes at the bottom; the indicator reflects the paused state (and follow-off shows "static", not the pause hint).
- [x] Wrap toggle (default on) switches to `white-space: pre` + horizontal scroll when off.
- [x] Lines show line numbers and blue/red gutter stripes; A−/A+ adjusts font size and persists across sessions; fullscreen toggle works.
- [x] `svelte-check` and the full Vitest suite are green (new pure-helper + component tests included); `analysis:web` adds no warnings from the new files.

**Delivered:** commits `fcd7b12` … `a249d0f` on `feature/log-ui-redesign`; docs synced (`frontend.md`, `roadmap.md`, `CHANGELOG.md`, spec Decisions 4 & 7).
