//! Monitor-dashboard snapshot endpoint (monitor-dashboard spec Decision 5):
//! a single `GET /api/monitor/snapshot` returns the host plus every active
//! instance, each with current values and both granularity tiers — the
//! 15 s fine series (last hour) and the 5 min coarse series (24 hours) — from
//! the in-memory `MetricsStore`. Gated by the `can_view_monitoring` group flag
//! (admins and Managers hold it by default).

use axum::{
    extract::State,
    http::StatusCode,
    routing::get,
    Json, Router,
};
use std::collections::HashMap;
use uuid::Uuid;

use super::AppState;
use crate::auth::AuthUser;
use crate::db::{UserRepository, WorkspaceInstanceRepository, WorkspaceTemplateRepository};
use crate::metrics::EntitySnapshot;
use crate::openapi::MonitorSnapshotEnvelope;
use crate::proc;

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/api/monitor/snapshot", get(snapshot))
}

#[utoipa::path(
    get,
    path = "/api/monitor/snapshot",
    tag = "admin-gated",
    responses(
        (status = 200, description = "host and per-instance resource series: fine (15 s, last hour) + coarse (5 min, 24 h) tiers", body = MonitorSnapshotEnvelope),
        (status = 401, description = "missing or invalid ow_token"),
        (status = 403, description = "requires can_view_monitoring or admin"),
        (status = 500, description = "internal server error"),
    )
)]
pub(crate) async fn snapshot(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !auth.can_view_monitoring() {
        return Err(StatusCode::FORBIDDEN);
    }

    let snap = state.metrics.snapshot();

    // Resolve display labels for the active instances in one pass.
    let instance_repo = WorkspaceInstanceRepository::new(&state.db);
    let template_repo = WorkspaceTemplateRepository::new(&state.db);
    let user_repo = UserRepository::new(&state.db);

    let active = instance_repo
        .list_active_for_monitoring()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let series_by_id: HashMap<Uuid, EntitySnapshot> = snap.instances.into_iter().collect();

    let mut template_meta: HashMap<Uuid, (String, String, i64, i32)> = HashMap::new();
    let mut owner_names: HashMap<Uuid, String> = HashMap::new();
    for inst in &active {
        if !template_meta.contains_key(&inst.template_id)
            && let Ok(Some(t)) = template_repo.find_by_id(inst.template_id).await
        {
            template_meta.insert(
                inst.template_id,
                (t.name, t.container_runtime, t.memory, t.cores),
            );
        }
        if !owner_names.contains_key(&inst.owner_id)
            && let Ok(Some(u)) = user_repo.find_by_id(inst.owner_id).await
        {
            owner_names.insert(inst.owner_id, u.username);
        }
    }

    let now = chrono::Utc::now();
    let instances: Vec<_> = active
        .iter()
        .map(|inst| {
            let snap = series_by_id.get(&inst.id);
            let (tpl_name, tpl_runtime, tpl_memory, tpl_cores) = template_meta
                .get(&inst.template_id)
                .map(|(n, r, m, c)| (n.as_str(), r.as_str(), *m, *c))
                .unwrap_or(("", "", 0, 0));
            serde_json::json!({
                "id": inst.id,
                "name": inst.name,
                "owner": owner_names.get(&inst.owner_id).cloned().unwrap_or_default(),
                "template": tpl_name,
                "runtime": tpl_runtime,
                "status": inst.status,
                "uptime_secs": inst.started_at.map(|t| (now - t).num_seconds().max(0)),
                "cpu_percent": snap.map(|s| s.cpu_percent).unwrap_or(0.0),
                // The CPU ceiling is the template's core count, expressed in
                // the same per-core-% unit Docker reports (200% = 2 cores);
                // 0 means "unlimited" (the container can use the whole host).
                "cpu_limit_percent": if tpl_cores > 0 { tpl_cores as f64 * 100.0 } else { 0.0 },
                "mem_used_bytes": snap.map(|s| s.mem_used_bytes).unwrap_or(0),
                // The rendered memory limit is the template's configured cap in
                // bytes; 0 means "unlimited" (the container's cgroup reports the
                // host RAM as its limit, which would be misleading as a "max").
                "mem_limit_bytes": if tpl_memory > 0 { tpl_memory as u64 } else { 0 },
                "cpu_fine": snap.map(|s| s.cpu_fine.clone()).unwrap_or_default(),
                "cpu_coarse": snap.map(|s| s.cpu_coarse.clone()).unwrap_or_default(),
                "mem_fine": snap.map(|s| s.mem_fine.clone()).unwrap_or_default(),
                "mem_coarse": snap.map(|s| s.mem_coarse.clone()).unwrap_or_default(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "host": {
            "cpu_cores": proc::host_cpu_count(),
            "cpu_percent": snap.host.cpu_percent,
            "mem_used_bytes": snap.host.mem_used_bytes,
            "mem_total_bytes": snap.host.mem_total_bytes,
            "disk_used_bytes": snap.host.disk_used_bytes,
            "disk_total_bytes": snap.host.disk_total_bytes,
            "cpu_fine": snap.host.cpu_fine,
            "cpu_coarse": snap.host.cpu_coarse,
            "mem_fine": snap.host.mem_fine,
            "mem_coarse": snap.host.mem_coarse,
            "disk_fine": snap.host.disk_fine,
            "disk_coarse": snap.host.disk_coarse,
        },
        "instances": instances,
    })))
}
