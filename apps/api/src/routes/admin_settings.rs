use axum::{
    extract::State,
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use super::AppState;
use crate::audit::{action, diff_detail, target, AuditEvent};
use crate::auth::AuthUser;
use crate::db::WorkspaceInstanceRepository;
use crate::openapi::SettingsEnvelope;
use crate::system_settings::{SystemSettings, SystemSettingsRepository};

pub fn routes() -> Router<AppState> {
    Router::new().route(
        "/api/admin/settings",
        get(get_settings).put(update_settings),
    )
}

#[derive(Deserialize)]
pub struct UpdateSettingsRequest {
    host_instance_limit: i32,
    host_cpu_cores: i32,
    host_memory_mb: i64,
    host_gpu_count: i32,
}

impl UpdateSettingsRequest {
    /// Every knob must be `>= -1` (`-1` carries its documented meaning:
    /// unlimited / disabled; `0` is a real zero).
    fn validate(&self) -> Result<(), StatusCode> {
        if self.host_instance_limit >= -1
            && self.host_cpu_cores >= -1
            && self.host_memory_mb >= -1
            && self.host_gpu_count >= -1
        {
            Ok(())
        } else {
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

#[utoipa::path(
    get,
    path = "/api/admin/settings",
    tag = "admin-gated",
    responses(
        (status = 200, description = "global policy settings", body = SettingsEnvelope),
        (status = 401, description = "missing or invalid ow_token"),
        (status = 403, description = "requires admin"),
        (status = 500, description = "internal server error"),
    )
)]
pub(crate) async fn get_settings(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let settings = SystemSettingsRepository::new(&state.db)
        .get_or_create()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "settings": settings })))
}

async fn update_settings(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<UpdateSettingsRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    input.validate()?;

    // Whole-layer-consumer guard (spec §7, same as the group/member guards in
    // `routes/groups.rs`): lowering a host cap to a *finite* value while an
    // active instance with a `-1` snapshot on that resource exists would let
    // it silently stop counting — reject `409` so the admin stops those
    // instances before imposing limits.
    let unlimited = WorkspaceInstanceRepository::new(&state.db)
        .count_active_unlimited_snapshots(None, None)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let blocks_tightening = (input.host_cpu_cores >= 0 && unlimited.cpu_cores > 0)
        || (input.host_memory_mb >= 0 && unlimited.memory_mb > 0)
        || (input.host_gpu_count >= 0 && unlimited.gpu_count > 0);
    if blocks_tightening {
        return Err(StatusCode::CONFLICT);
    }

    let repo = SystemSettingsRepository::new(&state.db);
    let old = repo
        .get_or_create()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let settings = SystemSettings {
        host_instance_limit: input.host_instance_limit,
        host_cpu_cores: input.host_cpu_cores,
        host_memory_mb: input.host_memory_mb,
        host_gpu_count: input.host_gpu_count,
    };

    let updated = repo
        .upsert(&settings)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut changes = Vec::new();
    if old.host_instance_limit != updated.host_instance_limit {
        changes.push((
            "host_instance_limit".to_string(),
            serde_json::json!(old.host_instance_limit),
            serde_json::json!(updated.host_instance_limit),
        ));
    }
    if old.host_cpu_cores != updated.host_cpu_cores {
        changes.push((
            "host_cpu_cores".to_string(),
            serde_json::json!(old.host_cpu_cores),
            serde_json::json!(updated.host_cpu_cores),
        ));
    }
    if old.host_memory_mb != updated.host_memory_mb {
        changes.push((
            "host_memory_mb".to_string(),
            serde_json::json!(old.host_memory_mb),
            serde_json::json!(updated.host_memory_mb),
        ));
    }
    if old.host_gpu_count != updated.host_gpu_count {
        changes.push((
            "host_gpu_count".to_string(),
            serde_json::json!(old.host_gpu_count),
            serde_json::json!(updated.host_gpu_count),
        ));
    }
    if !changes.is_empty() {
        state.audit.emit(
            AuditEvent::from_auth(&auth, action::SETTINGS_UPDATE, target::SETTINGS)
                .with_detail(diff_detail(&changes)),
        );
    }

    Ok(Json(serde_json::json!({ "settings": updated })))
}
