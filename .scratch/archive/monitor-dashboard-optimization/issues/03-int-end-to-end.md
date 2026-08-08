# 03 — Integration: monitor dashboard interactions against the real backend

**Track:** integration

**What to build:** A full-stack verification that the interactive Monitor
dashboard works against the real backend: the snapshot endpoint's timestamped
two-tier data flows into the browser and drives the chart interactions. An
operator logging in with monitoring access sees live, correctly shaped charts
on the Monitor tab, can drag to zoom, and can open an instance's detail modal —
all against a running stack, not mocks.

**Blocked by:** 02 — Frontend: interactive time-series charts, enlarged host cards, instance detail modal

**Status:** completed

- [x] Log in as an admin against a running dev stack and open the Monitor tab.
- [x] The three host charts render with real timestamped data (fine points for
      the recent hour, coarse points beyond), and the value cells match the
      chart current values.
- [x] Dragging across a host chart highlights the range with live stats and
      zooms on release; "back to now" resets the view and re-engages follow.
- [x] Opening an instance's detail modal shows interactive CPU and memory
      charts populated from the same data, with no additional request.
- [x] Assertions target observable UI state (chart points, readouts, zoomed
      window), following the existing E2E conventions; teardown cleans up any
      launched instances.
