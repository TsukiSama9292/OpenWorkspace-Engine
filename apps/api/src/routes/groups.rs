use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use super::AppState;
use crate::activation::{sum_resources_for_group, sum_resources_for_user_in_group};
use crate::audit::{action, diff_detail, target, AuditEvent};
use crate::auth::AuthUser;
use crate::db::{
    validate_template_ids, GroupRecord, GroupRepository, PolicyRepository, UserRepository,
    UserWithPolicy, WorkspaceInstanceRepository,
};
use crate::effective_context::{group_kind_tier, ResourceUse};
use crate::openapi::{GroupBillingEnvelope, GroupListEnvelope};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/groups", get(list_groups).post(create_group))
        .route(
            "/api/groups/{id}",
            put(update_group).delete(delete_group),
        )
        .route(
            "/api/groups/{id}/members/{user_id}/quota",
            put(update_member_quota),
        )
        .route("/api/groups/{id}/billing", get(group_billing))
}

fn default_max_instances() -> i32 {
    2
}

fn default_blocked_cpu() -> i32 {
    0
}

fn default_blocked_memory() -> i64 {
    0
}

fn default_blocked_gpu() -> i32 {
    0
}

fn default_billing_model() -> String {
    "shared".to_string()
}

/// The pinned group-management contract (frontend `GroupInput`): every policy
/// field plus the template whitelist and the resource pool. `max_instances`,
/// the flags, and the pool defaults match the schema defaults so a minimal
/// create body still forms a valid group.
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
    #[serde(default = "default_billing_model")]
    billing_model: String,
    /// `-1` = unlimited, `0` = blocked. Omitted pool fields default to `0`
    /// (blocked, spec Decision 1 / Story 16) so a new group never opens an
    /// ungoverned pool by accident.
    #[serde(default = "default_blocked_cpu")]
    pool_cpu_cores: i32,
    #[serde(default = "default_blocked_memory")]
    pool_memory_mb: i64,
    #[serde(default = "default_blocked_gpu")]
    pool_gpu_count: i32,
}

/// The pinned `Group` JSON shape.
fn group_to_json(
    group: &GroupRecord,
    template_ids: &[Uuid],
    members: &[serde_json::Value],
) -> serde_json::Value {
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
        "billing_model": group.billing_model,
        "pool_cpu_cores": group.pool_cpu_cores,
        "pool_memory_mb": group.pool_memory_mb,
        "pool_gpu_count": group.pool_gpu_count,
        "template_ids": template_ids,
        "members": members,
    })
}

fn validate_group_input(input: &GroupInput) -> Result<(), StatusCode> {
    if input.name.trim().is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    if input.max_instances < -1 {
        return Err(StatusCode::BAD_REQUEST);
    }
    if input.billing_model != "shared" && input.billing_model != "dedicated" {
        return Err(StatusCode::BAD_REQUEST);
    }
    if input.pool_cpu_cores < -1 || input.pool_memory_mb < -1 || input.pool_gpu_count < -1 {
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

    // One pass over every user for the member rows (id, username, tier), so a
    // group catalog with many members never degrades into N queries per group.
    let user_repo = UserRepository::new(&state.db);
    let users = user_repo
        .list_all_with_policy()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let user_by_id: std::collections::HashMap<Uuid, &UserWithPolicy> =
        users.iter().map(|u| (u.id, u)).collect();

    let mut groups_json = Vec::with_capacity(groups.len());
    for group in &groups {
        let template_ids = repo
            .list_template_ids(group.id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let member_quotas = user_repo
            .list_membership_quotas(group.id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        let members = member_quotas
            .iter()
            .map(|mq| {
                let user = user_by_id.get(&mq.user_id);
                serde_json::json!({
                    "user_id": mq.user_id,
                    "username": user.map(|u| u.username.as_str()).unwrap_or(""),
                    "tier": user.map(|u| u.tier).unwrap_or(crate::effective_context::TIER_USER),
                    "cpu_quota": mq.cpu_quota,
                    "memory_quota": mq.memory_quota,
                    "gpu_quota": mq.gpu_quota,
                })
            })
            .collect::<Vec<_>>();
        groups_json.push(group_to_json(group, &template_ids, &members));
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
            &input.billing_model,
            input.pool_cpu_cores,
            input.pool_memory_mb,
            input.pool_gpu_count,
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
        "group": group_to_json(&group, &template_ids, &[])
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

    // Pool-tightening guards (spec Decision 7): lowering a pool to a *finite*
    // value is rejected while a member's finite quota sits above it (the
    // `0 <= member quota <= pool` invariant) or while an active instance with a
    // `-1` snapshot on that resource is attributed to the group (a whole-layer
    // consumer that would silently stop counting). A member at `-1` is bounded
    // only by the pool, so it never blocks a finite pool.
    let members = UserRepository::new(&state.db)
        .list_membership_quotas(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let unlimited = WorkspaceInstanceRepository::new(&state.db)
        .count_active_unlimited_snapshots(None, Some(id))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if input.pool_cpu_cores >= 0
        && existing.pool_cpu_cores != input.pool_cpu_cores
        && (members.iter().any(|m| m.cpu_quota > input.pool_cpu_cores)
            || unlimited.cpu_cores > 0)
    {
        return Err(StatusCode::CONFLICT);
    }
    if input.pool_memory_mb >= 0
        && existing.pool_memory_mb != input.pool_memory_mb
        && (members.iter().any(|m| m.memory_quota > input.pool_memory_mb)
            || unlimited.memory_mb > 0)
    {
        return Err(StatusCode::CONFLICT);
    }
    if input.pool_gpu_count >= 0
        && existing.pool_gpu_count != input.pool_gpu_count
        && (members.iter().any(|m| m.gpu_quota > input.pool_gpu_count)
            || unlimited.gpu_count > 0)
    {
        return Err(StatusCode::CONFLICT);
    }

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
            &input.billing_model,
            input.pool_cpu_cores,
            input.pool_memory_mb,
            input.pool_gpu_count,
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
    if existing.billing_model != group.billing_model {
        changes.push(("billing_model".to_string(), serde_json::json!(&existing.billing_model), serde_json::json!(&group.billing_model)));
    }
    if existing.pool_cpu_cores != group.pool_cpu_cores {
        changes.push(("pool_cpu_cores".to_string(), serde_json::json!(existing.pool_cpu_cores), serde_json::json!(group.pool_cpu_cores)));
    }
    if existing.pool_memory_mb != group.pool_memory_mb {
        changes.push(("pool_memory_mb".to_string(), serde_json::json!(existing.pool_memory_mb), serde_json::json!(group.pool_memory_mb)));
    }
    if existing.pool_gpu_count != group.pool_gpu_count {
        changes.push(("pool_gpu_count".to_string(), serde_json::json!(existing.pool_gpu_count), serde_json::json!(group.pool_gpu_count)));
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
        "group": group_to_json(&group, &template_ids, &[])
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

    // A group that is still an active resource-billing target cannot be
    // deleted: its pool is what active instances bill against, and dropping it
    // would silently orphan that accounting. Stopped instances do not block —
    // their `owner_group_id` becomes `NULL` (`ON DELETE SET NULL`) and they are
    // re-attributed on their next restart. The FK is a hard fallback, but the
    // route refuses first.
    if repo
        .count_active_instances_billed_to(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        > 0
    {
        return Err(StatusCode::CONFLICT);
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

/// The per-member resource-cap edit body. Each value follows the `0` /
/// `>0` / `-1` convention (`-1` = unlimited).
#[derive(serde::Deserialize)]
struct MembershipQuotaInput {
    cpu_quota: i32,
    memory_quota: i64,
    gpu_quota: i32,
}

/// Set a membership's resource cap inside a group (`PUT /api/groups/{id}/
/// members/{user_id}/quota`, spec §7). Gates:
///
/// 1. the actor holds `can_manage_users`;
/// 2. the target user's tier is strictly below the actor's (so a manager
///    cannot edit a fellow-manager or an admin);
/// 3. the membership's group tier is strictly below the actor's (so a manager
///    cannot inflate a quota inside the Manager or Admin group).
///
/// Values are `-1` or bounded by the group pool; lowering to a finite value
/// while an active whole-layer consumer (`-1` snapshot) is attributed to
/// (user, group) is rejected with `409`.
async fn update_member_quota(
    State(state): State<AppState>,
    Path((group_id, user_id)): Path<(Uuid, Uuid)>,
    auth: AuthUser,
    Json(input): Json<MembershipQuotaInput>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !auth.can_manage_users() {
        return Err(StatusCode::FORBIDDEN);
    }

    let repo = UserRepository::new(&state.db);
    let group_repo = GroupRepository::new(&state.db);

    let group = group_repo
        .find_by_id(group_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let membership = repo
        .get_membership_quota(user_id, group_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Tier guardrails: both the target's tier and the membership's group tier
    // must be strictly below the actor's (spec user stories 20-21).
    let target_tier = PolicyRepository::new(&state.db)
        .load_user_tier(user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    let group_tier = group_kind_tier(group.kind.as_deref());
    if auth.context.tier <= target_tier || auth.context.tier <= group_tier {
        return Err(StatusCode::FORBIDDEN);
    }

    // Validation: each value is `-1` (unlimited, bounded only by the pool) or
    // within the pool — `0..=pool` when the pool is finite, any `>= 0` when
    // the pool is unlimited. A member can never be granted more than the pool
    // holds (spec user story 21).
    let pool = ResourceUse {
        cpu_cores: group.pool_cpu_cores as i64,
        memory_mb: group.pool_memory_mb,
        gpu_count: group.pool_gpu_count as i64,
    };
    let values: [(&str, i64, i64); 3] = [
        ("cpu_quota", input.cpu_quota as i64, pool.cpu_cores),
        ("memory_quota", input.memory_quota, pool.memory_mb),
        ("gpu_quota", input.gpu_quota as i64, pool.gpu_count),
    ];
    for (name, value, pool_value) in values {
        if value == -1 {
            continue;
        }
        if value < -1 || (pool_value != -1 && value > pool_value) {
            tracing::debug!(
                "Rejecting out-of-pool membership quota: {name}={value} pool={pool_value}"
            );
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    // Whole-layer-consumer guard (spec §7): lowering to a *finite* value while
    // an active instance with a `-1` snapshot on that resource is attributed
    // to (user, group) would let it silently stop counting — reject `409` so
    // the admin stops those instances before imposing limits.
    let unlimited = WorkspaceInstanceRepository::new(&state.db)
        .count_active_unlimited_snapshots(Some(user_id), Some(group_id))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let blocks_tightening = (input.cpu_quota >= 0 && unlimited.cpu_cores > 0)
        || (input.memory_quota >= 0 && unlimited.memory_mb > 0)
        || (input.gpu_quota >= 0 && unlimited.gpu_count > 0);
    if blocks_tightening {
        return Err(StatusCode::CONFLICT);
    }

    let updated = repo
        .update_membership_quota(
            user_id,
            group_id,
            input.cpu_quota,
            input.memory_quota,
            input.gpu_quota,
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !updated {
        return Err(StatusCode::NOT_FOUND);
    }

    state.audit.emit(
        AuditEvent::from_auth(&auth, action::GROUP_QUOTA_CHANGE, target::GROUP)
            .with_target(Some(group.id.to_string()), Some(group.name.clone()))
            .with_detail(diff_detail(&[
                (
                    "cpu_quota".to_string(),
                    serde_json::json!(membership.cpu_quota),
                    serde_json::json!(input.cpu_quota),
                ),
                (
                    "memory_quota".to_string(),
                    serde_json::json!(membership.memory_quota),
                    serde_json::json!(input.memory_quota),
                ),
                (
                    "gpu_quota".to_string(),
                    serde_json::json!(membership.gpu_quota),
                    serde_json::json!(input.gpu_quota),
                ),
            ])),
    );

    Ok(Json(serde_json::json!({
        "membership_quota": {
            "user_id": user_id,
            "group_id": group_id,
            "cpu_quota": input.cpu_quota,
            "memory_quota": input.memory_quota,
            "gpu_quota": input.gpu_quota,
        }
    })))
}

/// The resource-billing view for a group: the pool, the aggregate usage billed
/// against it, and a per-member usage breakdown. Used by the admin / group
/// manager to see who is consuming the pool.
#[utoipa::path(
    get,
    path = "/api/groups/{id}/billing",
    tag = "admin-gated",
    params(
        ("id" = Uuid, description = "group uuid"),
    ),
    responses(
        (status = 200, description = "group pool usage", body = GroupBillingEnvelope),
        (status = 401, description = "missing or invalid ow_token"),
        (status = 403, description = "requires can_manage_users, can_manage_group_instances, or admin"),
        (status = 404, description = "group not found"),
        (status = 500, description = "internal server error"),
    )
)]
pub(crate) async fn group_billing(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    // The pool-usage view is the management surface: user managers (who shape
    // groups) and group-instance managers (who watch a group's load) both get
    // it; tenants only ever see their own aggregate on `/auth/me`.
    if !auth.can_manage_users() && !auth.context.can_manage_group_instances {
        return Err(StatusCode::FORBIDDEN);
    }

    let repo = GroupRepository::new(&state.db);
    let group = repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let used = sum_resources_for_group(&state.db, id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let member_ids = UserRepository::new(&state.db)
        .list_group_members(&[id])
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut members = Vec::with_capacity(member_ids.len());
    for user_id in member_ids {
        let username = UserRepository::new(&state.db)
            .find_by_id(user_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
            .map(|u| u.username)
            .unwrap_or_default();
        let member_used = sum_resources_for_user_in_group(&state.db, user_id, id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
        members.push(serde_json::json!({
            "user_id": user_id,
            "username": username,
            "used": member_used,
        }));
    }

    Ok(Json(serde_json::json!({
        "group": {
            "id": group.id,
            "name": group.name,
            "billing_model": group.billing_model,
            "pool_cpu_cores": group.pool_cpu_cores,
            "pool_memory_mb": group.pool_memory_mb,
            "pool_gpu_count": group.pool_gpu_count,
        },
        "used": used,
        "members": members,
    })))
}
