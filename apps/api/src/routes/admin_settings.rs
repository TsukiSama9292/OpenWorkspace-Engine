use axum::{
    extract::State,
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use super::AppState;
use crate::auth::AuthUser;
use crate::system_settings::{SystemSettings, SystemSettingsRepository};

pub fn routes() -> Router<AppState> {
    Router::new().route(
        "/api/admin/settings",
        get(get_settings).put(update_settings),
    )
}

#[derive(Deserialize)]
pub struct UpdateSettingsRequest {
    max_cpu_cores: i32,
    max_ram_bytes: i64,
    host_instance_limit: i32,
    shared_max_cpu: i32,
    shared_max_ram: i64,
}

impl UpdateSettingsRequest {
    /// Every knob must be a non-negative integer (`0` carries its documented
    /// meaning: unlimited instance count, shared fuse off).
    fn validate(&self) -> Result<(), StatusCode> {
        let non_negative = [
            self.max_cpu_cores >= 0,
            self.max_ram_bytes >= 0,
            self.host_instance_limit >= 0,
            self.shared_max_cpu >= 0,
            self.shared_max_ram >= 0,
        ];
        if non_negative.into_iter().all(|ok| ok) {
            Ok(())
        } else {
            Err(StatusCode::BAD_REQUEST)
        }
    }
}

async fn get_settings(
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

    let settings = SystemSettings {
        max_cpu_cores: input.max_cpu_cores,
        max_ram_bytes: input.max_ram_bytes,
        host_instance_limit: input.host_instance_limit,
        shared_max_cpu: input.shared_max_cpu,
        shared_max_ram: input.shared_max_ram,
    };

    let updated = SystemSettingsRepository::new(&state.db)
        .upsert(&settings)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "settings": updated })))
}
