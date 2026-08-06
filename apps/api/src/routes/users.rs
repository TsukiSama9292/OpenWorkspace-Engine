use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use super::AppState;
use crate::auth::AuthUser;
use crate::db::{
    validate_group_ids, GroupRepository, PolicyRepository, UserRepository, UserWithPolicy,
};
use crate::effective_context::can_assign_groups;

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
    /// Optional initial memberships. When absent/empty the User system group
    /// is assigned (assigning zero groups is out of scope).
    #[serde(default)]
    group_ids: Option<Vec<Uuid>>,
}

/// The user-management payload: identity fields plus the flat-RBAC policy
/// overrides (group memberships, personal instance ceiling, personal template
/// whitelist). Policy fields are optional so an identity-only PUT never wipes
/// them; `direct_max_instances` distinguishes "absent" from an explicit
/// `null` that clears the personal ceiling.
/// Deserialize a `direct_max_instances` payload that may be a number (set), a
/// `null` (explicitly clear the personal ceiling), or absent (leave untouched).
/// Plain `Option<Option<i32>>` cannot express all three states because serde
/// folds JSON `null` into `None` at the outermost `Option` — this visitor is
/// invoked only when the field is present, so the outer `Option` keeps
/// meaning "absent" while `visit_none` distinguishes `null` from a value.
fn deserialize_clearable_ceiling<'de, D>(
    deserializer: D,
) -> Result<Option<Option<i32>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Visitor;

    struct CeilingVisitor;

    impl<'de> Visitor<'de> for CeilingVisitor {
        type Value = Option<Option<i32>>;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("an integer, `null`, or an absent field")
        }

        fn visit_none<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(None))
        }

        fn visit_unit<E>(self) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(Some(None))
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            i32::deserialize(deserializer).map(|value| Some(Some(value)))
        }

        fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            i32::try_from(value)
                .map(|v| Some(Some(v)))
                .map_err(E::custom)
        }

        fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            i32::try_from(value)
                .map(|v| Some(Some(v)))
                .map_err(E::custom)
        }
    }

    deserializer.deserialize_option(CeilingVisitor)
}

#[derive(Deserialize)]
struct UpdateUserRequest {
    username: Option<String>,
    password: Option<String>,
    #[serde(default)]
    group_ids: Option<Vec<Uuid>>,
    #[serde(default, deserialize_with = "deserialize_clearable_ceiling")]
    direct_max_instances: Option<Option<i32>>,
}

/// Serialize a user row with its policy rows for admin responses.
fn user_to_json(user: &UserWithPolicy) -> serde_json::Value {
    serde_json::json!({
        "id": user.id,
        "username": user.username,
        "created_at": user.created_at,
        "direct_max_instances": user.direct_max_instances,
        "group_ids": user.group_ids,
        "is_admin": user.is_admin,
        "tier": user.tier,
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
        .list_all_with_policy()
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
        .find_by_id_with_policy(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if auth.can_manage_users() || auth.user_id == id {
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

    repo.find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Tier guardrail: the actor's tier must be strictly greater than the
    // target's, except root. Only an Admin can delete an Admin.
    let target_tier = PolicyRepository::new(&state.db)
        .load_user_tier(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if !auth.is_admin() && auth.context.tier <= target_tier {
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

    let group_repo = GroupRepository::new(&state.db);

    // Default membership: the User system group (zero-groups assignment is out
    // of scope). An explicit non-empty list replaces the default.
    let default_user_group = group_repo
        .find_by_kind("user")
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let group_ids: Vec<Uuid> = match input.group_ids {
        Some(ids) if !ids.is_empty() => ids,
        _ => match default_user_group {
            Some(g) => vec![g.id],
            None => vec![],
        },
    };

    // Assignment guardrail: the actor may only place the target into groups
    // whose tier is strictly below their own.
    validate_assignable_groups(&state.db, &auth, &group_ids).await?;

    let repo = UserRepository::new(&state.db);
    let password_hash = bcrypt::hash(&input.password, 10)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let id = repo
        .create(&input.username, &password_hash)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !group_ids.is_empty() {
        repo.set_group_memberships(id, &group_ids)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    let user = repo
        .find_by_id_with_policy(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "user": user_to_json(&user)
    })))
}

/// The assignment guardrail shared by create/update: every group id must exist
/// (else 400) and every group's tier must be strictly below the actor's (else
/// 403) — an admin cannot place anyone into the Admin group, a manager cannot
/// place anyone into Manager/Admin, etc.
async fn validate_assignable_groups(
    db: &sea_orm::DatabaseConnection,
    auth: &AuthUser,
    group_ids: &[Uuid],
) -> Result<(), StatusCode> {
    if group_ids.is_empty() {
        return Ok(());
    }
    if !validate_group_ids(db, group_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    let groups = GroupRepository::new(db)
        .list_all()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let kinds: Vec<Option<String>> = groups
        .into_iter()
        .filter(|g| group_ids.contains(&g.id))
        .map(|g| g.kind)
        .collect();
    if !can_assign_groups(auth.context.tier, &kinds) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

async fn update_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
    Json(input): Json<UpdateUserRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let repo = UserRepository::new(&state.db);

    repo.find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // A user may always reset their own password; anything else (identity,
    // policy) is user management and requires `can_manage_users`.
    if !auth.can_manage_users() && auth.user_id != id {
        return Err(StatusCode::FORBIDDEN);
    }

    // Escalation guard (spec Decision 5 / user story 15): policy writes
    // (memberships + personal ceiling) to a target require the actor's tier to
    // be strictly greater than the target's — root excepted. This also forbids
    // a non-admin writing their own policy rows: assigning themselves a group
    // (including a privileged one) or a personal ceiling is exactly the
    // self-join escalation the flat model forbids.
    let has_policy_write = input.group_ids.is_some() || input.direct_max_instances.is_some();
    if has_policy_write && !auth.is_admin() {
        let target_tier = PolicyRepository::new(&state.db)
            .load_user_tier(id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .ok_or(StatusCode::NOT_FOUND)?;
        if auth.context.tier <= target_tier {
            return Err(StatusCode::FORBIDDEN);
        }
    }

    if !auth.is_admin() && auth.user_id == id && input.username.is_some() {
        return Err(StatusCode::FORBIDDEN);
    }

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

    // Membership and personal overrides apply only to the target user; the
    // tier guard above already rejected self-writes by non-admins.
    if let Some(group_ids) = input.group_ids.as_deref() {
        validate_assignable_groups(&state.db, &auth, group_ids).await?;
        repo.set_group_memberships(id, group_ids)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }
    if let Some(direct) = input.direct_max_instances {
        if let Some(ceiling) = direct
            && ceiling < 0 {
                return Err(StatusCode::BAD_REQUEST);
            }
        repo.set_direct_max_instances(id, direct)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    }

    let updated = repo
        .update(id, input.username.as_deref(), password_hash.as_deref())
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if updated {
        let user = repo
            .find_by_id_with_policy(id)
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
