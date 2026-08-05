use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use bcrypt::verify;
use serde::Deserialize;

use super::super::AppState;
use crate::auth::{create_token, set_cookie};
use crate::db::{PolicyRepository, UserRepository};

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/api/auth/login", post(login))
}

async fn login(
    State(state): State<AppState>,
    Json(input): Json<LoginRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let repo = UserRepository::new(&state.db);

    let user = repo
        .find_by_username(&input.username)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let valid = verify(&input.password, &user.password_hash).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !valid {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let context = PolicyRepository::new(&state.db)
        .load_effective_context(user.id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token = create_token(&user.id, &state.settings.jwt_secret)?;

    let mut headers = axum::http::HeaderMap::new();
    set_cookie(&mut headers, &token);

    Ok((
        headers,
        Json(serde_json::json!({ "context": context })),
    ))
}
