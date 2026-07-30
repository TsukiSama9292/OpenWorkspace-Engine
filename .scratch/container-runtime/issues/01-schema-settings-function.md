# 01 — Schema, Settings & pure function

**What to build:** The database column, Rust data model fields, environment variable parsing, and the pure mapping function that together make `container_runtime` a real concept the rest of the system can build on. No API or UI changes yet — just infrastructure with tests.

**Blocked by:** None — can start immediately

**Status:** ready-for-agent

- [ ] Create migration `m20260723_000009_add_container_runtime.rs` — `ALTER TABLE workspace_templates ADD COLUMN container_runtime VARCHAR(64) NOT NULL DEFAULT 'docker'`
- [ ] Add `container_runtime: String` to SeaORM entity `workspace_template::Model` in `db.rs`
- [ ] Add `container_runtime: String` to public `WorkspaceTemplate` struct and thread through `From<workspace_template::Model>` impl
- [ ] Add `pub container_runtime: Option<String>` to `ContainerConfig` in `docker.rs`
- [ ] Add `container_runtime: String` (default `"docker"`) to `Settings` in `settings.rs`, parsed from `OW_CONTAINER_RUNTIME` env var following the same pattern as `DOCKER_NETWORK`
- [ ] Implement `fn runtime_to_host_config(value: &str) -> Option<String>` as a pure function in `docker.rs` — maps `"docker"`/`""` → `None`, `"runsc"` → `Some("runsc")`, all other values → `Some(value)`
- [ ] Unit tests for `runtime_to_host_config` covering all three paths
- [ ] Unit tests in `settings.rs` for `OW_CONTAINER_RUNTIME` default (`"docker"`), custom value, and absence
