# 04 — Quota policy module (pure logic) + unit tests

**What to build:** the pre-flight decision logic itself, as a self-contained pure module with no database or Docker access — the deep module behind the whole feature. It defines the domain types (`AllocationMode`, `Quota`, `HostCapacity`, `QuotaScope`, `QuotaViolation`) and a single `check` function that runs the five-step pipeline in fixed order over plain values: per-user instance count → global host instance count → per-user CPU/RAM → host dedicated pool (dedicated templates only) → host shared fuse (shared templates only, when enabled). It also owns `resolve_effective_quota` (role defaults, Admin personal exemption) and the `QuotaScope` enum that the rejection contract uses. From a developer's perspective: after this ticket, every quota rule in the spec is implemented and unit-tested without any infrastructure.

**Blocked by:** None — can start immediately.

**Status:** ready-for-agent

- [ ] The module exposes `AllocationMode`, `Quota`, `HostCapacity`, `QuotaScope`, `QuotaViolation`, `resolve_effective_quota`, and `check`, taking only plain values (no DB/Docker).
- [ ] `check` runs the pipeline in the spec's fixed order and returns the first violation as `QuotaViolation` carrying `scope`, `current`, `limit`, `requested`.
- [ ] Admin personal-level checks are skipped when the effective quota is exempt; global-level checks (host instance count, dedicated pool, shared fuse) always run.
- [ ] `host_instance_limit = 0` and `shared_max_* = 0` skip their checks; dedicated checks run only for dedicated templates; the shared fuse only for shared templates.
- [ ] Unit tests cover every scope's boundary (at-limit vs over-limit), Admin exemption, zero-disabled skips, mode gating, and pipeline ordering.
