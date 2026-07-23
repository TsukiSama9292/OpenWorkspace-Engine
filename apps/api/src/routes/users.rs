use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use uuid::Uuid;

use super::AppState;
use crate::auth::AuthUser;
use crate::db::UserRepository;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/users", get(list_users))
        .route("/api/users/{id}", get(get_user).delete(delete_user))
}

async fn list_users(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let repo = UserRepository::new(&state.db);

    let users = repo
        .list_all()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let users_json: Vec<_> = users
        .into_iter()
        .map(|u| {
            serde_json::json!({
                "id": u.0, "username": u.1, "role": u.2, "created_at": u.3
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "users": users_json })))
}

async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let repo = UserRepository::new(&state.db);

    let user = repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(serde_json::json!({
        "user": { "id": user.0, "username": user.1, "role": user.3, "created_at": user.4 }
    })))
}

async fn delete_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
) -> Result<StatusCode, StatusCode> {
    if auth.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    let repo = UserRepository::new(&state.db);

    let deleted = repo
        .delete(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if deleted {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
