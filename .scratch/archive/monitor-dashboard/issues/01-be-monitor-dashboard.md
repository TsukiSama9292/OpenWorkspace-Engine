# 01 — Backend: Monitor snapshot API + sampler + RBAC flag

**Track:** backend

**What to build:** The server side of the Monitor tab. A background sampler
reuses the existing 3-second health-worker tick and, every 5th tick (15 s),
samples host metrics from `/proc` and per-instance metrics via a new
`DockerService::container_stats()` method, storing everything in an in-memory
two-tier ring buffer (`MetricsStore`: 240 × 15 s fine-grained samples per
entity ≈ 1 hour, plus 288 five-minute mean+peak aggregates ≈ 24 hours). A new
`GET /api/monitor/snapshot?range=1h|24h` endpoint returns the host and
per-instance series for the requested range, gated by a new group permission
flag `can_view_monitoring` (Admin and Manager system groups default enabled,
User off). Nothing is persisted; the API stays inside the <35 MB RAM budget.

**Blocked by:** None — can start immediately.

**Status:** completed

- [x] New group flag `can_view_monitoring` lands via migration (Admin and
      Manager system groups backfilled on, User off, custom groups default
      off), appears in effective context (OR'd across groups, admin carries
      it), and is exposed through group create/update and the effective-context
      payload.
- [x] `AuthUser::can_view_monitoring()` (admin or flag) is the sole gate for
      the Monitor snapshot endpoint; unauthenticated → 401, no flag → 403.
- [x] `DockerService::container_stats()` (the new seam) returns CPU %,
      mem used, mem limit; the real client computes CPU % from deltas between
      consecutive one-shot stats reads (null on first read), and a stats
      failure is fail-open (logged, instance skipped).
- [x] Host `/proc` sampling (stat / meminfo / mounts) is implemented as pure
      parse functions with fixture-driven unit tests.
- [x] `MetricsStore` two-tier ring buffer: Tier 1 240 samples @ 15 s; Tier 2
      288 five-minute aggregates computed from Tier 1 as mean + peak; capacity
      eviction; snapshot read by `range` — all pure logic with unit tests.
- [x] The sampler runs in the health worker on every 5th tick, samples host +
      active (running / starting / paused) instances, folds Tier-2 aggregates
      on the 5-minute boundary, and is testable against the mock `DockerService`.
- [x] Endpoint integration tests (mock docker): 200 for admin and for a
      flag-holding manager, 403 without the flag, correct response shape per
      `?range=`.
- [x] `bash scripts/check.sh` produces no output (both feature sets) and
      `bash scripts/run_tests.sh` is green before the ticket closes.

**Notes (post-build sync):** Landed as expected — migration `000022`
(`apps/api/migration/src/m20260803_000022_add_can_view_monitoring.rs`)
adds the flag and backfills Admin/Manager on. Effective-context OR is
implemented in `effective_context.rs` (`groups.iter().any(...)`, admin groups
carry it). The sampler lives in `health_worker.rs` (`MetricsSampler`, every 5th
3 s tick), `/proc` parsers in `proc.rs` (10 fixture-driven unit tests),
`MetricsStore` in `metrics.rs` (8 unit tests), the endpoint gate on
`AuthUser::can_view_monitoring` (`auth.rs`). `check.sh` silent on both feature
sets.
