use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use super::super::AppState;
use crate::auth::{clear_cookie, AuthUser};
use crate::db::UserRepository;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/auth/validate", get(validate))
        .route("/api/auth/me", get(me))
        .route("/api/auth/logout", post(logout))
        .route("/api/auth/change-password", post(change_password))
}

async fn validate(auth: AuthUser) -> impl IntoResponse {
    Json(serde_json::json!({
        "user_id": auth.user_id,
        "username": auth.username,
        "is_admin": auth.context.is_admin,
        "tier": auth.context.tier,
    }))
}

async fn me(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // The extractor already resolved the context from the DB on this request;
    // re-resolve it so the response is exactly current, even if the row
    // changed between extraction and serialization.
    let context = crate::db::PolicyRepository::new(&state.db)
        .load_effective_context(auth.user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    Ok(Json(serde_json::json!({ "context": context })))
}

async fn logout() -> impl IntoResponse {
    let mut headers = axum::http::HeaderMap::new();
    clear_cookie(&mut headers);
    (headers, Json(serde_json::json!({ "status": "ok" })))
}

#[derive(Deserialize)]
struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
}

async fn change_password(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<ChangePasswordRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if input.new_password.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "New password is required" })),
        ));
    }

    let repo = UserRepository::new(&state.db);

    let user = repo
        .find_by_id(auth.user_id)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal server error" })),
            )
        })?
        .ok_or((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Not authenticated" })),
        ))?;

    let valid = bcrypt::verify(&input.current_password, &user.password_hash).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Internal server error" })),
        )
    })?;
    if !valid {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "Current password is incorrect" })),
        ));
    }

    let password_hash = bcrypt::hash(&input.new_password, 10).map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": "Internal server error" })),
        )
    })?;

    repo.update(auth.user_id, None, Some(&password_hash))
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Internal server error" })),
            )
        })?;

    Ok(Json(serde_json::json!({ "status": "ok" })))
}
