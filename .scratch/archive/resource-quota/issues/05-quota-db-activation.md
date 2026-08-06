# 05 — Quota DB queries + transactional activation helper

**What to build:** the data layer and the atomic check-and-reserve step that both activation paths will share. Repository queries count active instances (per user, and globally) and sum active resources (overall, dedicated-only, shared-only) over the Active Set (running / starting / paused), usable inside a transaction. An activation helper wraps the sequence the spec mandates: begin transaction → `select_for_update` the global `system_settings` row → `select_for_update` the user row → gather counters → run the policy `check` → on violation roll back and report the `QuotaViolation` → otherwise reserve by creating/updating the instance row with status `starting` → commit. Lock order is always global-then-user. From a developer's perspective: after this ticket, the atomic reserve exists, is tested against Postgres, and is ready to be called by the launch/start routes.

**Blocked by:** 01 — `system_settings` singleton, 02 — Per-user quotas, 03 — Template `allocation_mode`, 04 — Quota policy module.

**Status:** completed

- [ ] Active-count and resource-sum queries exist (per-user count, global count, overall/dedicated/shared sums) and treat only `running` / `starting` / `paused` as active.
- [ ] The activation helper runs begin → lock global → lock user → check → reserve (`starting`) → commit, and rolls back cleanly leaving no instance row on violation.
- [ ] The helper acquires locks in global-then-user order only (the spec's deadlock rule).
- [ ] The reservation is a new `starting` record for a launch and a status flip back to `starting` for a restart; the returned `QuotaViolation` is propagatable to a `409`.
- [ ] Integration tests against Postgres verify: reservation semantics, rollback-on-violation (no row left), and that two concurrent activations of the same user at the limit do not both reserve.
