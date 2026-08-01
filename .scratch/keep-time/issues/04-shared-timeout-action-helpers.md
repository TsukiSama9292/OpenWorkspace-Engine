# 04 — Extract shared timeout-action helpers (prefactor)

**What to build:** the auto-sleep removal sequence (`remove`/`stop`/`pause`) becomes reusable shared helpers so the Keep-Time worker can dispatch the same actions without duplication. Pure refactor — no behavior change, no new user-visible behavior.

**Blocked by:** None — can start immediately

**Status:** ready-for-agent

- [x] `auto_sleep_remove`/`auto_sleep_stop`/`auto_sleep_pause` extracted into shared helpers (e.g. a small timeout-action module) with identical cleanup sequences
- [x] `check_auto_sleep` uses the shared helpers; behavior unchanged
- [x] Existing auto-sleep worker tests stay green; zero warnings under both feature gates

## Notes

New module: `apps/api/src/timeout_action.rs` (registered `pub mod timeout_action;` in `apps/api/src/lib.rs`).

Three helper signatures (all `Result<(), String>`, each gains a `label: &str` param used only to prefix the existing success `tracing::info!` messages — e.g. `"Auto-sleep removed instance '...'"` → `"{label} removed instance '...'"`):

```rust
pub async fn remove(
    instance_repo: &WorkspaceInstanceRepository<'_>,
    docker: &dyn DockerService,
    vnc_cache: &VncCache,
    instance: &crate::db::WorkspaceInstance,
    label: &str,
) -> Result<(), String>

pub async fn stop(
    instance_repo: &WorkspaceInstanceRepository<'_>,
    docker: &dyn DockerService,
    vnc_cache: &VncCache,
    instance: &crate::db::WorkspaceInstance,
    label: &str,
) -> Result<(), String>

pub async fn pause(
    instance_repo: &WorkspaceInstanceRepository<'_>,
    docker: &dyn DockerService,
    instance: &crate::db::WorkspaceInstance,
    label: &str,
) -> Result<(), String>
```

Cleanup sequences are byte-for-byte the original helpers: `remove` deletes the Traefik route (error logged), removes the VNC cache entry, stops+removes the container (if any), then `instance_repo.delete`; `stop` stops the container (warn-and-continue on Err), deletes route, removes cache entry, sets status "stopped" and clears started_at; `pause` keeps the route/cache untouched, pauses the container (warn-and-continue on Err), sets status "paused" and clears started_at.

`check_auto_sleep` in `health_worker.rs` now dispatches `template.timeout_action` to `crate::timeout_action::remove/stop/pause`, passing `"Auto-sleep"` as the label. Public signature, `Result<usize, String>` return, and scan/filter logic unchanged; the three private `auto_sleep_*` helpers were deleted.

Behavior unchanged: `cargo check --lib` and `cargo check --lib --features docker` both compile clean (zero warnings); existing auto-sleep tests still exercise the same paths through `check_auto_sleep`.
