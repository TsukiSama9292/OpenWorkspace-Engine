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
use crate::auth::{create_token, set_cookie, Role};
use crate::db::UserRepository;

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

    let valid = verify(&input.password, &user.2).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !valid {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let role = Role::from_str(&user.3).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let token = create_token(&user.0, &role, &state.settings.jwt_secret)?;

    let mut headers = axum::http::HeaderMap::new();
    set_cookie(&mut headers, &token);

    Ok((
        headers,
        Json(serde_json::json!({
            "user": { "id": user.0, "username": user.1, "role": user.3 }
        })),
    ))
}
