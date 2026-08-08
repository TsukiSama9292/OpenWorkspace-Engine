# 02 — Frontend: interactive time-series charts, enlarged host cards, instance detail modal

**Track:** frontend

**What to build:** The frontend draws interactive 24-hour charts from the
timestamped snapshot: three enlarged host cards, per-row sparklines with
hover/tooltip/pin, and an instance-detail modal showing CPU + memory as full
interactive charts. The 1h / 24h range toggle is removed.

**Blocked by:** 01 — Backend: timestamped two-tier series in the monitor snapshot

**Status:** completed

- [x] A reusable interactive SVG chart component implements: hover crosshair +
      value readout, click-to-pin, drag highlight with live avg/max/min stats,
      auto-zoom on release, follow / "back to now", zoom clamping (5 min – 24 h),
      1 h fine-data boundary marker.
- [x] Chart math (time↔x mapping, fine/coarse merging by window, nearest-point,
      drag → zoom, zoom clamping, follow state) lives in `apps/web/src/lib/chart/`
      as pure functions, DOM-free and unit-tested.
- [x] The Monitor panel renders three enlarged interactive host charts (CPU /
      RAM / Disk, ~180 px tall, 3-across).
- [x] Each instance row opens an instance-detail modal showing interactive CPU
      memory charts without a new fetch.
- [x] Row sparklines show a light hover tooltip (value + time) and support
      click-to-pin.
- [x] The 1h/24h range toggle is removed; the 5-second poll is unchanged.
- [x] Vitest suite and `svelte-check` green, including new component and pure
      chart math tests.
