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
use crate::db::{WorkspaceConfig, WorkspaceConfigRepository};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/configs",
            get(list_configs).post(create_config),
        )
        .route(
            "/api/configs/{id}",
            get(get_config).put(update_config).delete(delete_config),
        )
}

fn config_to_json(config: &WorkspaceConfig, instance_count: i64) -> serde_json::Value {
    serde_json::json!({
        "id": config.id,
        "name": config.name,
        "description": config.description,
        "owner_id": config.owner_id,
        "image": config.image,
        "cores": config.cores,
        "memory": config.memory,
        "gpu_count": config.gpu_count,
        "docker_registry": config.docker_registry,
        "run_config": config.run_config,
        "exec_config": config.exec_config,
        "volume_mappings": config.volume_mappings,
        "persistent_storage_path": config.persistent_storage_path,
        "instance_count": instance_count,
        "created_at": config.created_at,
        "updated_at": config.updated_at,
    })
}

#[derive(Deserialize)]
struct CreateConfigRequest {
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
    #[serde(default)]
    run_config: serde_json::Value,
    #[serde(default)]
    exec_config: serde_json::Value,
    #[serde(default)]
    volume_mappings: serde_json::Value,
    persistent_storage_path: Option<String>,
}

#[derive(Deserialize)]
struct UpdateConfigRequest {
    name: String,
    description: Option<String>,
    image: String,
    cores: i32,
    memory: i64,
    gpu_count: i32,
    docker_registry: Option<String>,
    run_config: serde_json::Value,
    exec_config: serde_json::Value,
    volume_mappings: serde_json::Value,
    persistent_storage_path: Option<String>,
}

fn default_image() -> String {
    "kasmweb/desktop:1.19.0-rolling-daily".to_string()
}
fn default_cores() -> i32 {
    2
}
fn default_memory() -> i64 {
    4_294_967_296
}

async fn list_configs(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let config_repo = WorkspaceConfigRepository::new(&state.db);

    let configs = if auth.role == "admin" {
        config_repo
            .list_all()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        config_repo
            .list_by_owner(auth.user_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    let mut configs_json = Vec::new();
    for config in &configs {
        let count = config_repo
            .count_instances(config.id)
            .await
            .unwrap_or(0);
        configs_json.push(config_to_json(config, count));
    }

    Ok(Json(serde_json::json!({ "configs": configs_json })))
}

async fn create_config(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<CreateConfigRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let repo = WorkspaceConfigRepository::new(&state.db);

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

    let config = repo
        .create(
            &input.name,
            input.description.as_deref(),
            auth.user_id,
            &input.image,
            input.cores,
            input.memory,
            input.gpu_count,
            input.docker_registry.as_deref(),
            &run_config,
            &exec_config,
            &volume_mappings,
            input.persistent_storage_path.as_deref(),
        )
        .await
        .map_err(|e| {
            tracing::error!("Failed to create config: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to create config"})),
            )
        })?;

    tracing::info!("Config '{}' created (id={})", config.name, config.id);

    Ok(Json(serde_json::json!({
        "config": config_to_json(&config, 0)
    })))
}

async fn get_config(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let config_repo = WorkspaceConfigRepository::new(&state.db);

    let config = config_repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let count = config_repo
        .count_instances(id)
        .await
        .unwrap_or(0);

    Ok(Json(serde_json::json!({ "config": config_to_json(&config, count) })))
}

async fn update_config(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
    Json(input): Json<UpdateConfigRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let repo = WorkspaceConfigRepository::new(&state.db);

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
            &input.run_config,
            &input.exec_config,
            &input.volume_mappings,
            input.persistent_storage_path.as_deref(),
        )
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if !updated {
        return Err(StatusCode::NOT_FOUND);
    }

    let config = repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let count = repo.count_instances(id).await.unwrap_or(0);

    Ok(Json(serde_json::json!({ "config": config_to_json(&config, count) })))
}

async fn delete_config(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Result<StatusCode, StatusCode> {
    let repo = WorkspaceConfigRepository::new(&state.db);

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
