mod auth;
mod proxy;
mod users;
mod workspace;

use axum::Router;
use sea_orm::DatabaseConnection;

use crate::core::Settings;
use crate::vnc_cache::VncCache;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub vnc_cache: VncCache,
    pub settings: Settings,
}

pub fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/health", axum::routing::get(|| async { axum::Json(serde_json::json!({ "status": "ok" })) }))
        .merge(auth::routes())
        .merge(users::routes())
        .merge(workspace::routes())
        .merge(proxy::routes())
}
