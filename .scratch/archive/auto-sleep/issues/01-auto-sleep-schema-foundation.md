# 01 — Auto-Sleep schema foundation

**What to build:** the DB schema and repositories support the feature: Templates have a nullable usage duration (`max_run_seconds`, NULL = disabled) and a timeout action (default `remove`); Instances have a nullable `started_at` recording when the current running session began. The migration runs cleanly, the entities/structs compile with zero warnings, and the repository methods later tickets need are in place.

**Blocked by:** None — can start immediately

**Status:** completed

- [ ] Migration applies and rolls back cleanly; existing rows backfilled (Templates: NULL duration, `'remove'` action; Instances: NULL `started_at`)
- [ ] Entity/struct changes compile with zero warnings under both default and `docker` features
- [ ] Template repository `create`/`update` accept `max_run_seconds` (nullable) and `timeout_action` and persist them
- [ ] Instance repository exposes `update_started_at` (set/clear) and a query for running Instances with `started_at` set
