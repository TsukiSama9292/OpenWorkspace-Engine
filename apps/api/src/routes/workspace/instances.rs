use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use super::super::AppState;
use crate::auth::{AuthUser, Role};
use crate::db::{UserRepository, WorkspaceTemplateRepository, WorkspaceInstance, WorkspaceInstanceRepository};
use crate::docker::{ContainerConfig, RemoteType};
use chrono::{DateTime, Utc};

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
        .route("/api/instances/{id}/heartbeat", post(heartbeat_instance))
}

fn resolve_runtime(container_runtime: &str, settings_runtime: &str) -> String {
    if container_runtime.is_empty() {
        settings_runtime.to_string()
    } else {
        container_runtime.to_string()
    }
}

fn auto_sleep_deadline(inst: &WorkspaceInstance, max_run_seconds: Option<i64>) -> Option<DateTime<Utc>> {
    if inst.status != "running" {
        return None;
    }
    match (inst.started_at, max_run_seconds) {
        (Some(started_at), Some(max_run_seconds)) => {
            Some(started_at + chrono::Duration::seconds(max_run_seconds))
        }
        _ => None,
    }
}

fn keep_time_deadline(inst: &WorkspaceInstance, keep_time_seconds: Option<i64>) -> Option<DateTime<Utc>> {
    if inst.status != "running" {
        return None;
    }
    match (inst.last_seen_at, keep_time_seconds) {
        (Some(last_seen_at), Some(keep_time_seconds)) => {
            Some(last_seen_at + chrono::Duration::seconds(keep_time_seconds))
        }
        _ => None,
    }
}

fn instance_to_json(
    inst: &WorkspaceInstance,
    template_name: Option<&str>,
    remote_type: Option<&str>,
    owner_username: Option<&str>,
    owner_role: Option<&str>,
    max_run_seconds: Option<i64>,
    timeout_action: Option<&str>,
    keep_time_seconds: Option<i64>,
    keep_time_action: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({
        "id": inst.id,
        "template_id": inst.template_id,
        "name": inst.name,
        "instance_number": inst.instance_number,
        "owner_id": inst.owner_id,
        "owner_username": owner_username.unwrap_or(""),
        "owner_role": owner_role.unwrap_or("user"),
        "container_id": inst.container_id,
        "status": inst.status,
        "access_token": inst.access_token,
        "access_password": inst.access_password,
        "mount_persistent": inst.mount_persistent,
        "resolved_volume_host_path": inst.resolved_volume_host_path,
        "started_at": inst.started_at,
        "template_name": template_name,
        "remote_type": remote_type,
        "auto_sleeps_at": auto_sleep_deadline(inst, max_run_seconds),
        "timeout_action": timeout_action,
        "keep_time_deadline": keep_time_deadline(inst, keep_time_seconds),
        "keep_time_seconds": keep_time_seconds,
        "keep_time_action": if keep_time_seconds.is_some() { keep_time_action } else { None },
        "created_at": inst.created_at,
        "updated_at": inst.updated_at,
    })
}

async fn can_manage_instance(
    state: &AppState,
    auth: &AuthUser,
    instance: &WorkspaceInstance,
) -> Result<bool, StatusCode> {
    if instance.owner_id == auth.user_id {
        return Ok(true);
    }
    let user_repo = UserRepository::new(&state.db);
    let owner = user_repo
        .find_by_id(instance.owner_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    let owner_role = Role::from_str(&owner.3).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(auth.role.can_manage_instance(&owner_role))
}

#[derive(Deserialize)]
struct LaunchInstanceRequest {
    template_id: Uuid,
    mount_persistent: Option<bool>,
    resolved_volume_host_path: Option<String>,
}

async fn list_instances(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let instance_repo = WorkspaceInstanceRepository::new(&state.db);
    let template_repo = WorkspaceTemplateRepository::new(&state.db);

    let instances = if auth.role.can_view_all_instances() {
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

    let mut template_names = std::collections::HashMap::new();
    let mut template_remote_types = std::collections::HashMap::new();
    let mut template_max_run_seconds = std::collections::HashMap::new();
    let mut template_timeout_actions = std::collections::HashMap::new();
    let mut template_keep_time_seconds = std::collections::HashMap::new();
    let mut template_keep_time_actions = std::collections::HashMap::new();
    for inst in &instances {
        if !template_names.contains_key(&inst.template_id) {
            if let Ok(Some(template)) = template_repo.find_by_id(inst.template_id).await {
                template_names.insert(inst.template_id, template.name);
                template_remote_types.insert(inst.template_id, template.remote_type);
                template_max_run_seconds.insert(inst.template_id, template.max_run_seconds);
                template_timeout_actions.insert(inst.template_id, template.timeout_action);
                template_keep_time_seconds.insert(inst.template_id, template.keep_time_seconds);
                template_keep_time_actions.insert(inst.template_id, template.keep_time_action);
            }
        }
    }

    let user_repo = UserRepository::new(&state.db);
    let mut owner_usernames = std::collections::HashMap::new();
    let mut owner_roles = std::collections::HashMap::new();
    for inst in &instances {
        if !owner_usernames.contains_key(&inst.owner_id) {
            if let Ok(Some(user)) = user_repo.find_by_id(inst.owner_id).await {
                owner_usernames.insert(inst.owner_id, user.1);
                owner_roles.insert(inst.owner_id, user.3);
            }
        }
    }

    let instances_json: Vec<_> = instances
        .iter()
        .map(|inst| {
            let template_name = template_names.get(&inst.template_id).map(|s| s.as_str());
            let remote_type = template_remote_types.get(&inst.template_id).map(|s| s.as_str());
            let max_run_seconds = template_max_run_seconds.get(&inst.template_id).copied().flatten();
            let timeout_action = template_timeout_actions.get(&inst.template_id).map(|s| s.as_str());
            let keep_time_seconds = template_keep_time_seconds.get(&inst.template_id).copied().flatten();
            let keep_time_action = template_keep_time_actions.get(&inst.template_id).map(|s| s.as_str());
            let owner_username = owner_usernames.get(&inst.owner_id).map(|s| s.as_str());
            let owner_role = owner_roles.get(&inst.owner_id).map(|s| s.as_str());
            instance_to_json(inst, template_name, remote_type, owner_username, owner_role, max_run_seconds, timeout_action, keep_time_seconds, keep_time_action)
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
    let template_repo = WorkspaceTemplateRepository::new(&state.db);
    let user_repo = UserRepository::new(&state.db);

    let template = template_repo
        .find_by_id(input.template_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find template: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to find template"})),
            )
        })?
        .ok_or((
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Template not found"})),
        ))?;

    let mount = input.mount_persistent.unwrap_or(false);
    let resolved_path = if mount {
        input.resolved_volume_host_path.as_deref()
    } else {
        None
    };

    let instance = instance_repo
        .launch(input.template_id, auth.user_id, &template.name, mount, resolved_path)
        .await
        .map_err(|e| {
            tracing::error!("Failed to launch instance (template={}, owner={}): {}", input.template_id, auth.user_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to launch instance"})),
            )
        })?;

    tracing::info!(
        "Instance '{}' launched (id={}, template={})",
        instance.name,
        instance.id,
        template.name
    );

    let remote_type: RemoteType = template.remote_type.parse().unwrap_or(RemoteType::KasmVnc);

    let container_config = ContainerConfig {
        image: template.image.clone(),
        cores: template.cores,
        memory: template.memory,
        gpu_count: template.gpu_count,
        remote_type: remote_type.clone(),
        run_config: template.run_config.clone(),
        exec_config: template.exec_config.clone(),
        volume_mappings: template.volume_mappings.clone(),
        persistent_volume: resolved_path.map(|s| s.to_string()),
        command: None,
        runtime: Some(resolve_runtime(&template.container_runtime, &state.settings.container_runtime)),
        network_bandwidth_up_mbps: template.network_bandwidth_up_mbps,
        network_bandwidth_down_mbps: template.network_bandwidth_down_mbps,
    };

    match state.docker
        .create_container_from_template(&instance.name, instance.instance_number, &container_config, &instance.access_password, &instance.access_token)
        .await
    {
        Ok(container_id) => {
            instance_repo.update_container_id(instance.id, &container_id).await.ok();
            instance_repo.update_status(instance.id, "starting").await.ok();

            match state.docker.get_container_ip(&container_id, &state.docker.network_name()).await {
                Ok(ip) => {
                    if let Err(e) = crate::route_writer::write_route(&remote_type, &instance.access_token, &ip, &instance.access_password) {
                        tracing::error!("Failed to write Traefik VNC route: {}", e);
                    }
                    state.vnc_cache.insert(&instance.access_token, "starting");
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
            inst.status = "starting".to_string();
            let owner = user_repo.find_by_id(inst.owner_id).await.ok().flatten();
            let owner_username = owner.as_ref().map(|u| u.1.as_str());
            let owner_role = owner.as_ref().map(|u| u.3.as_str());
            Ok(Json(serde_json::json!({ "instance": instance_to_json(&inst, Some(&template.name), Some(&template.remote_type), owner_username, owner_role, template.max_run_seconds, Some(&template.timeout_action), template.keep_time_seconds, Some(&template.keep_time_action)) })))
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
            let owner = user_repo.find_by_id(inst.owner_id).await.ok().flatten();
            let owner_username = owner.as_ref().map(|u| u.1.as_str());
            let owner_role = owner.as_ref().map(|u| u.3.as_str());
            Ok(Json(serde_json::json!({ "instance": instance_to_json(&inst, Some(&template.name), Some(&template.remote_type), owner_username, owner_role, template.max_run_seconds, Some(&template.timeout_action), template.keep_time_seconds, Some(&template.keep_time_action)), "docker_error": e })))
        }
    }
}

async fn get_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let instance_repo = WorkspaceInstanceRepository::new(&state.db);
    let template_repo = WorkspaceTemplateRepository::new(&state.db);
    let user_repo = UserRepository::new(&state.db);

    let instance = instance_repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let template = template_repo
        .find_by_id(instance.template_id)
        .await
        .ok()
        .flatten();

    let template_name = template.as_ref().map(|t| t.name.clone());
    let remote_type = template.as_ref().map(|t| t.remote_type.clone());
    let max_run_seconds = template.as_ref().map(|t| t.max_run_seconds).flatten();
    let timeout_action = template.as_ref().map(|t| t.timeout_action.clone());
    let keep_time_seconds = template.as_ref().map(|t| t.keep_time_seconds).flatten();
    let keep_time_action = template.as_ref().map(|t| t.keep_time_action.clone());

    let owner = user_repo.find_by_id(instance.owner_id).await.ok().flatten();
    let owner_username = owner.as_ref().map(|u| u.1.as_str());
    let owner_role = owner.as_ref().map(|u| u.3.as_str());

    Ok(Json(serde_json::json!({ "instance": instance_to_json(&instance, template_name.as_deref(), remote_type.as_deref(), owner_username, owner_role, max_run_seconds, timeout_action.as_deref(), keep_time_seconds, keep_time_action.as_deref()) })))
}

async fn delete_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
) -> Result<StatusCode, StatusCode> {
    let instance_repo = WorkspaceInstanceRepository::new(&state.db);

    let instance = instance_repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    if !can_manage_instance(&state, &auth, &instance).await? {
        return Err(StatusCode::FORBIDDEN);
    }

    if let Err(e) = crate::route_writer::delete_route(&instance.access_token) {
        tracing::error!("Failed to delete Traefik VNC route: {}", e);
    }
    state.vnc_cache.remove(&instance.access_token);

    if let Some(ref container_id) = instance.container_id {
        crate::docker::stop_and_remove_container(&*state.docker, container_id, &instance.name)
            .await;
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
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let instance_repo = WorkspaceInstanceRepository::new(&state.db);
    let template_repo = WorkspaceTemplateRepository::new(&state.db);

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

    if !can_manage_instance(&state, &auth, &instance).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal error"})),
        )
    })? {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Forbidden"})),
        ));
    }

    if instance.status == "running" {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "Instance is already running"})),
        ));
    }

    let template = template_repo.find_by_id(instance.template_id).await.ok().flatten();
    let remote_type: RemoteType = template.as_ref().map(|t| t.remote_type.parse().ok()).flatten().unwrap_or(RemoteType::KasmVnc);

    let new_container_id = match instance.container_id {
        Some(ref cid) => {
            match state.docker.inspect_container_state(cid).await {
                Ok(Some(state_str)) => {
                    if state_str.to_lowercase().contains("running") {
                        tracing::info!("Container for '{}' already running, updating DB", instance.name);
                    } else {
                        tracing::info!("Starting stopped container for '{}' (id: {})", instance.name, &cid[..12]);
                        state.docker.start_container_by_id(cid).await.map_err(|e| {
                            tracing::error!("Failed to start container for '{}': {}", instance.name, e);
                            (
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(serde_json::json!({"error": "Failed to start container"})),
                            )
                        })?;

                        // Docker recreates the veth pair on start, destroying any
                        // prior qdisc — re-apply the template's bandwidth limit.
                        if let Some(t) = template.as_ref() {
                            if t.network_bandwidth_up_mbps > 0 || t.network_bandwidth_down_mbps > 0 {
                                if let Err(e) = state.docker
                                    .apply_bandwidth_limit(cid, t.network_bandwidth_up_mbps, t.network_bandwidth_down_mbps)
                                    .await
                                {
                                    tracing::error!(
                                        "Failed to apply bandwidth limit for '{}': {} (container keeps running without limit) — TODO: notify admin",
                                        instance.name, e
                                    );
                                }
                            }
                        }
                    }
                    Some(cid.clone())
                }
                _ => {
                    tracing::warn!("Container for '{}' not found, creating new one", instance.name);
                    let template = template_repo.find_by_id(instance.template_id).await.ok().flatten();
                    if let Some(template) = template {
                        let container_config = ContainerConfig {
                            image: template.image,
                            cores: template.cores,
                            memory: template.memory,
                            gpu_count: template.gpu_count,
                            remote_type: remote_type.clone(),
                            run_config: template.run_config,
                            exec_config: template.exec_config,
                            volume_mappings: template.volume_mappings,
                            persistent_volume: instance.resolved_volume_host_path.clone(),
                            command: None,
                            runtime: Some(resolve_runtime(&template.container_runtime, &state.settings.container_runtime)),
                            network_bandwidth_up_mbps: template.network_bandwidth_up_mbps,
                            network_bandwidth_down_mbps: template.network_bandwidth_down_mbps,
                        };
                        let new_id = state.docker.create_container_from_template(&instance.name, instance.instance_number, &container_config, &instance.access_password, &instance.access_token).await.map_err(|e| {
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
                            Json(serde_json::json!({"error": "Template not found for instance"})),
                        ));
                    }
                }
            }
        }
        None => {
            tracing::info!("No container for instance '{}', creating new one", instance.name);
            let template = template_repo.find_by_id(instance.template_id).await.ok().flatten();
            if let Some(template) = template {
                let container_config = ContainerConfig {
                    image: template.image,
                    cores: template.cores,
                    memory: template.memory,
                    gpu_count: template.gpu_count,
                    remote_type: remote_type.clone(),
                    run_config: template.run_config,
                    exec_config: template.exec_config,
                    volume_mappings: template.volume_mappings,
                    persistent_volume: instance.resolved_volume_host_path.clone(),
                    command: None,
                    runtime: Some(resolve_runtime(&template.container_runtime, &state.settings.container_runtime)),
                    network_bandwidth_up_mbps: template.network_bandwidth_up_mbps,
                    network_bandwidth_down_mbps: template.network_bandwidth_down_mbps,
                };
                let new_id = state.docker.create_container_from_template(&instance.name, instance.instance_number, &container_config, &instance.access_password, &instance.access_token).await.map_err(|e| {
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
                    Json(serde_json::json!({"error": "Template not found for instance"})),
                ));
            }
        }
    };

    if let Some(ref cid) = new_container_id {
        instance_repo.update_container_id(instance.id, cid).await.ok();

        match state.docker.get_container_ip(cid, &state.docker.network_name()).await {
            Ok(ip) => {
                if let Err(e) = crate::route_writer::write_route(&remote_type, &instance.access_token, &ip, &instance.access_password) {
                    tracing::error!("Failed to write Traefik route: {}", e);
                }
                state.vnc_cache.insert(&instance.access_token, "starting");
            }
            Err(e) => tracing::error!("Failed to get container IP for Traefik route: {}", e),
        }
    }
    instance_repo.update_status(instance.id, "starting").await.map_err(|e| {
        tracing::error!("Failed to update instance status: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to update status"})),
        )
    })?;

    tracing::info!("Instance '{}' started", instance.name);

    let container_id_str = new_container_id.as_deref();
    Ok(Json(serde_json::json!({
        "status": "starting",
        "container_id": container_id_str
    })))
}

async fn stop_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
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

    if !can_manage_instance(&state, &auth, &instance).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal error"})),
        )
    })? {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Forbidden"})),
        ));
    }

    if instance.status == "stopped" {
        return Err((
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": "Instance is already stopped"})),
        ));
    }

    if instance.status == "paused" {
        if let Some(ref cid) = instance.container_id {
            let _ = state.docker.unpause_container_by_id(cid).await;
        }
    }

    if let Some(ref cid) = instance.container_id {
        match state.docker.stop_container_by_id(cid).await {
            Ok(()) => {
                tracing::info!("Container for '{}' stopped (id: {})", instance.name, &cid[..12]);
            }
            Err(e) => {
                tracing::warn!("Failed to stop container for '{}': {} (updating DB anyway)", instance.name, e);
            }
        }
    }

    if let Err(e) = crate::route_writer::delete_route(&instance.access_token) {
        tracing::error!("Failed to delete Traefik VNC route: {}", e);
    }
    state.vnc_cache.remove(&instance.access_token);

    instance_repo.update_status(instance.id, "stopped").await.map_err(|e| {
        tracing::error!("Failed to update instance status: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to update status"})),
        )
    })?;
    instance_repo.update_started_at(instance.id, None).await.ok();
    instance_repo.update_last_seen_at(instance.id, None).await.ok();

    tracing::info!("Instance '{}' stopped", instance.name);

    Ok(Json(serde_json::json!({ "status": "stopped" })))
}

async fn pause_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
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

    if !can_manage_instance(&state, &auth, &instance).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal error"})),
        )
    })? {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Forbidden"})),
        ));
    }

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

    state.docker.pause_container_by_id(cid).await.map_err(|e| {
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
    instance_repo.update_started_at(instance.id, None).await.ok();
    instance_repo.update_last_seen_at(instance.id, None).await.ok();

    tracing::info!("Instance '{}' paused", instance.name);

    Ok(Json(serde_json::json!({ "status": "paused" })))
}

async fn unpause_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
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

    if !can_manage_instance(&state, &auth, &instance).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal error"})),
        )
    })? {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Forbidden"})),
        ));
    }

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

    state.docker.unpause_container_by_id(cid).await.map_err(|e| {
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
    instance_repo.update_started_at(instance.id, Some(Utc::now())).await.map_err(|e| {
        tracing::error!("Failed to update started_at: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to update started_at"})),
        )
    })?;
    instance_repo.update_last_seen_at(instance.id, Some(Utc::now())).await.map_err(|e| {
        tracing::error!("Failed to update last_seen_at: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to update last_seen_at"})),
        )
    })?;

    tracing::info!("Instance '{}' unpaused", instance.name);

    Ok(Json(serde_json::json!({ "status": "running" })))
}

async fn heartbeat_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
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

    if !can_manage_instance(&state, &auth, &instance).await.map_err(|_| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal error"})),
        )
    })? {
        return Err((
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({"error": "Forbidden"})),
        ));
    }

    instance_repo.update_last_seen_at(id, Some(Utc::now())).await.map_err(|e| {
        tracing::error!("Failed to update last_seen_at for instance {}: {}", id, e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to update last_seen_at"})),
        )
    })?;

    Ok(Json(serde_json::json!({ "status": "ok" })))
}
