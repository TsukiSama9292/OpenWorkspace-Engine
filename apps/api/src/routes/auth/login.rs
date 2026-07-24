use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::post,
    Json, Router,
};
use bcrypt::{hash, verify, DEFAULT_COST};
use serde::Deserialize;

use super::super::AppState;
use crate::auth::{create_token, set_cookie};
use crate::db::UserRepository;

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct RegisterRequest {
    username: String,
    password: String,
    role: Option<String>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/register", post(register))
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

    let token = create_token(&user.0, &user.3, &state.settings.jwt_secret)?;

    let mut headers = axum::http::HeaderMap::new();
    set_cookie(&mut headers, &token);

    Ok((
        headers,
        Json(serde_json::json!({
            "user": { "id": user.0, "username": user.1, "role": user.3 }
        })),
    ))
}

async fn register(
    State(state): State<AppState>,
    Json(input): Json<RegisterRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let repo = UserRepository::new(&state.db);

    let password_hash = hash(&input.password, DEFAULT_COST)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let role = input.role.unwrap_or_else(|| "user".to_string());

    let user_id = repo
        .create(&input.username, &password_hash, &role)
        .await
        .map_err(|_| StatusCode::CONFLICT)?;

    Ok(Json(serde_json::json!({
        "user": { "id": user_id, "username": input.username, "role": role }
    })))
}
