Status: completed

# Resource Monitoring Dashboard (Monitor tab)

## Problem Statement

OpenWorkspace Engine shares one Linux box among many developers, governed by
per-instance limits (CPU, RAM, bandwidth) and auto-sleep/keep-time policies.
Today an operator cannot see *what is actually happening* on the box: the
Session desktop shows basic status, but there is no visibility into host CPU /
RAM / disk headroom or per-instance resource consumption. The `#monitor` tab in
the sidebar is an admin-only placeholder ("Not implemented yet").

Without this, "can I launch another instance?" is answered by guessing, a
memory leak goes unnoticed until the OOM killer acts, and a CPU-spinning
instance is invisible until every other desktop slows down. This is roadmap
Stage 5, item "Resource monitoring dashboard".

## Solution

A Monitor tab (admin + manager viewable) rendered as a single scrolling page:

- **Top — three host cards** (CPU / RAM / Disk): current value, and a
  1-hour fine-grained sparkline (native SVG, no chart library).
- **Below — an "Active Instances" table**: running / starting / paused
  instances, one row each, with owner, template, runtime, uptime, and CPU % /
  RAM usage (value + sparkline). Paused rows are greyed with a `[paused]`
  badge. Stopped and errored instances are not listed.
- **Range toggle** (1h / 24h): 1h shows the fine-grained tier; 24h shows the
  coarse-grained all-day tier, letting an operator spot slow memory-growth or
  a sustained CPU load over a full day.

The data is produced by a **background sampler**: the existing 3-second
`health_worker` tick keeps its current duties, and every **5th tick (15 s)** it
samples host metrics (`/proc`) and per-instance metrics (one-shot `docker
stats` per active instance) into an **in-memory two-tier ring buffer**
(`MetricsStore`). Tier 1 keeps 240 samples at 15 s granularity (1 hour); Tier 2
keeps 288 five-minute aggregates (24 hours), computed from Tier 1. Nothing is
persisted to the database, and no chart library is added to the web bundle.

Access is gated by a **new group permission flag `can_view_monitoring`**:
Admin and Manager system groups default to enabled (Admin pinned all-on per the
existing rule; Manager defaulted on), User stays off, and custom groups default
off until an admin enables the flag. The flag replaces the current
`is_admin`-only gating of the tab and gates the snapshot API.

## User Stories

### Host overview
1. As an **operator**, I want to see the host's current CPU usage as a big card, so that I can judge headroom before launching instances.
2. As an **operator**, I want to see the host's current RAM usage (used / total) as a big card, so that I can spot a machine about to exhaust memory.
3. As an **operator**, I want to see the host's current disk usage (used / total) as a big card, so that I can see at a glance when the disk is filling up.
4. As an **operator**, I want each host card to carry a 1-hour sparkline of its history, so that I can tell a transient spike from a sustained climb.
5. As an **operator**, I want the host cards to update live (5-second poll, data refreshed every 15 seconds), so that the page reflects reality while open.

### Instance table
6. As an **operator**, I want a table of active instances (running / starting / paused) below the host cards, so that I can attribute host load to specific users and workloads.
7. As an **operator**, I want each instance row to show the owner and instance name, so that I can contact or govern the right tenant.
8. As an **operator**, I want each instance row to show the template name, so that I know which workload profile a row corresponds to.
9. As an **operator**, I want each instance row to show the runtime (`gVisor` / `runC`) as a badge, so that I can tell sandboxed from native workloads at a glance.
10. As an **operator**, I want each instance row to show uptime, so that I can spot long-running instances that may need keep-time attention.
11. As an **operator**, I want each instance row to show CPU % (current value + sparkline), so that I can immediately pick out the instance spiking the host CPU.
12. As an **operator**, I want each instance row to show RAM usage (used / limit, current value + sparkline), so that I can find a memory leak by its slope.
13. As an **operator**, I want paused (auto-slept) instances shown greyed-out with a `[paused]` badge, so that the RAM they still hold is not a blind spot.
14. As an **operator**, I want starting instances listed (with a short history), so that a freshly launched workload is visible immediately.
15. As an **operator**, I want stopped / errored instances excluded from the table, so that dead containers do not clutter the view.
16. As an **operator**, I want to sort the table by a column (e.g. RAM or CPU), so that the worst offender is found without scanning.

### Trends
17. As an **operator**, I want a 1h / 24h range toggle, so that I can switch between fine-grained (15 s) recent detail and all-day coarse-grained trends.
18. As an **operator**, I want the 24h range to be derived from five-minute averages with the five-minute peak retained, so that both the typical load and the worst spike survive downsampling.
19. As an **operator**, I want sparklines drawn with native SVG (no chart library), so that the page stays as light as the rest of the SPA.

### Permissions
20. As a **manager**, I want to see the Monitor tab (host + all instances) without being an admin, so that I can operate the shared box with my existing manager role.
21. As an **admin**, I want to grant or revoke `can_view_monitoring` per group, so that I can decide which roles can see host-wide resource usage.
22. As an **admin**, I want the Monitor tab to stay hidden from users without the flag, so that cross-tenant resource usage is not leaked.
23. As an **admin**, I want the Admin system group to always carry the flag (pinned all-on), so that admins can never lock themselves out of monitoring.

### Sampling & data
24. As a **platform developer**, I want the sampler to reuse the existing health-worker tick (one stats pass every 15 s, i.e. every 5th tick), so that no new process or scheduler is introduced.
25. As a **platform developer**, I want instance metrics fetched through a new `DockerService::container_stats()` method, so that the sampler is testable with the existing mock seam and works under both runC and gVisor runtimes.
26. As a **platform developer**, I want host metrics read from `/proc` (stat / meminfo / mounts), so that no Docker daemon traffic is spent on host data.
27. As a **platform developer**, I want a failed stats read to log and skip (fail-open), so that one dead container cannot stop the sampler.
28. As a **platform developer**, I want everything stored in-memory only, so that the <35 MB API RAM budget is respected (≈500 samples per instance ≈ 20 KB; ~2 MB for a 100-instance box).
29. As a **platform developer**, I want a 24h history without database writes, so that the DB and disk are never burdened by monitoring data.

## Implementation Decisions

### 1. Two-tier in-memory `MetricsStore` (new pure module)
- Holds host tiers and per-instance tiers, keyed by instance id. Pure logic,
  no I/O; unit-tested without Docker.
- **Tier 1** (fine): fixed-capacity `VecDeque`, 240 samples at 15 s = 60 min.
  One sample pushed per stats pass per entity (host + each active instance).
  Oldest evicted past capacity.
- **Tier 2** (coarse): fixed-capacity `VecDeque`, 288 samples at 5 min = 24 h.
  Every 5 minutes, the last 20 Tier-1 samples are folded into one aggregate
  sample (timestamp = window end, plus **mean** and **peak** of CPU % and of
  RAM usage) which is appended to Tier 2. Tier 1 is kept intact.
- A sample carries: timestamp, cpu_percent, mem_used_bytes; the mem limit is
  per-entity metadata (stored once per instance, not per sample).
- Aggregation is a pure function: `aggregate_window(samples) -> sample`
  computing mean + peak; unit-tested for empty, partial, and full windows.
- Snapshot read is pure: given the store and a range (`1h` | `24h`), return
  the host series + per-instance series from the matching tier.

### 2. Sampling cadence (piggyback on the health worker)
- The `health_worker` 3-second tick keeps its current duties unchanged. A
  per-worker tick counter triggers the stats pass on every 5th tick (15 s).
- The stats pass: read host `/proc` metrics; list active instances
  (running / starting / paused); for each, call `container_stats()`; push all
  samples into `MetricsStore`; run the Tier-2 aggregation step if the 5-minute
  boundary is crossed.
- The sampler is an injectable function taking `&dyn DockerService` and the
  `MetricsStore`, so the whole pass is exercised against the mock in tests.

### 3. `DockerService::container_stats()` (the one new seam)
- New trait method: `async fn container_stats(&self, container_id: &str) ->
  Result<ContainerStats, String>` returning cpu_percent, mem_used_bytes,
  mem_limit_bytes. `ContainerStats` is a plain data struct.
- Real `DockerClient` impl: one-shot bollard `stats` (stream = false). CPU %
  requires a **delta between consecutive reads** (cumulative CPU vs system
  time, exactly how `docker stats` computes it); the client caches the
  previous (cpu_total, system_total) per container and returns no cpu_percent
  for the first read of a container (null until a second sample exists). The
  cache is keyed by container id so restarts do not reuse stale deltas.
- Fail-open: a stats error is logged and skipped for that instance; the row
  keeps its previous values (or shows a dash) rather than failing the pass.
- Mock seam: the trait gains the method; all existing mock impls return canned
  `ContainerStats`. gVisor (runsc) containers report through the daemon the
  same as runC, so no runtime branch exists in this code path.

### 4. Host `/proc` sampling (pure parsers)
- CPU: parse `/proc/stat` aggregate busy percentage from deltas between reads
  (previous counters cached in the sampler, `/proc/stat` is host-global and
  visible from the API container).
- RAM: parse `/proc/meminfo` (MemTotal / MemAvailable) → used = total −
  available.
- Disk: statfs on the host root filesystem (or the volume backing instance
  data) → used / total.
- Each parse step is a pure function (string/stat input → value), unit-tested
  with fixture strings, mirroring the `/proc` parsing approach already used in
  the benchmark tooling.

### 5. Snapshot API contract
- New endpoint `GET /api/monitor/snapshot?range=1h|24h` (default `1h`), under
  the existing authenticated-route family; the single source of truth for the
  response shape remains the generated OpenAPI spec.
- Response: host block (cpu %, mem used/total, disk used/total, plus the
  series for the requested range) and an instances array (instance id, name,
  owner, template, runtime, status, uptime seconds, plus the series for the
  requested range). Series points are timestamp + cpu % + mem bytes; both the
  CPU and the RAM sparkline derive from the same series.
- Gating: `AuthUser::can_view_monitoring()` (admin or flag) → 403 otherwise.
  The endpoint is part of the safe fuzz surface.

### 6. RBAC: new group flag `can_view_monitoring`
- Migration `000022`: add `groups.can_view_monitoring BOOLEAN NOT NULL DEFAULT
  FALSE`; backfill TRUE for the Admin and Manager system groups (by `kind`).
  New custom groups default FALSE (column default).
- Effective context: the flag is OR'd across the user's groups exactly like the
  five existing flags, and Admin groups carry it (admin groups carry every
  flag); it joins the effective-context payload so a permission change lands on
  the next request.
- `AuthUser` gains `can_view_monitoring()` mirroring `can_manage_docker()`.
- Group create/update accepts and persists the flag; the group editor UI gains
  a checkbox; Admin/User system-group pinning follows the existing rule (Admin
  on, User off, Manager editable — defaulted on).
- Frontend: `EffectiveContext` / `Group` types gain the field; a
  `mayViewMonitoring(ctx)` helper is added to the permissions module; the
  Monitor sidebar item and tab swap `$isAdmin` for `mayViewMonitoring`.

### 7. Frontend Monitor panel (new component)
- `MonitorPanel`: host cards (value + SVG sparkline) on top; active-instance
  table below (sortable by column); 1h/24h range toggle; 5-second poll of the
  snapshot endpoint (data changes at the 15 s cadence; polling keeps uptime
  fresh). Rows: owner / instance, template, runtime badge, uptime,
  CPU % (value + sparkline), RAM used/limit (value + sparkline); paused rows
  greyed with a `[paused]` badge.
- A small reusable `Sparkline` component maps a number series to an SVG
  `<path>` (no chart library, no new dependency).

## Testing Decisions

A good test asserts externally observable behavior — the series content, the
aggregation math, the gating decision, the rendered rows — never the internal
storage layout.

- **`MetricsStore` (pure unit tests)**: capacity/eviction of both tiers;
  aggregation mean + peak for empty / partial / full windows; Tier-2 promotion
  cadence; snapshot selection by range; per-instance keying. Mirrors the pure
  unit tests of `host_port.rs` / `network_qos.rs`.
- **`/proc` parsers (pure unit tests)**: CPU % from two `/proc/stat` fixtures,
  meminfo used/total, statfs used/total; malformed input handling. Mirrors
  `benchlib.sh`'s parse functions (fixture-driven, no host).
- **Sampler (mock `DockerService`)**: with a mock returning canned stats, a
  stats pass populates host + instance tiers; a failing `container_stats`
  logs and skips (fail-open); cadence trigger (every 5th tick).
- **Endpoint (integration, mock docker)**: `GET /api/monitor/snapshot` returns
  200 for admin and for a manager whose group carries the flag, 403 for a
  user without it, 401 unauthenticated; response shape per range; series from
  the right tier. Follows the existing `instances_mock_test.rs` / auth
  integration-test patterns.
- **RBAC**: effective-context OR across groups includes the new flag; admin
  carries it; `AuthUser::can_view_monitoring` tests mirror the existing
  `can_manage_docker` tests; migration backfills Admin/Manager on, User off.
- **Web (vitest)**: `Sparkline` renders a valid path from a series; `MonitorPanel`
  renders host cards + table columns, pauses a row on `paused` (greyed +
  badge), toggles range, renders sorted table; `mayViewMonitoring` in the
  permissions/rbac-actions tests; group editor checkbox round-trips the flag.
- **Gates**: `bash scripts/check.sh` silent (both feature sets), web
  `pnpm check`, full `run_tests.sh` (docker feature) and web `pnpm test` green
  before the change is considered done.

## Out of Scope

- Owner-visible per-instance metrics (an instance owner seeing their own
  resource usage) — a separate follow-up, deliberately not part of v1.
- Per-instance disk usage (sampling cost + `docker inspect` traversal are not
  worth the value; host disk card covers the 95% case).
- Persisting metrics to the database, or history beyond 24 hours.
- Alerting / notifications on thresholds.
- GPU (NVProxy) metrics.
- Streaming (WebSocket) updates; per-group/user CPU/RAM quotas (separate
  roadmap item); audit logging (separate roadmap item); the Logs tab.

## Further Notes

- **Payload size**: the snapshot returns full tier series (240 / 288 points)
  per entity. If profiling on a busy box shows the poll payload too heavy,
  decimate the sparkline series server-side (bounded point count, range kept)
  — an implementation-time optimization, testable through the same seam.
- **Memory budget**: 240 + 288 ≈ 528 samples/instance ≈ < 20 KB; a 100-instance
  box ≈ 1.7 MB plus the host tiers — inside the < 35 MB API budget by two
  orders of magnitude.
- **Docs sync on delivery**: `docs/user-guide/rbac.md` (new flag),
  `docs/user-guide/frontend.md` (Monitor tab), `docs/developer-guide/tech-stack.md`
  (sampler + store), `roadmap.md` (Stage 5 item → completed),
  `CHANGELOG.md` (user-visible change).
