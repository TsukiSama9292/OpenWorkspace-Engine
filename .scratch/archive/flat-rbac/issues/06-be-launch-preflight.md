# 06 — Launch pre-flight: three checks with a single user-row lock

**Track:** backend

**What to build:** The launch and restart paths run the pure `pre_flight` decision from the policy module. The template-whitelist check rejects with `403`; the per-user ceiling check runs inside a transaction that takes a `SELECT … FOR UPDATE` on the single user row (exact, no deadlock cycle) and rejects with `409`; the global ceiling check reads a best-effort count of active instances and rejects with `409`. The old five-step quota pipeline, `allocation_mode` enforcement, host-capacity and shared-fuse reads, and the quota-override fields on the user-update API are removed from the live path.

**Blocked by:** 04-be-effective-context

**Status:** resolved

- [x] Whitelist and both ceiling checks are enforced on launch and restart, with structured rejection bodies and no DB row left behind on rejection
- [x] Concurrent launches from the same user at the ceiling: exactly one succeeds
- [x] Concurrent launches from different users never deadlock and cross-user locks are absent
- [x] The legacy quota pipeline, `allocation_mode` enforcement, and host-capacity/shared-fuse reads are gone; user update no longer accepts quota overrides

## Answer

Implemented. The launch/restart/start path now runs the pure `pre_flight` decision
(`effective_context.rs`) through the new transactional helper `src/activation.rs`:
best-effort global count + `host_instance_limit` read before the tx; a single
`SELECT … FOR UPDATE` on the owner's user row inside the tx; per-user count; the
three checks (whitelist → 403, per-user ceiling → 409, host ceiling → 409); then
the reservation (insert or restart flip) commits in the same tx, so a rejection
leaves no DB row. `src/quota.rs`, `src/quota_activation.rs`, the `AllocationMode`
enforcement, and the shared-fuse reads are deleted; `PUT /api/users/{id}` no longer
accepts `instance_limit`/`max_cpu_cores`/`max_ram_bytes` and the user JSON drops the
effective-quota fields. Rejection bodies use the pinned
`{ error, rejection: { scope, current, limit, requested } }` shape. Route-level
tests cover 403/409, no-row-on-reject, host-ceiling, owner-accounted restarts, and
two concurrency cases (same-user exact-one-wins; different-users both succeed
exercising the port-conflict retry). Full suite green, zero warnings both feature
configs.
