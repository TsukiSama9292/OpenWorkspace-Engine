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
use crate::db::{WorkspaceTemplate, WorkspaceTemplateRepository};

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

fn template_to_json(template: &WorkspaceTemplate, instance_count: i64) -> serde_json::Value {
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
        "container_runtime": if template.container_runtime.is_empty() { "docker" } else { &template.container_runtime },
        "run_config": template.run_config,
        "exec_config": template.exec_config,
        "volume_mappings": template.volume_mappings,
        "persistent_storage_path": template.persistent_storage_path,
        "max_run_seconds": template.max_run_seconds,
        "timeout_action": template.timeout_action,
        "network_bandwidth_up_mbps": template.network_bandwidth_up_mbps,
        "network_bandwidth_down_mbps": template.network_bandwidth_down_mbps,
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
    network_bandwidth_up_mbps: i32,
    #[serde(default)]
    network_bandwidth_down_mbps: i32,
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
    network_bandwidth_up_mbps: i32,
    #[serde(default)]
    network_bandwidth_down_mbps: i32,
}

fn default_image() -> String {
    "tsukisama9292/ow-kasmvnc-ubuntu:jammy".to_string()
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

fn validate_auto_sleep(
    max_run_seconds: Option<i64>,
    timeout_action: &str,
) -> Result<(), (StatusCode, Json<serde_json::Value>)> {
    if let Some(seconds) = max_run_seconds {
        if seconds < 60 {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({"error": "max_run_seconds must be at least 60"})),
            ));
        }
    }
    if !matches!(timeout_action, "remove" | "stop" | "pause") {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "timeout_action must be one of: remove, stop, pause"})),
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

async fn list_templates(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let template_repo = WorkspaceTemplateRepository::new(&state.db);

    let templates = if auth.role.can_view_all_instances() {
        template_repo
            .list_all()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        template_repo
            .list_by_owner(auth.user_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    let mut templates_json = Vec::new();
    for template in &templates {
        let count = template_repo
            .count_instances(template.id)
            .await
            .unwrap_or(0);
        templates_json.push(template_to_json(template, count));
    }

    Ok(Json(serde_json::json!({ "templates": templates_json })))
}

async fn create_template(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<CreateTemplateRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
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

    Ok(Json(serde_json::json!({
        "template": template_to_json(&template, 0)
    })))
}

async fn get_template(
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

    Ok(Json(serde_json::json!({ "template": template_to_json(&template, count) })))
}

async fn update_template(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
    Json(input): Json<UpdateTemplateRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let repo = WorkspaceTemplateRepository::new(&state.db);

    let existing = repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !auth.role.can_manage_templates() && existing.owner_id != auth.user_id {
        return Err(StatusCode::FORBIDDEN);
    }

    validate_auto_sleep(input.max_run_seconds, &input.timeout_action)
        .map_err(|(status, _)| status)?;
    validate_bandwidth(input.network_bandwidth_up_mbps, input.network_bandwidth_down_mbps)
        .map_err(|(status, _)| status)?;

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
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !updated {
        return Err(StatusCode::NOT_FOUND);
    }

    let template = repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let count = repo.count_instances(id).await.unwrap_or(0);

    Ok(Json(serde_json::json!({ "template": template_to_json(&template, count) })))
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

    if !auth.role.can_manage_templates() && existing.owner_id != auth.user_id {
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
