pub(crate) mod admin_settings;
pub(crate) mod auth;
pub(crate) mod groups;
pub(crate) mod monitor;
pub(crate) mod proxy;
pub(crate) mod users;
pub(crate) mod workspace;

use axum::routing::get;
use axum::Router;
use sea_orm::DatabaseConnection;
use std::sync::Arc;

use crate::core::Settings;
use crate::docker::DockerService;
use crate::metrics::MetricsStore;
use crate::openapi::HealthResponse;
use crate::vnc_cache::VncCache;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub docker: Arc<dyn DockerService>,
    pub vnc_cache: VncCache,
    pub settings: Settings,
    pub metrics: Arc<MetricsStore>,
}

/// Liveness probe. Part of the fuzz surface, so it is a named handler with an
/// exported OpenAPI operation (the spec only covers the 17 safe endpoints).
#[utoipa::path(
    get,
    path = "/health",
    tag = "system",
    responses(
        (status = 200, description = "service is healthy", body = HealthResponse),
    )
)]
pub(crate) async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({ "status": "ok" }))
}

pub fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .merge(auth::routes())
        .merge(users::routes())
        .merge(groups::routes())
        .merge(admin_settings::routes())
        .merge(workspace::routes())
        .merge(proxy::routes())
        .merge(monitor::routes())
}
