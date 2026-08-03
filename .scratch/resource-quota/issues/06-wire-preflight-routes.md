# 06 — Wire pre-flight into launch & start (fail-fast, status transitions, 409 body)

**What to build:** the quota pipeline actually gates instance activation. Both activation paths — launching a new instance and restarting a stopped one — call the transactional activation helper before anything else; a `QuotaViolation` returns `409 Conflict` with the structured `quota` body and leaves **no** database record behind (fail-fast). After a successful reservation the existing Docker build/start flow runs: an infrastructure failure after reservation marks a new launch `error` (record kept) and rolls a restart back to `stopped`. The persistent-instance uniqueness check moves inside the transaction so it is serialized with the quota check by the user-row lock. From a user's perspective: after this ticket, launches and restarts are actually refused with a clear reason when any limit would be exceeded.

**Blocked by:** 05 — Quota DB queries + transactional activation helper.

**Status:** ready-for-agent

- [ ] `launch` runs the full pipeline and, on rejection, returns `409` with the `quota` body and creates no instance row.
- [ ] `start` runs the full pipeline (a restart re-consumes released quota) and, on rejection, returns `409` and leaves the instance `stopped`.
- [ ] `unpause` performs no quota check (paused instances already hold their reservation).
- [ ] On quota-pass, the instance is reserved as `starting` before the Docker call; an infra failure marks a new launch `error` and returns a restart to `stopped`.
- [ ] The persistent-instance uniqueness check runs inside the activation transaction.
- [ ] Route-level tests with a mocked Docker service verify: rejection shape and no DB row, error/stopped transitions, and the existing one-click create-and-start experience is unchanged on success.
