use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use super::super::AppState;
use crate::auth::AuthUser;
use crate::db::{GroupRepository, WorkspaceTemplate, WorkspaceTemplateRepository};
use crate::effective_context::TemplateVisibility;
use crate::openapi::{TemplateEnvelope, TemplateListEnvelope};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/templates",
            get(list_templates).post(create_template),
        )
        .route(
            "/api/templates/{id}",
            get(get_template).put(update_template).delete(delete_template),
        )
}

fn template_to_json(
    template: &WorkspaceTemplate,
    instance_count: i64,
    default_container_runtime: &str,
) -> serde_json::Value {
    serde_json::json!({
        "id": template.id,
        "name": template.name,
        "description": template.description,
        "owner_id": template.owner_id,
        "image": template.image,
        "cores": template.cores,
        "memory": template.memory,
        "gpu_count": template.gpu_count,
        "docker_registry": template.docker_registry,
        "remote_type": template.remote_type,
        "container_runtime": if template.container_runtime.is_empty() {
            default_container_runtime
        } else {
            &template.container_runtime
        },
        "run_config": template.run_config,
        "exec_config": template.exec_config,
        "volume_mappings": template.volume_mappings,
        "persistent_storage_path": template.persistent_storage_path,
        "max_run_seconds": template.max_run_seconds,
        "timeout_action": template.timeout_action,
        "keep_time_seconds": template.keep_time_seconds,
        "keep_time_action": template.keep_time_action,
        "network_bandwidth_up_mbps": template.network_bandwidth_up_mbps,
        "network_bandwidth_down_mbps": template.network_bandwidth_down_mbps,
        "docker_in_instance": template.docker_in_instance,
        "visibility": template.visibility,
        "instance_count": instance_count,
        "created_at": template.created_at,
        "updated_at": template.updated_at,
    })
}

#[derive(Deserialize)]
struct CreateTemplateRequest {
    name: String,
    description: Option<String>,
    #[serde(default = "default_image")]
    image: String,
    #[serde(default = "default_cores")]
    cores: i32,
    #[serde(default = "default_memory")]
    memory: i64,
    #[serde(default)]
    gpu_count: i32,
    docker_registry: Option<String>,
    #[serde(default = "default_remote_type")]
    remote_type: String,
    #[serde(default)]
    run_config: serde_json::Value,
    #[serde(default)]
    exec_config: serde_json::Value,
    #[serde(default)]
    volume_mappings: serde_json::Value,
    persistent_storage_path: Option<String>,
    #[serde(default = "default_container_runtime")]
    container_runtime: String,
    #[serde(default)]
    max_run_seconds: Option<i64>,
    #[serde(default = "default_timeout_action")]
    timeout_action: String,
    #[serde(default)]
    keep_time_seconds: Option<i64>,
    #[serde(default = "default_keep_time_action")]
    keep_time_action: String,
    #[serde(default)]
    network_bandwidth_up_mbps: i32,
    #[serde(default)]
    network_bandwidth_down_mbps: i32,
    #[serde(default)]
    docker_in_instance: bool,
    #[serde(default)]
    visibility: TemplateVisibility,
}

#[derive(Deserialize)]
struct UpdateTemplateRequest {
    name: String,
    description: Option<String>,
    image: String,
    cores: i32,
    memory: i64,
    gpu_count: i32,
    docker_registry: Option<String>,
    #[serde(default = "default_remote_type")]
    remote_type: String,
    run_config: serde_json::Value,
    exec_config: serde_json::Value,
    volume_mappings: serde_json::Value,
    persistent_storage_path: Option<String>,
    #[serde(default = "default_container_runtime")]
    container_runtime: String,
    #[serde(default)]
    max_run_seconds: Option<i64>,
    #[serde(default = "default_timeout_action")]
    timeout_action: String,
    #[serde(default)]
    keep_time_seconds: Option<i64>,
    #[serde(default = "default_keep_time_action")]
    keep_time_action: String,
    #[serde(default)]
    network_bandwidth_up_mbps: i32,
    #[serde(default)]
    network_bandwidth_down_mbps: i32,
    #[serde(default)]
    docker_in_instance: bool,
    #[serde(default)]
    visibility: TemplateVisibility,
}

fn default_image() -> String {
    "tsukisama9292/ow-kasmvnc-ubuntu-dini:jammy".to_string()
}
fn default_cores() -> i32 {
    2
}
fn default_memory() -> i64 {
    4_294_967_296
}
fn default_remote_type() -> String {
    "kasmvnc".to_string()
}
fn default_container_runtime() -> String {
    "docker".to_string()
}
fn default_timeout_action() -> String {
    "remove".to_string()
}

fn default_keep_time_action() -> String {
    "pause".to_string()
}

fn validate_auto_sleep(
    max_run_seconds: Option<i64>,
    timeout_action: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if let Some(seconds) = max_run_seconds
        && seconds < 60 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "max_run_seconds must be at least 60"})),
            ));
        }
    if !matches!(timeout_action, "remove" | "stop" | "pause") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "timeout_action must be one of: remove, stop, pause"})),
        ));
    }
    Ok(())
}

fn validate_keep_time(
    keep_time_seconds: Option<i64>,
    keep_time_action: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if let Some(seconds) = keep_time_seconds
        && seconds < 60 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "keep_time_seconds must be at least 60"})),
            ));
        }
    if !matches!(keep_time_action, "remove" | "stop" | "pause") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "keep_time_action must be one of: remove, stop, pause"})),
        ));
    }
    Ok(())
}

fn validate_bandwidth(
    up_mbps: i32,
    down_mbps: i32,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if up_mbps < 0 || down_mbps < 0 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": "network_bandwidth_up_mbps and network_bandwidth_down_mbps must be >= 0 (0 = unlimited)"
            })),
        ));
    }
    Ok(())
}

#[utoipa::path(
    get,
    path = "/api/templates",
    tag = "templates",
    responses(
        (status = 200, description = "templates catalog", body = TemplateListEnvelope),
        (status = 401, description = "missing or invalid ow_token"),
        (status = 500, description = "internal server error"),
    )
)]
pub(crate) async fn list_templates(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let template_repo = WorkspaceTemplateRepository::new(&state.db);

    // Templates are a global browsable catalog (spec Decision 5): every
    // authenticated user lists/view the full catalog — including hidden
    // templates, so the templates-management UI can display and restore them.
    // Launch is gated by the effective whitelist, not by visibility; the
    // launch/session surface (not the catalog) decides which templates to show.
    let templates = template_repo
        .list_all()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut templates_json = Vec::new();
    for template in &templates {
        let count = template_repo
            .count_instances(template.id)
            .await
            .unwrap_or(0);
        templates_json.push(template_to_json(template, count, &state.settings.container_runtime));
    }

    Ok(Json(serde_json::json!({ "templates": templates_json })))
}

async fn create_template(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<CreateTemplateRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if !auth.is_admin() && !auth.context.can_create_template {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Forbidden"})),
        ));
    }

    let repo = WorkspaceTemplateRepository::new(&state.db);

    let run_config = if input.run_config.is_null() {
        serde_json::json!({})
    } else {
        input.run_config
    };
    let exec_config = if input.exec_config.is_null() {
        serde_json::json!({})
    } else {
        input.exec_config
    };
    let volume_mappings = if input.volume_mappings.is_null() {
        serde_json::json!({})
    } else {
        input.volume_mappings
    };

    validate_auto_sleep(input.max_run_seconds, &input.timeout_action)?;
    validate_keep_time(input.keep_time_seconds, &input.keep_time_action)?;
    validate_bandwidth(input.network_bandwidth_up_mbps, input.network_bandwidth_down_mbps)?;

    let template = repo
        .create(
            &input.name,
            input.description.as_deref(),
            auth.user_id,
            &input.image,
            input.cores,
            input.memory,
            input.gpu_count,
            input.docker_registry.as_deref(),
            &input.remote_type,
            &input.container_runtime,
            &run_config,
            &exec_config,
            &volume_mappings,
            input.persistent_storage_path.as_deref(),
            input.max_run_seconds,
            &input.timeout_action,
            input.network_bandwidth_up_mbps,
            input.network_bandwidth_down_mbps,
            input.keep_time_seconds,
            &input.keep_time_action,
            input.docker_in_instance,
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to create template: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to create template"})),
            )
        })?;

    tracing::info!("Template '{}' created (id={})", template.name, template.id);

    // Visibility: the repository create leaves the DB default (`private`)
    // untouched (template-visibility spec Decision 1); apply a non-default
    // value via the dedicated writer and re-read so the response reflects it.
    let template = if input.visibility != TemplateVisibility::Private {
        repo.set_visibility(template.id, input.visibility)
            .await
            .map_err(|e| {
                tracing::error!("Failed to set template visibility: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Failed to create template"})),
                )
            })?;
        repo.find_by_id(template.id)
            .await
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Failed to create template"})),
                )
            })?
            .ok_or((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to create template"})),
            ))?
    } else {
        template
    };

    // Group-only template authorization: a new template whitelists the Admin
    // group by default (no other group), so it is admin-usable until the
    // Admin group's whitelist is edited via the group-management API.
    if let Ok(Some(admin_group)) = GroupRepository::new(&state.db)
        .find_by_kind("admin")
        .await
    {
        let group_repo = GroupRepository::new(&state.db);
        let mut ids = group_repo
            .list_template_ids(admin_group.id)
            .await
            .unwrap_or_default();
        if !ids.contains(&template.id) {
            ids.push(template.id);
            if let Err(e) = group_repo.set_template_ids(admin_group.id, &ids).await {
                tracing::error!("Failed to whitelist Admin group on new template: {}", e);
            }
        }
    }

    Ok(Json(serde_json::json!({
        "template": template_to_json(&template, 0, &state.settings.container_runtime)
    })))
}

#[utoipa::path(
    get,
    path = "/api/templates/{id}",
    tag = "templates",
    params(
        ("id" = Uuid, description = "template uuid"),
    ),
    responses(
        (status = 200, description = "template detail", body = TemplateEnvelope),
        (status = 400, description = "invalid uuid"),
        (status = 401, description = "missing or invalid ow_token"),
        (status = 404, description = "template not found"),
        (status = 500, description = "internal server error"),
    )
)]
pub(crate) async fn get_template(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let template_repo = WorkspaceTemplateRepository::new(&state.db);

    let template = template_repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let count = template_repo
        .count_instances(id)
        .await
        .unwrap_or(0);

    Ok(Json(serde_json::json!({ "template": template_to_json(&template, count, &state.settings.container_runtime) })))
}

async fn update_template(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
    Json(input): Json<UpdateTemplateRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let repo = WorkspaceTemplateRepository::new(&state.db);

    let existing = repo
        .find_by_id(id)
        .await
        .map_err(|_| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to find template"})),
        ))?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Template not found"})),
        ))?;

    // Edit requires `can_create_template` AND ownership; a system admin may
    // edit any template (spec Decision 5).
    if !auth.is_admin()
        && !(auth.context.can_create_template && existing.owner_id == auth.user_id)
    {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Forbidden"})),
        ));
    }

    validate_auto_sleep(input.max_run_seconds, &input.timeout_action)?;
    validate_keep_time(input.keep_time_seconds, &input.keep_time_action)?;
    validate_bandwidth(input.network_bandwidth_up_mbps, input.network_bandwidth_down_mbps)?;

    let updated = repo
        .update(
            id,
            &input.name,
            input.description.as_deref(),
            &input.image,
            input.cores,
            input.memory,
            input.gpu_count,
            input.docker_registry.as_deref(),
            &input.remote_type,
            &input.container_runtime,
            &input.run_config,
            &input.exec_config,
            &input.volume_mappings,
            input.persistent_storage_path.as_deref(),
            input.max_run_seconds,
            &input.timeout_action,
            input.network_bandwidth_up_mbps,
            input.network_bandwidth_down_mbps,
            input.keep_time_seconds,
            &input.keep_time_action,
            input.docker_in_instance,
        )
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to update template"})),
            )
        })?;

    if !updated {
        return Err((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Template not found"})),
        ));
    }

    if existing.visibility != input.visibility {
        repo.set_visibility(id, input.visibility)
            .await
            .map_err(|_| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Failed to update template"})),
                )
            })?;
    }

    let template = repo
        .find_by_id(id)
        .await
        .map_err(|_| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to find template"})),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Template not found"})),
        ))?;

    let count = repo.count_instances(id).await.unwrap_or(0);

    Ok(Json(serde_json::json!({ "template": template_to_json(&template, count, &state.settings.container_runtime) })))
}

async fn delete_template(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
) -> Result<StatusCode, StatusCode> {
    let repo = WorkspaceTemplateRepository::new(&state.db);

    let existing = repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    // Delete requires `can_create_template` AND ownership; a system admin may
    // delete any template (spec Decision 5).
    if !auth.is_admin()
        && !(auth.context.can_create_template && existing.owner_id == auth.user_id)
    {
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
