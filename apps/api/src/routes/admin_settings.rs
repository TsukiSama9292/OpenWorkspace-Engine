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
    host_instance_limit: i32,
}

impl UpdateSettingsRequest {
    /// The knob must be a non-negative integer (`0` carries its documented
    /// meaning: unlimited instance count).
    fn validate(&self) -> Result<(), StatusCode> {
        if self.host_instance_limit >= 0 {
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
        host_instance_limit: input.host_instance_limit,
    };

    let updated = SystemSettingsRepository::new(&state.db)
        .upsert(&settings)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "settings": updated })))
}
