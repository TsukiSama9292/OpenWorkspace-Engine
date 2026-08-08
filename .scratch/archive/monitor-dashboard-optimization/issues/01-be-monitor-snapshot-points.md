# 01 — Backend: timestamped two-tier series in the monitor snapshot

**Track:** backend

**What to build:** The monitor snapshot endpoint now returns timestamped point
arrays for both granularity tiers, so the frontend can draw an interactive
24-hour time axis that switches resolution by zoom window. The operator-facing
behaviour this enables: a chart that shows fine-grained (15 s) data for the
last hour and coarse-grained (5 min) data for the rest of the day from a
single fetch. Sampling cadence, storage, and the `can_view_monitoring` gate
are unchanged — only the response shape changes.

**Blocked by:** None — can start immediately

**Status:** completed

- [x] Every series field on the monitor snapshot (host + each instance) becomes
      an array of `{ t, v }` points with real wall-clock timestamps, emitted in
      two tiers: fine (15 s) and coarse (5 min). The host carries CPU, memory,
      and disk; each instance carries CPU and memory.
- [x] Current-value and limit fields (CPU %, CPU limit, memory used/limit) are
      preserved exactly as before, so existing value cells keep their source.
- [x] The `can_view_monitoring` gate behaves identically (admin / flagged
      manager 200, unflagged 403, anonymous 401).
- [x] Pure unit tests cover point emission from both tiers for host and
      instances.
- [x] The endpoint integration tests assert the new timestamped two-tier
      shape.
- [x] Full API suite green and zero compiler warnings (both feature sets).
