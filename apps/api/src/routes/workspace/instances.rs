use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use super::super::AppState;
use crate::auth::AuthUser;
use crate::db::{UserRepository, WorkspaceConfigRepository, WorkspaceInstance, WorkspaceInstanceRepository};
use crate::docker::{ContainerConfig, DockerClient};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/instances",
            get(list_instances).post(launch_instance),
        )
        .route("/api/instances/{id}", get(get_instance).delete(delete_instance))
        .route("/api/instances/{id}/start", post(start_instance))
        .route("/api/instances/{id}/stop", post(stop_instance))
        .route("/api/instances/{id}/pause", post(pause_instance))
        .route("/api/instances/{id}/unpause", post(unpause_instance))
}

fn instance_to_json(inst: &WorkspaceInstance, config_name: Option<&str>, owner_username: Option<&str>) -> serde_json::Value {
    serde_json::json!({
        "id": inst.id,
        "config_id": inst.config_id,
        "name": inst.name,
        "instance_number": inst.instance_number,
        "owner_id": inst.owner_id,
        "owner_username": owner_username.unwrap_or(""),
        "container_id": inst.container_id,
        "status": inst.status,
        "vnc_token": inst.vnc_token,
        "mount_persistent": inst.mount_persistent,
        "resolved_volume_host_path": inst.resolved_volume_host_path,
        "config_name": config_name,
        "created_at": inst.created_at,
        "updated_at": inst.updated_at,
    })
}

#[derive(Deserialize)]
struct LaunchInstanceRequest {
    config_id: Uuid,
    mount_persistent: Option<bool>,
    resolved_volume_host_path: Option<String>,
}

async fn list_instances(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let instance_repo = WorkspaceInstanceRepository::new(&state.db);
    let config_repo = WorkspaceConfigRepository::new(&state.db);

    let instances = if auth.role == "admin" {
        instance_repo
            .list_all()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        instance_repo
            .list_by_owner(auth.user_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    let mut config_names = std::collections::HashMap::new();
    for inst in &instances {
        if !config_names.contains_key(&inst.config_id) {
            if let Ok(Some(config)) = config_repo.find_by_id(inst.config_id).await {
                config_names.insert(inst.config_id, config.name);
            }
        }
    }

    let user_repo = UserRepository::new(&state.db);
    let mut owner_usernames = std::collections::HashMap::new();
    for inst in &instances {
        if !owner_usernames.contains_key(&inst.owner_id) {
            if let Ok(Some(user)) = user_repo.find_by_id(inst.owner_id).await {
                owner_usernames.insert(inst.owner_id, user.1);
            }
        }
    }

    let instances_json: Vec<_> = instances
        .iter()
        .map(|inst| {
            let config_name = config_names.get(&inst.config_id).map(|s| s.as_str());
            let owner_username = owner_usernames.get(&inst.owner_id).map(|s| s.as_str());
            instance_to_json(inst, config_name, owner_username)
        })
        .collect();

    Ok(Json(serde_json::json!({ "instances": instances_json })))
}

async fn launch_instance(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<LaunchInstanceRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let instance_repo = WorkspaceInstanceRepository::new(&state.db);
    let config_repo = WorkspaceConfigRepository::new(&state.db);
    let user_repo = UserRepository::new(&state.db);

    let config = config_repo
        .find_by_id(input.config_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find config: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to find config"})),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Config not found"})),
        ))?;

    let mount = input.mount_persistent.unwrap_or(false);
    let resolved_path = if mount {
        input.resolved_volume_host_path.as_deref()
    } else {
        None
    };

    let instance = instance_repo
        .launch(input.config_id, auth.user_id, &config.name, mount, resolved_path)
        .await
        .map_err(|e| {
            tracing::error!("Failed to launch instance (config={}, owner={}): {}", input.config_id, auth.user_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to launch instance"})),
            )
        })?;

    tracing::info!(
        "Instance '{}' launched (id={}, config={})",
        instance.name,
        instance.id,
        config.name
    );

    let docker = DockerClient::with_network(&state.settings.docker_network).await.map_err(|e| {
        tracing::error!("Failed to connect to Docker: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Docker unavailable"})),
        )
    })?;

    let container_config = ContainerConfig {
        image: config.image.clone(),
        cores: config.cores,
        memory: config.memory,
        gpu_count: config.gpu_count,
        run_config: config.run_config.clone(),
        exec_config: config.exec_config.clone(),
        volume_mappings: config.volume_mappings.clone(),
        persistent_volume: resolved_path.map(|s| s.to_string()),
        command: None,
    };

    match docker
        .create_container_from_config(&instance.name, instance.instance_number, &container_config)
        .await
    {
        Ok(container_id) => {
            instance_repo.update_container_id(instance.id, &container_id).await.ok();
            instance_repo.update_status(instance.id, "running").await.ok();

            match docker.get_container_ip(&container_id, &docker.network_name).await {
                Ok(ip) => {
                    if let Err(e) = crate::vnc_trafik::write_vnc_route(&instance.vnc_token, &ip) {
                        tracing::error!("Failed to write Traefik VNC route: {}", e);
                    }
                    state.vnc_cache.insert(&instance.vnc_token, "running");
                }
                Err(e) => tracing::error!("Failed to get container IP for Traefik route: {}", e),
            }

            tracing::info!(
                "Container started for instance '{}' (container={})",
                instance.name,
                &container_id[..12]
            );

            let mut inst = instance;
            inst.container_id = Some(container_id);
            inst.status = "running".to_string();
            let owner_username = user_repo.find_by_id(inst.owner_id).await.ok().flatten().map(|u| u.1);
            Ok(Json(serde_json::json!({ "instance": instance_to_json(&inst, Some(&config.name), owner_username.as_deref()) })))
        }
        Err(e) => {
            tracing::warn!(
                "Failed to create container for instance '{}': {} (DB record kept)",
                instance.name,
                e
            );
            instance_repo.update_status(instance.id, "error").await.ok();
            let mut inst = instance;
            inst.status = "error".to_string();
            let owner_username = user_repo.find_by_id(inst.owner_id).await.ok().flatten().map(|u| u.1);
            Ok(Json(serde_json::json!({ "instance": instance_to_json(&inst, Some(&config.name), owner_username.as_deref()), "docker_error": e })))
        }
    }
}

async fn get_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let instance_repo = WorkspaceInstanceRepository::new(&state.db);
    let config_repo = WorkspaceConfigRepository::new(&state.db);
    let user_repo = UserRepository::new(&state.db);

    let instance = instance_repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let config_name = config_repo
        .find_by_id(instance.config_id)
        .await
        .ok()
        .flatten()
        .map(|c| c.name);

    let owner_username = user_repo.find_by_id(instance.owner_id).await.ok().flatten().map(|u| u.1);

    Ok(Json(serde_json::json!({ "instance": instance_to_json(&instance, config_name.as_deref(), owner_username.as_deref()) })))
}

async fn delete_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Result<StatusCode, StatusCode> {
    let instance_repo = WorkspaceInstanceRepository::new(&state.db);

    let instance = instance_repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if let Err(e) = crate::vnc_trafik::delete_vnc_route(&instance.vnc_token) {
        tracing::error!("Failed to delete Traefik VNC route: {}", e);
    }
    state.vnc_cache.remove(&instance.vnc_token);

    if let Some(ref container_id) = instance.container_id {
        if let Ok(docker) = DockerClient::with_network(&state.settings.docker_network).await {
            let _ = docker.stop_container_by_id(container_id).await;
            match docker.remove_container_by_id(container_id).await {
                Ok(()) => tracing::info!("Container removed for instance '{}'", instance.name),
                Err(e) => tracing::warn!("Failed to remove container for '{}': {}", instance.name, e),
            }
        }
    }

    match instance_repo.delete(id).await {
        Ok(true) => Ok(StatusCode::NO_CONTENT),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn start_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let instance_repo = WorkspaceInstanceRepository::new(&state.db);
    let config_repo = WorkspaceConfigRepository::new(&state.db);

    let instance = instance_repo
        .find_by_id(id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find instance {}: {}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal error"})),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Instance not found"})),
        ))?;

    if instance.status == "running" {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "Instance is already running"})),
        ));
    }

    let docker = DockerClient::with_network(&state.settings.docker_network).await.map_err(|e| {
        tracing::error!("Failed to connect to Docker: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Docker unavailable"})),
        )
    })?;

    let new_container_id = match instance.container_id {
        Some(ref cid) => {
            match docker.inspect_container_state(cid).await {
                Ok(Some(state_str)) => {
                    if state_str.to_lowercase().contains("running") {
                        tracing::info!("Container for '{}' already running, updating DB", instance.name);
                    } else {
                        tracing::info!("Starting stopped container for '{}' (id: {})", instance.name, &cid[..12]);
                        docker.start_container_by_id(cid).await.map_err(|e| {
                            tracing::error!("Failed to start container for '{}': {}", instance.name, e);
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(serde_json::json!({"error": "Failed to start container"})),
                            )
                        })?;
                    }
                    Some(cid.clone())
                }
                _ => {
                    tracing::warn!("Container for '{}' not found, creating new one", instance.name);
                    let config = config_repo.find_by_id(instance.config_id).await.ok().flatten();
                    if let Some(config) = config {
                        let container_config = ContainerConfig {
                            image: config.image,
                            cores: config.cores,
                            memory: config.memory,
                            gpu_count: config.gpu_count,
                            run_config: config.run_config,
                            exec_config: config.exec_config,
                            volume_mappings: config.volume_mappings,
                            persistent_volume: instance.resolved_volume_host_path.clone(),
                            command: None,
                        };
                        let new_id = docker.create_container_from_config(&instance.name, instance.instance_number, &container_config).await.map_err(|e| {
                            tracing::error!("Failed to create container for '{}': {}", instance.name, e);
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(serde_json::json!({"error": "Failed to create container"})),
                            )
                        })?;
                        Some(new_id)
                    } else {
                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(serde_json::json!({"error": "Config not found for instance"})),
                        ));
                    }
                }
            }
        }
        None => {
            tracing::info!("No container for instance '{}', creating new one", instance.name);
            let config = config_repo.find_by_id(instance.config_id).await.ok().flatten();
            if let Some(config) = config {
                let container_config = ContainerConfig {
                    image: config.image,
                    cores: config.cores,
                    memory: config.memory,
                    gpu_count: config.gpu_count,
                    run_config: config.run_config,
                    exec_config: config.exec_config,
                    volume_mappings: config.volume_mappings,
                    persistent_volume: instance.resolved_volume_host_path.clone(),
                    command: None,
                };
                let new_id = docker.create_container_from_config(&instance.name, instance.instance_number, &container_config).await.map_err(|e| {
                    tracing::error!("Failed to create container for '{}': {}", instance.name, e);
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": "Failed to create container"})),
                    )
                })?;
                Some(new_id)
            } else {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Config not found for instance"})),
                ));
            }
        }
    };

    if let Some(ref cid) = new_container_id {
        instance_repo.update_container_id(instance.id, cid).await.ok();

        match docker.get_container_ip(cid, &docker.network_name).await {
            Ok(ip) => {
                if let Err(e) = crate::vnc_trafik::write_vnc_route(&instance.vnc_token, &ip) {
                    tracing::error!("Failed to write Traefik VNC route: {}", e);
                }
                state.vnc_cache.insert(&instance.vnc_token, "running");
            }
            Err(e) => tracing::error!("Failed to get container IP for Traefik route: {}", e),
        }
    }
    instance_repo.update_status(instance.id, "running").await.map_err(|e| {
        tracing::error!("Failed to update instance status: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to update status"})),
        )
    })?;

    tracing::info!("Instance '{}' started", instance.name);

    Ok(Json(serde_json::json!({
        "status": "running",
        "container_id": new_container_id
    })))
}

async fn stop_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let instance_repo = WorkspaceInstanceRepository::new(&state.db);

    let instance = instance_repo
        .find_by_id(id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find instance {}: {}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal error"})),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Instance not found"})),
        ))?;

    if instance.status == "stopped" {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "Instance is already stopped"})),
        ));
    }

    if instance.status == "paused" {
        if let Some(ref cid) = instance.container_id {
            if let Ok(docker) = DockerClient::with_network(&state.settings.docker_network).await {
                let _ = docker.unpause_container_by_id(cid).await;
            }
        }
    }

    if let Some(ref cid) = instance.container_id {
        let docker = DockerClient::with_network(&state.settings.docker_network).await.map_err(|e| {
            tracing::error!("Failed to connect to Docker: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Docker unavailable"})),
            )
        })?;

        match docker.stop_container_by_id(cid).await {
            Ok(()) => {
                tracing::info!("Container for '{}' stopped (id: {})", instance.name, &cid[..12]);
            }
            Err(e) => {
                tracing::warn!("Failed to stop container for '{}': {} (updating DB anyway)", instance.name, e);
            }
        }
    }

    if let Err(e) = crate::vnc_trafik::delete_vnc_route(&instance.vnc_token) {
        tracing::error!("Failed to delete Traefik VNC route: {}", e);
    }
    state.vnc_cache.remove(&instance.vnc_token);

    instance_repo.update_status(instance.id, "stopped").await.map_err(|e| {
        tracing::error!("Failed to update instance status: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to update status"})),
        )
    })?;

    tracing::info!("Instance '{}' stopped", instance.name);

    Ok(Json(serde_json::json!({ "status": "stopped" })))
}

async fn pause_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let instance_repo = WorkspaceInstanceRepository::new(&state.db);

    let instance = instance_repo
        .find_by_id(id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find instance {}: {}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal error"})),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Instance not found"})),
        ))?;

    if instance.status != "running" {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "Instance must be running to pause"})),
        ));
    }

    let cid = instance.container_id.as_ref().ok_or((
        StatusCode::CONFLICT,
        Json(serde_json::json!({"error": "No container attached"})),
    ))?;

    let docker = DockerClient::with_network(&state.settings.docker_network).await.map_err(|e| {
        tracing::error!("Failed to connect to Docker: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Docker unavailable"})),
        )
    })?;

    docker.pause_container_by_id(cid).await.map_err(|e| {
        tracing::error!("Failed to pause container for '{}': {}", instance.name, e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to pause container"})),
        )
    })?;

    instance_repo.update_status(instance.id, "paused").await.map_err(|e| {
        tracing::error!("Failed to update instance status: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to update status"})),
        )
    })?;

    tracing::info!("Instance '{}' paused", instance.name);

    Ok(Json(serde_json::json!({ "status": "paused" })))
}

async fn unpause_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let instance_repo = WorkspaceInstanceRepository::new(&state.db);

    let instance = instance_repo
        .find_by_id(id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find instance {}: {}", id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal error"})),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Instance not found"})),
        ))?;

    if instance.status != "paused" {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "Instance must be paused to resume"})),
        ));
    }

    let cid = instance.container_id.as_ref().ok_or((
        StatusCode::CONFLICT,
        Json(serde_json::json!({"error": "No container attached"})),
    ))?;

    let docker = DockerClient::with_network(&state.settings.docker_network).await.map_err(|e| {
        tracing::error!("Failed to connect to Docker: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Docker unavailable"})),
        )
    })?;

    docker.unpause_container_by_id(cid).await.map_err(|e| {
        tracing::error!("Failed to unpause container for '{}': {}", instance.name, e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to resume container"})),
        )
    })?;

    instance_repo.update_status(instance.id, "running").await.map_err(|e| {
        tracing::error!("Failed to update instance status: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to update status"})),
        )
    })?;

    tracing::info!("Instance '{}' unpaused", instance.name);

    Ok(Json(serde_json::json!({ "status": "running" })))
}
