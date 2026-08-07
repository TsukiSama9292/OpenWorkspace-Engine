//! Monitor-dashboard snapshot endpoint (monitor-dashboard spec Decision 5):
//! a single `GET /api/monitor/snapshot` returns the host plus every active
//! instance, each with current values and the requested series from the
//! in-memory `MetricsStore`. Gated by the `can_view_monitoring` group flag
//! (admins and Managers hold it by default).

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use std::collections::HashMap;
use uuid::Uuid;

use super::AppState;
use crate::auth::AuthUser;
use crate::db::{UserRepository, WorkspaceInstanceRepository, WorkspaceTemplateRepository};
use crate::metrics::{EntitySnapshot, Range};
use crate::openapi::MonitorSnapshotEnvelope;

pub(crate) fn routes() -> Router<AppState> {
    Router::new().route("/api/monitor/snapshot", get(snapshot))
}

#[derive(Deserialize)]
pub(crate) struct RangeParam {
    pub range: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/monitor/snapshot",
    tag = "admin-gated",
    params(
        ("range" = String, Query, description = "Resolution: `1h` (15 s series) or `24h` (5 min aggregates); defaults to `1h`."),
    ),
    responses(
        (status = 200, description = "host and per-instance resource series", body = MonitorSnapshotEnvelope),
        (status = 400, description = "invalid range value"),
        (status = 401, description = "missing or invalid ow_token"),
        (status = 403, description = "requires can_view_monitoring or admin"),
        (status = 500, description = "internal server error"),
    )
)]
pub(crate) async fn snapshot(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(params): Query<RangeParam>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !auth.can_view_monitoring() {
        return Err(StatusCode::FORBIDDEN);
    }

    let range = match params.range.as_deref() {
        None | Some("1h") => Range::Hour,
        Some("24h") => Range::Day,
        Some(_) => return Err(StatusCode::BAD_REQUEST),
    };

    let snap = state.metrics.snapshot(range);

    // Resolve display labels for the active instances in one pass.
    let instance_repo = WorkspaceInstanceRepository::new(&state.db);
    let template_repo = WorkspaceTemplateRepository::new(&state.db);
    let user_repo = UserRepository::new(&state.db);

    let active = instance_repo
        .list_active_for_monitoring()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let series_by_id: HashMap<Uuid, EntitySnapshot> = snap.instances.into_iter().collect();

    let mut template_meta: HashMap<Uuid, (String, String)> = HashMap::new();
    let mut owner_names: HashMap<Uuid, String> = HashMap::new();
    for inst in &active {
        if !template_meta.contains_key(&inst.template_id)
            && let Ok(Some(t)) = template_repo.find_by_id(inst.template_id).await
        {
            template_meta.insert(inst.template_id, (t.name, t.container_runtime));
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
            serde_json::json!({
                "id": inst.id,
                "name": inst.name,
                "owner": owner_names.get(&inst.owner_id).cloned().unwrap_or_default(),
                "template": template_meta.get(&inst.template_id).map(|(n, _)| n.as_str()).unwrap_or_default(),
                "runtime": template_meta.get(&inst.template_id).map(|(_, r)| r.as_str()).unwrap_or_default(),
                "status": inst.status,
                "uptime_secs": inst.started_at.map(|t| (now - t).num_seconds().max(0)),
                "cpu_percent": snap.map(|s| s.cpu_percent).unwrap_or(0.0),
                "mem_used_bytes": snap.map(|s| s.mem_used_bytes).unwrap_or(0),
                "mem_limit_bytes": snap.map(|s| s.mem_total_bytes).unwrap_or(0),
                "cpu_series": snap.map(|s| s.cpu_series.clone()).unwrap_or_default(),
                "mem_series": snap.map(|s| s.mem_series.clone()).unwrap_or_default(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "host": {
            "cpu_percent": snap.host.cpu_percent,
            "mem_used_bytes": snap.host.mem_used_bytes,
            "mem_total_bytes": snap.host.mem_total_bytes,
            "disk_used_bytes": snap.host.disk_used_bytes,
            "disk_total_bytes": snap.host.disk_total_bytes,
            "cpu_series": snap.host.cpu_series,
            "mem_series": snap.host.mem_series,
            "disk_series": snap.host.disk_series,
        },
        "instances": instances,
    })))
}
