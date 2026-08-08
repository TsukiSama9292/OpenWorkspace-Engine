Status: completed

# Monitor Dashboard Optimization (interactive time-series)

## Problem Statement

The Resource Monitoring Dashboard (see the completed spec at
`.scratch/archive/monitor-dashboard/spec.md`) shipped static sparklines and a
coarse 1h / 24h range toggle. An operator investigating a spike cannot zoom
into it, cannot read an exact value off the chart, and must page back and
forth between the two granularities. Instance rows carry only a tiny sparkline
with no way to inspect one workload's history in detail.

This iteration makes the charts interactive: a single 24-hour time axis that
auto-switches resolution as the operator zooms, hover / click / drag
interactions on host and instance charts, and an instance-detail modal for a
chosen workload. The sampling and storage machinery (in-memory two-tier
`MetricsStore`, 15 s fine tier / 5 min coarse tier, 5 s poll) is unchanged.

## Solution

A single 24-hour interactive time axis replaces the 1h / 24h toggle. Zooming
automatically selects the right resolution: fine (15 s) points for the portion
of the window that falls inside the last hour, coarse (5 min) points elsewhere.
The chart is a hand-written SVG component (no chart library, consistent with
the SPA's no-dependency rule).

- **Top — three enlarged host cards** (CPU / RAM / Disk): each is a full
  interactive chart (~180 px tall, 3-across). Drag to zoom, hover for a
  crosshair + value, click to pin, drag-select shows live range stats
  (start/end, average / max / min) and auto-zooms on release.
- **Below — the "Active Instances" table**: unchanged columns (owner,
  instance, template, runtime, uptime, CPU %, RAM used/limit). Each row's
  sparkline gains a light hover tooltip and a click-to-pin highlight; each row
  gains a detail affordance that opens an **instance-detail modal**.
- **Instance-detail modal**: overlays the page, shows the instance's **CPU and
  memory** as two full interactive charts (same interactions as the host
  charts), reading from the same snapshot already fetched by the panel.
- **Time axis defaults**: the full 24 h window with the right edge
  auto-following "now" (new data extends the view). Panning or zooming
  disengages follow; a "back to now" button re-engages it. A subtle marker on
  the axis marks the 1 h boundary where fine data begins.
- **Overview sparklines** (table rows) get light hover tooltips and
  click-to-pin only — the full interactive treatment lives in the enlarged
  host charts and the detail modal.

The API returns **timestamped points for both tiers** in a single snapshot
(no new endpoint, no extra poll). The frontend merges the fine and coarse
series per metric based on the visible window; the detail modal and host
charts share the same fetched data.

## User Stories

### Interactions
1. As an **operator**, I want to hover over an interactive chart to see a crosshair and the exact value + timestamp at that point, so that I can read precise numbers off a trend.
2. As an **operator**, I want to click a point to pin its readout, so that I can compare a value across charts without keeping the mouse still.
3. As an **operator**, I want to drag across a chart to select a time range and see live stats (start/end, average / max / min) for the selection, so that I can quantify a spike or a leak while still holding the mouse.
4. As an **operator**, I want releasing a drag to zoom the chart to the selected range, so that I can drill into an incident in one gesture.
5. As an **operator**, I want a single time axis whose resolution follows my zoom window (fine 15 s inside the last hour, coarse 5 min outside), so that I never have to pick a granularity by hand.
6. As an **operator**, I want zoom bounded to roughly 5 minutes minimum and the full 24 hours maximum, so that I cannot zoom into meaningless sub-sample noise or past the data.

### Follow / reset
7. As an **operator**, I want the 1-hour boundary on the axis marked, so that I can tell where the fine-grained data begins.
8. As an **operator**, I want the charts to default to the full 24 h window with the right edge following the newest data, so that a fresh view always shows the latest state.
9. As an **operator**, I want a "back to now" button that re-engages follow and resets the window, so that I can recover after investigating the past.

### Host charts
10. As an **operator**, I want the three host cards to be interactive charts (~180 px tall, 3-across) with the full interaction set, so that I can inspect host CPU / RAM / disk history without opening anything else.
11. As an **operator**, I want the host cards to keep updating live (existing 5 s poll), so that the enlarged charts reflect reality while open.

### Instance detail
12. As an **operator**, I want each table row to open an instance-detail modal, so that I can focus on a single workload's history.
13. As an **operator**, I want the modal to show CPU and memory as two interactive charts with the full interaction set, so that I can trace a memory leak or CPU spin precisely.
14. As an **operator**, I want the modal to reuse the already-fetched snapshot, so that it opens instantly without a new request.

### Overview sparklines
15. As an **operator**, I want the small row sparklines to show a light hover tooltip with value + time, so that I can read a point without opening the modal.
16. As an **operator**, I want clicking a row sparkline to pin its highlight, so that the point readout stays visible while I look elsewhere.

### Data contract
17. As a **platform developer**, I want the snapshot to return timestamped points for both tiers per metric (fine and coarse), so that the frontend can switch resolution by window without another request.
18. As a **platform developer**, I want the existing endpoint and its `can_view_monitoring` gating to stay exactly as they are, so that no new auth surface is introduced.
19. As a **platform developer**, I want the frontend to compute drag stats from the displayed points, so that the numbers shown always match what the operator sees.

## Implementation Decisions

### 1. Snapshot payload: timestamped two-tier series (breaking shape change)
- Series fields change from `number[]` to point arrays `{ t: number; v: number }[]`.
- Per entity (host + each instance): `cpu_fine`, `cpu_coarse`, `mem_fine`,
  `mem_coarse`; the host additionally carries `disk_fine`, `disk_coarse`.
  Current values and limits (cpu_percent, cpu_limit_percent, mem_used_bytes,
  mem_limit_bytes) are unchanged, so the existing value cells keep their source.
- `metrics.rs` `from_metrics` / snapshot assembly gains a pure step that emits
  fine points from Tier 1 and coarse points from Tier 2 (mean values) with
  their wall-clock timestamps. No new endpoint, no change to sampling cadence
  or `MetricsStore` internals; `can_view_monitoring` gating is untouched.
- Instance-detail (CPU + memory only) needs nothing extra — the same snapshot
  already carries both tiers for every listed instance.

### 2. Frontend chart math as pure functions (new module, kept small)
- A new `apps/web/src/lib/chart/` module holds pure SVG/time math: time→x
  domain mapping, series→path, nearest-point lookup, fine/coarse merging by
  window, drag selection → zoom window, zoom clamping (min ≈ 5 min, max 24 h),
  follow-state transitions. These are unit-testable without a DOM, mirroring
  how the API separates pure modules from route handlers.
- Merging rule: for a visible window `[start, end]`, draw coarse points inside
  the window plus fine points for the portion of the window inside the last
  hour; a window entirely older than one hour draws coarse only. The 1 h
  boundary marker is derived from the same rule.

### 3. `TimeSeriesChart` component (new component, hand-written SVG)
- A reusable interactive SVG chart component implementing: hover crosshair +
  point readout, click-to-pin, drag highlight + live range stats
  (start/end timestamps, average / max / min of the displayed values over the
  selection), auto-zoom on release, follow/back-to-now, zoom clamping, and the
  1 h boundary marker. Pointer events (`pointerdown`/`pointermove`/`pointerup`)
  drive all interactions; no chart library, no new dependency.
- The enlarged host cards and the modal's CPU/memory charts both render this
  component; the small row sparklines keep the existing `Sparkline` component
  and gain only the light hover tooltip + click-to-pin.

### 4. `MonitorPanel` layout changes
- The 1h / 24h range toggle is removed; the top three host cards become
  enlarged interactive charts (~180 px tall, 3-across, full width).
- Table rows gain a detail affordance (e.g. a per-row button / row click)
  that opens the instance-detail modal; the modal reads from the snapshot the
  panel already holds and renders two `TimeSeriesChart`s (CPU, memory).
- The 5 s poll is retained unchanged.

## Testing Decisions

A good test asserts externally observable behavior — the returned points, the
window math, the rendered charts and their response to pointer input — never
internal storage layout. All seams already exist; no new seam is introduced.

- **Pure chart math (vitest, no DOM)**: time↔x mapping; fine/coarse merging for
  windows inside / across / outside the last hour; nearest-point lookup; drag
  selection → zoom window incl. direction-agnostic min/max; zoom clamping at
  ~5 min and 24 h; follow engage/disengage transitions.
- **`TimeSeriesChart` component (vitest + happy-dom, pointer events)**:
  hover moves the crosshair and readout; click pins it; drag highlights the
  selection and shows start/end + avg/max/min stats, and releasing zooms the
  window; zoom clamp refuses an over-small window; "back to now" resets and
  re-engages follow; the 1 h boundary marker renders for an all-day window and
  hides when fully inside the last hour.
- **`MonitorPanel` component (vitest)**: renders three enlarged host charts;
  rows keep the existing columns and sparklines; a row opens the modal; the
  modal shows CPU and memory charts without a new fetch; the 1h/24h toggle is
  gone.
- **Sparkline tooltip / pin (vitest)**: hover shows value + time; click pins
  the highlight.
- **Backend pure (unit)**: `from_metrics` emits `{t, v}` fine and coarse points
  with correct timestamps for host + instances.
- **Endpoint (integration, mock docker)**: snapshot payload shape now carries
  `*_fine` / `*_coarse` point arrays; auth gating unchanged (200 admin /
  flagged manager, 403 unflagged, 401 anonymous) — updated in the existing
  `monitor_test.rs` rather than new tests.
- **Gates**: `bash scripts/check.sh` silent (both feature sets), web
  `pnpm check`, full `run_tests.sh` (docker feature), and web `pnpm test` green
  before the change is considered done.

## Out of Scope

- Streaming (WebSocket) updates; alerting / notifications.
- Instance disk metrics (host disk card covers the 95 % case; instance disk is
  not collected).
- Persisting metrics to the database, or history beyond 24 hours (in-memory
  store still wipes on API restart — a known platform limitation).
- Multi-instance selection / overlay comparison in one chart.
- Touch-specific gestures beyond what pointer events provide; keyboard
  accessibility of chart interaction (readouts are also visible in the table).
- GPU (NVProxy) metrics.

## Further Notes

- **Payload size**: the snapshot now carries both tiers (240 fine + 288 coarse
  points) per metric — the host adds disk. This roughly doubles the JSON per
  entity; the existing Further Notes decimation option applies if profiling on
  a busy box shows the poll too heavy.
- **Docs sync on delivery**: `docs/user-guide/frontend.md` (Monitor tab
  interactions), `docs/developer-guide/tech-stack.md` (chart module),
  `roadmap.md`, `CHANGELOG.md` (user-visible change).
