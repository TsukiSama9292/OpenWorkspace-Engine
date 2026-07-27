use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use super::AppState;
use crate::auth::{AuthUser, Role};
use crate::db::UserRepository;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/users", get(list_users).post(create_user))
        .route(
            "/api/users/{id}",
            get(get_user).put(update_user).delete(delete_user),
        )
}

#[derive(Deserialize)]
struct CreateUserRequest {
    username: String,
    password: String,
    role: Option<String>,
}

#[derive(Deserialize)]
struct UpdateUserRequest {
    username: Option<String>,
    password: Option<String>,
    role: Option<String>,
}

async fn list_users(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !auth.can_manage_users() {
        return Err(StatusCode::FORBIDDEN);
    }

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
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let repo = UserRepository::new(&state.db);

    let user = repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if auth.is_admin() || auth.user_id == id {
        Ok(Json(serde_json::json!({
            "user": { "id": user.0, "username": user.1, "role": user.3, "created_at": user.4 }
        })))
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

async fn delete_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
) -> Result<StatusCode, StatusCode> {
    if !auth.can_manage_users() {
        return Err(StatusCode::FORBIDDEN);
    }

    let repo = UserRepository::new(&state.db);

    let user = repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let target_role = Role::from_str(&user.3).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    if target_role == Role::Admin {
        return Err(StatusCode::FORBIDDEN);
    }

    if !auth.can_create_role(&target_role) {
        return Err(StatusCode::FORBIDDEN);
    }

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

async fn create_user(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<CreateUserRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !auth.can_manage_users() {
        return Err(StatusCode::FORBIDDEN);
    }

    if input.username.is_empty() || input.password.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }

    let target_role = match input.role.as_deref() {
        Some(r) => Role::from_str(r).ok_or(StatusCode::BAD_REQUEST)?,
        None => Role::User,
    };

    if !auth.can_create_role(&target_role) {
        return Err(StatusCode::FORBIDDEN);
    }

    let repo = UserRepository::new(&state.db);
    let password_hash = bcrypt::hash(&input.password, 10)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let id = repo
        .create(&input.username, &password_hash, target_role.as_str())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "user": { "id": id, "username": input.username, "role": target_role.as_str() }
    })))
}

async fn update_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
    Json(input): Json<UpdateUserRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let repo = UserRepository::new(&state.db);

    let user = repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let target_role = Role::from_str(&user.3).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    if target_role == Role::Admin && !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    if !auth.is_admin() && auth.user_id == id {
        if input.username.is_some() || input.role.is_some() {
            return Err(StatusCode::FORBIDDEN);
        }
    }

    if !auth.can_manage_users() && auth.user_id != id {
        return Err(StatusCode::FORBIDDEN);
    }

    let new_role = if let Some(ref r) = input.role {
        let parsed = Role::from_str(r).ok_or(StatusCode::BAD_REQUEST)?;
        if !auth.can_create_role(&parsed) {
            return Err(StatusCode::FORBIDDEN);
        }
        Some(parsed)
    } else {
        None
    };

    let password_hash = if let Some(ref p) = input.password {
        if p.is_empty() {
            return Err(StatusCode::BAD_REQUEST);
        }
        Some(
            bcrypt::hash(p, 10).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?,
        )
    } else {
        None
    };

    let updated = repo
        .update(
            id,
            input.username.as_deref(),
            password_hash.as_deref(),
            new_role.as_ref().map(|r| r.as_str()),
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if updated {
        let user = repo
            .find_by_id(id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;

        Ok(Json(serde_json::json!({
            "user": { "id": user.0, "username": user.1, "role": user.3, "created_at": user.4 }
        })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
