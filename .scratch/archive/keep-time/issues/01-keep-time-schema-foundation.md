# 01 — Keep-Time schema foundation

**What to build:** the DB schema and repositories support the feature: Templates have a nullable idle keep-time (`keep_time_seconds`, NULL = disabled) and a keep-time action (default `pause`); Instances have a nullable `last_seen_at` recording when the screen was last actively viewed. The migration runs cleanly, the entities/structs compile with zero warnings, and the repository methods later tickets need are in place.

**Blocked by:** None — can start immediately

**Status:** completed

- [x] One migration applies and rolls back cleanly; existing rows backfilled (Templates: NULL `keep_time_seconds`, `'pause'` action; Instances: NULL `last_seen_at`)
- [x] Template keep-time action column constrained to `remove`/`stop`/`pause` (DB CHECK)
- [x] Entity/struct changes compile with zero warnings under both default and `docker` features
- [x] Template repository `create`/`update` accept `keep_time_seconds` (nullable) and `keep_time_action` and persist them
- [x] Instance repository exposes `update_last_seen_at` (set/clear)

## Notes

**What changed:**
- New migration `m20260801_000012_add_keep_time` registered as the last entry in `apps/api/migration/src/lib.rs`. `up()` adds `workspace_templates.keep_time_seconds BIGINT` (NULL = disabled), `workspace_templates.keep_time_action VARCHAR(20) NOT NULL DEFAULT 'pause'` with CHECK `('remove','stop','pause')`, and `workspace_instances.last_seen_at TIMESTAMPTZ` (NULL = never actively viewed). `down()` reverses in dependency-safe order (instance column first, then CHECK, then template columns).
- `workspace_template::Model` + public `WorkspaceTemplate`: added `keep_time_seconds: Option<i64>` and `keep_time_action: String` after `network_bandwidth_down_mbps`; mapped in `From`.
- `workspace_instance::Model` + public `WorkspaceInstance`: added `last_seen_at: Option<DateTimeUtc>` after `started_at`; mapped in `From`.
- `WorkspaceTemplateRepository::create`/`update` take two trailing params `keep_time_seconds: Option<i64>, keep_time_action: &str` and persist them via `Set(...)`.
- `WorkspaceInstanceRepository::update_last_seen_at(id, Option<DateTimeUtc>)` added, mirroring `update_started_at` (returns `Ok(false)` on RecordNotFound/RecordNotUpdated).
- All existing callers updated with defaults: `templates.rs` routes, `db_test.rs` (26 create + 3 update calls), `health_worker_test.rs` (2 helper create calls). Added `migrations_round_trip` test (down then up) to `db_test.rs`.
- Verified: `cargo check --lib` (default + `docker`) and `scripts/check.sh` both emit zero warnings/errors; `cargo test --no-run --features docker` compiles all test binaries.

**Deferred to later tickets:** keep-time request/response JSON fields and validation in `templates.rs` (ticket 02); heartbeat endpoint and `last_seen_at` updates (heartbeat/keep-time worker ticket); frontend.
