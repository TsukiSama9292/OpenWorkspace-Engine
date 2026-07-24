use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};

use super::super::AppState;
use crate::auth::{clear_cookie, AuthUser};
use crate::db::UserRepository;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/auth/validate", get(validate))
        .route("/api/auth/me", get(me))
        .route("/api/auth/logout", post(logout))
}

async fn validate(AuthUser { user_id, role }: AuthUser) -> impl IntoResponse {
    Json(serde_json::json!({ "user_id": user_id, "role": role }))
}

async fn me(
    State(state): State<AppState>,
    AuthUser { user_id, .. }: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let repo = UserRepository::new(&state.db);

    let user = repo
        .find_by_id(user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(serde_json::json!({
        "user": { "id": user.0, "username": user.1, "role": user.3, "created_at": user.4 }
    })))
}

async fn logout() -> impl IntoResponse {
    let mut headers = axum::http::HeaderMap::new();
    clear_cookie(&mut headers);
    (headers, Json(serde_json::json!({ "status": "ok" })))
}
