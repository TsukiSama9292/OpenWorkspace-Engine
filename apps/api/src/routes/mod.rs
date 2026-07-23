mod auth;
mod configs;
mod docker_raw;
mod instances;
mod registry;
mod users;
mod vnc;

use axum::Router;
use sqlx::PgPool;

use crate::vnc_cache::VncCache;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub vnc_cache: VncCache,
}

pub fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/health", axum::routing::get(|| async { axum::Json(serde_json::json!({ "status": "ok" })) }))
        .merge(auth::routes())
        .merge(users::routes())
        .merge(configs::routes())
        .merge(instances::routes())
        .merge(registry::routes())
        .merge(docker_raw::routes())
        .merge(vnc::routes())
}
