use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use super::super::AppState;
use crate::auth::AuthUser;
use crate::db::RegistryRepository;
use crate::openapi::RegistryUrlEnvelope;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/registry", get(get_registry))
        .route("/api/registry/sync", post(sync_registry))
        .route("/api/registry/url", get(get_registry_url).put(set_registry_url))
}

#[derive(Deserialize)]
struct SetRegistryUrlRequest {
    url: String,
}

#[utoipa::path(
    get,
    path = "/api/registry",
    tag = "admin-gated",
    responses(
        (status = 200, description = "cached registry payload", body = serde_json::Value),
        (status = 401, description = "missing or invalid ow_token"),
        (status = 403, description = "requires can_manage_registry or admin"),
        (status = 404, description = "no registry payload cached yet"),
        (status = 500, description = "internal server error"),
    )
)]
pub(crate) async fn get_registry(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !auth.can_manage_registry() {
        return Err(StatusCode::FORBIDDEN);
    }

    let repo = RegistryRepository::new(&state.db);

    let cached = repo
        .get_cached()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match cached {
        Some(json) => Ok(Json(json)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn sync_registry(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !auth.can_manage_registry() {
        return Err(StatusCode::FORBIDDEN);
    }

    let repo = RegistryRepository::new(&state.db);

    let url = repo
        .get_url()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::BAD_REQUEST)?;

    let body = reqwest::get(&url)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch registry from '{}': {}", url, e);
            StatusCode::BAD_GATEWAY
        })?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| {
            tracing::error!("Failed to parse registry JSON from '{}': {}", url, e);
            StatusCode::BAD_GATEWAY
        })?;

    repo.set_cached(&body)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!("Registry synced from '{}'", url);

    Ok(Json(body))
}

#[utoipa::path(
    get,
    path = "/api/registry/url",
    tag = "admin-gated",
    responses(
        (status = 200, description = "configured registry url", body = RegistryUrlEnvelope),
        (status = 401, description = "missing or invalid ow_token"),
        (status = 403, description = "requires can_manage_registry or admin"),
        (status = 500, description = "internal server error"),
    )
)]
pub(crate) async fn get_registry_url(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !auth.can_manage_registry() {
        return Err(StatusCode::FORBIDDEN);
    }

    let repo = RegistryRepository::new(&state.db);

    let url = repo
        .get_url()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "url": url })))
}

async fn set_registry_url(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<SetRegistryUrlRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !auth.can_manage_registry() {
        return Err(StatusCode::FORBIDDEN);
    }

    let repo = RegistryRepository::new(&state.db);

    repo.set_url(&input.url)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!("Registry URL set to '{}'", input.url);

    Ok(Json(serde_json::json!({ "url": input.url })))
}
