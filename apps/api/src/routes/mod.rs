pub(crate) mod admin_settings;
pub(crate) mod audit;
pub(crate) mod auth;
pub(crate) mod groups;
pub(crate) mod monitor;
pub(crate) mod proxy;
pub(crate) mod users;
pub(crate) mod workspace;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::get;
use axum::Router;
use sea_orm::DatabaseConnection;
use std::sync::Arc;

use crate::audit::{AuditEvent, AuditSender};
use crate::core::Settings;
use crate::db::UserRepository;
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
    /// Best-effort, non-blocking audit-event channel (spec Decision 6).
    pub audit: AuditSender,
}

/// Liveness probe. Part of the fuzz surface, so it is a named handler with an
/// exported OpenAPI operation.
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

/// Thin audit middleware (observability-logs spec Decision 3): after the inner
/// handler runs, a 403 response from an **authenticated** actor records an
/// `auth.forbidden` failure event. Anonymous 401/403 scanner noise is never
/// audited — the actor is resolved only when the response is already 403, and
/// only a valid `ow_token` cookie backed by a live user row records anything.
pub(crate) async fn audit_forbidden_middleware(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    // Snapshot the headers before the request is consumed by `next`.
    let headers = request.headers().clone();
    let maybe_user_id =
        crate::auth::user_id_from_cookie(&headers, &state.settings.jwt_secret);
    let response = next.run(request).await;

    if response.status() == axum::http::StatusCode::FORBIDDEN
        && let Some(user_id) = maybe_user_id
    {
        // Resolve the actor name (one cheap query, only on the 403 path).
        if let Ok(Some(user)) = UserRepository::new(&state.db).find_by_id(user_id).await {
            let client_ip = crate::audit::client_ip(&headers);
            state
                .audit
                .emit(AuditEvent::forbidden(user_id, user.username, client_ip));
        }
    }

    response
}

/// Attach the `auth.forbidden` audit middleware. Applied at the top level
/// (main.rs and the test harness) because `from_fn_with_state` needs a concrete
/// state value, which only exists once `AppState` is constructed.
pub fn with_audit_middleware<S>(
    router: Router<S>,
    state: &AppState,
) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    router.layer(axum::middleware::from_fn_with_state(
        state.clone(),
        audit_forbidden_middleware,
    ))
}

pub fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .merge(auth::routes())
        .merge(users::routes())
        .merge(groups::routes())
        .merge(admin_settings::routes())
        .merge(audit::routes())
        .merge(workspace::routes())
        .merge(proxy::routes())
        .merge(monitor::routes())
}
