mod admin_settings;
mod auth;
mod groups;
mod proxy;
mod users;
mod workspace;

use axum::Router;
use sea_orm::DatabaseConnection;
use std::sync::Arc;

use crate::core::Settings;
use crate::docker::DockerService;
use crate::vnc_cache::VncCache;

#[derive(Clone)]
pub struct AppState {
    pub db: DatabaseConnection,
    pub docker: Arc<dyn DockerService>,
    pub vnc_cache: VncCache,
    pub settings: Settings,
    /// Serializes instance-network allocation+creation so concurrent launches
    /// in this process can never compute the same free `/30` from the same
    /// `list_networks` snapshot and collide. Production runs a single API
    /// process; cross-process races are still absorbed by the bounded
    /// re-allocate retry in `launch_instance`.
    pub network_lock: Arc<tokio::sync::Mutex<()>>,
    /// Serializes host-port allocation in this process. Concurrent launches must
    /// never hand the same probe-free port to two containers whose bind only
    /// happens at `start` — the loser would sit `created` with a leaked runsc
    /// sandbox. The pool tracks ports reserved for the allocate→create→start→
    /// DB-commit window (see `host_port::PortPool`); production runs a single
    /// API process, so the reservation covers the in-process race, while
    /// cross-process collisions are absorbed by the port-conflict retry.
    pub port_pool: Arc<tokio::sync::Mutex<crate::host_port::PortPool>>,
}

pub fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/health", axum::routing::get(|| async { axum::Json(serde_json::json!({ "status": "ok" })) }))
        .merge(auth::routes())
        .merge(users::routes())
        .merge(groups::routes())
        .merge(admin_settings::routes())
        .merge(workspace::routes())
        .merge(proxy::routes())
}
