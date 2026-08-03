use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Deserializer};
use uuid::Uuid;

use super::AppState;
use crate::auth::{AuthUser, Role};
use crate::db::{UserRecord, UserRepository};
use crate::quota::{resolve_effective_quota, QuotaOverride};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/users", get(list_users).post(create_user))
        .route(
            "/api/users/{id}",
            get(get_user).put(update_user).delete(delete_user),
        )
}

/// Deserialize a quota override field, distinguishing "key absent" (outer
/// `None` — leave the column untouched) from "key present with `null`"
/// (`Some(None)` — restore the role default) and "key present with a value"
/// (`Some(Some(v))`). Plain `Option<Option<T>>` collapses absent and `null`,
/// so this custom deserializer is required to preserve the NULL round-trip.
fn nullable_or_absent<'de, T, D>(de: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Option::<T>::deserialize(de).map(Some)
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
    #[serde(default, deserialize_with = "nullable_or_absent")]
    instance_limit: Option<Option<i32>>,
    #[serde(default, deserialize_with = "nullable_or_absent")]
    max_cpu_cores: Option<Option<i32>>,
    #[serde(default, deserialize_with = "nullable_or_absent")]
    max_ram_bytes: Option<Option<i64>>,
}

/// Serialize a user row for admin responses. The raw override columns are
/// echoed as-is (NULL = inherit the role default), and the effective quota is
/// computed through `resolve_effective_quota` so the UI never re-implements
/// role defaults.
fn user_to_json(user: &UserRecord) -> serde_json::Value {
    let role = Role::from_str(&user.role).unwrap_or(Role::User);
    let quota = resolve_effective_quota(
        QuotaOverride {
            instance_limit: user.instance_limit,
            max_cpu_cores: user.max_cpu_cores,
            max_ram_bytes: user.max_ram_bytes,
        },
        role.clone(),
    );
    serde_json::json!({
        "id": user.id,
        "username": user.username,
        "role": role.as_str(),
        "created_at": user.created_at,
        "instance_limit": user.instance_limit,
        "max_cpu_cores": user.max_cpu_cores,
        "max_ram_bytes": user.max_ram_bytes,
        "effective_instance_limit": quota.instance_limit,
        "effective_max_cpu_cores": quota.max_cpu_cores,
        "effective_max_ram_bytes": quota.max_ram_bytes,
        "quota_exempt": quota.exempt,
    })
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

    let users_json: Vec<_> = users.into_iter().map(|u| user_to_json(&u)).collect();

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
            "user": user_to_json(&user)
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

    let target_role = Role::from_str(&user.role).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

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

    let user = repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "user": user_to_json(&user)
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

    let target_role = Role::from_str(&user.role).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

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

    // Per-user quota overrides are a high-privilege action: only Admins may
    // set them. Managers can manage users (name/role/password) but not quotas.
    if !auth.is_admin()
        && (input.instance_limit.is_some()
            || input.max_cpu_cores.is_some()
            || input.max_ram_bytes.is_some())
    {
        return Err(StatusCode::FORBIDDEN);
    }

    // Overrides are stored as nullable i32/i64. Negative values make no sense
    // as a quota (a negative limit would reject every request), so reject them.
    if input.instance_limit.flatten().is_some_and(|v| v < 0)
        || input.max_cpu_cores.flatten().is_some_and(|v| v < 0)
        || input.max_ram_bytes.flatten().is_some_and(|v| v < 0)
    {
        return Err(StatusCode::BAD_REQUEST);
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
            input.instance_limit,
            input.max_cpu_cores,
            input.max_ram_bytes,
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
            "user": user_to_json(&user)
        })))
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
