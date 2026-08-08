use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use super::AppState;
use crate::audit::{action, diff_detail, target, AuditEvent};
use crate::auth::AuthUser;
use crate::db::{validate_template_ids, GroupRecord, GroupRepository};
use crate::openapi::GroupListEnvelope;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/groups", get(list_groups).post(create_group))
        .route(
            "/api/groups/{id}",
            put(update_group).delete(delete_group),
        )
}

fn default_max_instances() -> i32 {
    2
}

/// The pinned group-management contract (frontend `GroupInput`): every policy
/// field plus the template whitelist. `max_instances` and the flags default to
/// the schema defaults so a minimal create body still forms a valid group.
#[derive(Deserialize)]
struct GroupInput {
    name: String,
    description: Option<String>,
    #[serde(default)]
    can_create_template: bool,
    #[serde(default)]
    can_manage_users: bool,
    #[serde(default)]
    can_manage_group_instances: bool,
    #[serde(default)]
    can_manage_docker: bool,
    #[serde(default)]
    can_manage_registry: bool,
    #[serde(default)]
    can_view_monitoring: bool,
    #[serde(default)]
    can_view_audit_logs: bool,
    #[serde(default = "default_max_instances")]
    max_instances: i32,
    #[serde(default)]
    template_ids: Vec<Uuid>,
}

/// The pinned `Group` JSON shape.
fn group_to_json(group: &GroupRecord, template_ids: &[Uuid]) -> serde_json::Value {
    serde_json::json!({
        "id": group.id,
        "name": group.name,
        "description": group.description,
        "kind": group.kind,
        "can_create_template": group.can_create_template,
        "can_manage_users": group.can_manage_users,
        "can_manage_group_instances": group.can_manage_group_instances,
        "can_manage_docker": group.can_manage_docker,
        "can_manage_registry": group.can_manage_registry,
        "can_view_monitoring": group.can_view_monitoring,
        "can_view_audit_logs": group.can_view_audit_logs,
        "max_instances": group.max_instances,
        "template_ids": template_ids,
    })
}

fn validate_group_input(input: &GroupInput) -> Result<(), StatusCode> {
    if input.name.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if input.max_instances < 0 {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(())
}

/// De-duplicate while preserving order so a whitelist can never contain a
/// repeated primary key (the join tables key on the pair).
fn dedup_ids(ids: &[Uuid]) -> Vec<Uuid> {
    let mut seen = std::collections::HashSet::new();
    ids.iter().copied().filter(|id| seen.insert(*id)).collect()
}

/// The shared create/update reconcile: shape checks, whitelist de-dup, then
/// existence validation against `workspace_templates`. Returns the de-duped
/// whitelist ready to persist.
async fn validated_template_ids(
    state: &AppState,
    input: &GroupInput,
) -> Result<Vec<Uuid>, StatusCode> {
    validate_group_input(input)?;
    let template_ids = dedup_ids(&input.template_ids);
    if !validate_template_ids(&state.db, &template_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    {
        return Err(StatusCode::BAD_REQUEST);
    }
    Ok(template_ids)
}

#[utoipa::path(
    get,
    path = "/api/groups",
    tag = "admin-gated",
    responses(
        (status = 200, description = "group catalog with template whitelists", body = GroupListEnvelope),
        (status = 401, description = "missing or invalid ow_token"),
        (status = 403, description = "requires can_manage_users or admin"),
        (status = 500, description = "internal server error"),
    )
)]
pub(crate) async fn list_groups(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Reading the group catalog is the user-management surface (the UI lists
    // group names next to memberships); writing it is admin-only below.
    if !auth.can_manage_users() {
        return Err(StatusCode::FORBIDDEN);
    }

    let repo = GroupRepository::new(&state.db);
    let groups = repo
        .list_all()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut groups_json = Vec::with_capacity(groups.len());
    for group in &groups {
        let template_ids = repo
            .list_template_ids(group.id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        groups_json.push(group_to_json(group, &template_ids));
    }

    Ok(Json(serde_json::json!({ "groups": groups_json })))
}

async fn create_group(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<GroupInput>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // Group policy writes are root-only (spec Decision 5): a `can_manage_users`
    // holder must not be able to forge a high-privilege group and then join it.
    if !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let template_ids = validated_template_ids(&state, &input).await?;

    let repo = GroupRepository::new(&state.db);

    if repo
        .find_by_name(&input.name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .is_some()
    {
        return Err(StatusCode::CONFLICT);
    }

    let id = repo
        .create(
            &input.name,
            input.description.as_deref(),
            input.can_create_template,
            input.can_manage_users,
            input.can_manage_group_instances,
            input.can_manage_docker,
            input.can_manage_registry,
            input.can_view_monitoring,
            input.can_view_audit_logs,
            input.max_instances,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    repo.set_template_ids(id, &template_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let group = repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;

    state.audit.emit(
        AuditEvent::from_auth(&auth, action::GROUP_CREATE, target::GROUP)
            .with_target(Some(group.id.to_string()), Some(group.name.clone())),
    );

    Ok(Json(serde_json::json!({
        "group": group_to_json(&group, &template_ids)
    })))
}

async fn update_group(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
    Json(input): Json<GroupInput>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let template_ids = validated_template_ids(&state, &input).await?;

    let repo = GroupRepository::new(&state.db);

    let existing = repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // System groups (kind `admin`/`manager`/`user`) cannot be renamed.
    if existing.kind.is_some() && input.name != existing.name {
        return Err(StatusCode::FORBIDDEN);
    }

    // Renaming onto another existing group's name → conflict.
    if let Some(other) = repo
        .find_by_name(&input.name)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        && other.id != id {
            return Err(StatusCode::CONFLICT);
        }

    // System-group permission flags are fixed: Admin always all-on, User always
    // all-off. `max_instances` stays editable for all three. Custom groups
    // (kind NULL) take the payload verbatim.
    let (can_create_template, can_manage_users, can_manage_group_instances, can_manage_docker, can_manage_registry, can_view_monitoring, can_view_audit_logs) =
        match existing.kind.as_deref() {
            Some("admin") => (true, true, true, true, true, true, true),
            Some("user") => (false, false, false, false, false, false, false),
            _ => (
                input.can_create_template,
                input.can_manage_users,
                input.can_manage_group_instances,
                input.can_manage_docker,
                input.can_manage_registry,
                input.can_view_monitoring,
                input.can_view_audit_logs,
            ),
        };

    let updated = repo
        .update(
            id,
            &input.name,
            input.description.as_deref(),
            can_create_template,
            can_manage_users,
            can_manage_group_instances,
            can_manage_docker,
            can_manage_registry,
            can_view_monitoring,
            can_view_audit_logs,
            Some(input.max_instances),
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !updated {
        return Err(StatusCode::NOT_FOUND);
    }

    let old_template_ids = repo
        .list_template_ids(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    repo.set_template_ids(id, &template_ids)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let group = repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Redacted before/after diff of the edited policy fields.
    let mut changes: Vec<(String, serde_json::Value, serde_json::Value)> = Vec::new();
    if existing.name != group.name {
        changes.push(("name".to_string(), serde_json::json!(&existing.name), serde_json::json!(&group.name)));
    }
    if existing.description != group.description {
        changes.push(("description".to_string(), serde_json::json!(&existing.description), serde_json::json!(&group.description)));
    }
    if existing.can_create_template != group.can_create_template {
        changes.push(("can_create_template".to_string(), serde_json::json!(existing.can_create_template), serde_json::json!(group.can_create_template)));
    }
    if existing.can_manage_users != group.can_manage_users {
        changes.push(("can_manage_users".to_string(), serde_json::json!(existing.can_manage_users), serde_json::json!(group.can_manage_users)));
    }
    if existing.can_manage_group_instances != group.can_manage_group_instances {
        changes.push(("can_manage_group_instances".to_string(), serde_json::json!(existing.can_manage_group_instances), serde_json::json!(group.can_manage_group_instances)));
    }
    if existing.can_manage_docker != group.can_manage_docker {
        changes.push(("can_manage_docker".to_string(), serde_json::json!(existing.can_manage_docker), serde_json::json!(group.can_manage_docker)));
    }
    if existing.can_manage_registry != group.can_manage_registry {
        changes.push(("can_manage_registry".to_string(), serde_json::json!(existing.can_manage_registry), serde_json::json!(group.can_manage_registry)));
    }
    if existing.can_view_monitoring != group.can_view_monitoring {
        changes.push(("can_view_monitoring".to_string(), serde_json::json!(existing.can_view_monitoring), serde_json::json!(group.can_view_monitoring)));
    }
    if existing.can_view_audit_logs != group.can_view_audit_logs {
        changes.push(("can_view_audit_logs".to_string(), serde_json::json!(existing.can_view_audit_logs), serde_json::json!(group.can_view_audit_logs)));
    }
    if existing.max_instances != group.max_instances {
        changes.push(("max_instances".to_string(), serde_json::json!(existing.max_instances), serde_json::json!(group.max_instances)));
    }
    // The template whitelist is the core permission surface (template
    // authorization is group-only), so a change to it must appear in the diff
    // even though it lives in `group_templates`, not on the group row.
    if old_template_ids != template_ids {
        changes.push((
            "template_ids".to_string(),
            serde_json::json!(old_template_ids),
            serde_json::json!(template_ids),
        ));
    }

    state.audit.emit(
        AuditEvent::from_auth(&auth, action::GROUP_UPDATE, target::GROUP)
            .with_target(Some(group.id.to_string()), Some(group.name.clone()))
            .with_detail(diff_detail(&changes)),
    );

    Ok(Json(serde_json::json!({
        "group": group_to_json(&group, &template_ids)
    })))
}

async fn delete_group(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
) -> Result<StatusCode, StatusCode> {
    if !auth.is_admin() {
        return Err(StatusCode::FORBIDDEN);
    }

    let repo = GroupRepository::new(&state.db);

    // System groups (kind `admin`/`manager`/`user`) cannot be deleted.
    let existing = repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    if existing.kind.is_some() {
        return Err(StatusCode::FORBIDDEN);
    }

    // `user_groups` / `group_templates` rows cascade at the database level
    // (FKs from migration 000018), so no join-row cleanup is needed here.
    let deleted = repo
        .delete(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if deleted {
        state.audit.emit(
            AuditEvent::from_auth(&auth, action::GROUP_DELETE, target::GROUP)
                .with_target(Some(existing.id.to_string()), Some(existing.name.clone())),
        );
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}
