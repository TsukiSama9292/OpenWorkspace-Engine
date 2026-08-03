use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use std::collections::BTreeSet;
use std::net::Ipv4Addr;
use uuid::Uuid;

use super::super::AppState;
use crate::auth::{AuthUser, Role};
use crate::db::{UserRepository, WorkspaceTemplate, WorkspaceTemplateRepository, WorkspaceInstance, WorkspaceInstanceRepository};
use crate::docker::{ContainerConfig, RemoteType};
use crate::persistent_volume::{persistent_volume_name, resolve_persistent_host_path, resolve_persistent_host_path_opt};
use crate::quota::{QuotaOverride, QuotaScope, QuotaViolation};
use crate::quota_activation::{ActivationError, ActivationKind, ActivationRequest, LaunchPayload};
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
        "host_port": inst.host_port,
        "network_name": crate::instance_net::network_name(&inst.id.to_string()),
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
    let owner_role = Role::from_str(&owner.role).ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(auth.role.can_manage_instance(&owner_role))
}

/// How a launch request wants the Instance's persistent storage handled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PersistenceMode {
    /// Mount a persistent volume and reuse its data across launches.
    UsePersistent,
    /// Do not mount a persistent volume.
    NoPersistent,
    /// Wipe any existing data, then mount a fresh persistent volume.
    ResetPersistent,
}

#[derive(Deserialize)]
struct LaunchInstanceRequest {
    template_id: Uuid,
    persistence: Option<PersistenceMode>,
    mount_persistent: Option<bool>,
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
                owner_usernames.insert(inst.owner_id, user.username);
                owner_roles.insert(inst.owner_id, user.role);
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

/// Build the JSON body returned when a launch fails *after* the DB record was
/// created (volume prep or container creation): the Instance is kept and
/// reported in `error` state (spec §3), alongside a `docker_error` field —
/// mirroring the pre-existing container-creation failure path.
async fn launch_error_response(
    state: &AppState,
    mut instance: WorkspaceInstance,
    template: &WorkspaceTemplate,
    docker_error: &str,
) -> serde_json::Value {
    instance.status = "error".to_string();
    let user_repo = UserRepository::new(&state.db);
    let owner = user_repo.find_by_id(instance.owner_id).await.ok().flatten();
    let owner_username = owner.as_ref().map(|u| u.username.as_str());
    let owner_role = owner.as_ref().map(|u| u.role.as_str());
    serde_json::json!({
        "instance": instance_to_json(
            &instance,
            Some(&template.name),
            Some(&template.remote_type),
            owner_username,
            owner_role,
            template.max_run_seconds,
            Some(&template.timeout_action),
            template.keep_time_seconds,
            Some(&template.keep_time_action),
        ),
        "docker_error": docker_error,
    })
}

/// The structured `409` body for a quota rejection (spec §10): a short
/// human-readable message plus the machine-readable `quota` object. The
/// `quota` field is present only on quota rejections.
fn quota_rejection_json(violation: &QuotaViolation) -> serde_json::Value {
    use QuotaScope::*;
    let message = match violation.scope {
        UserInstance => format!(
            "Per-user instance limit reached (active: {}, limit: {})",
            violation.current, violation.limit
        ),
        HostInstance => format!(
            "Host instance limit reached (active: {}, limit: {})",
            violation.current, violation.limit
        ),
        UserCpu => format!(
            "Per-user CPU quota exceeded (active: {}, requested: {}, limit: {})",
            violation.current, violation.requested, violation.limit
        ),
        UserRam => format!(
            "Per-user RAM quota exceeded (active: {}, requested: {}, limit: {})",
            violation.current, violation.requested, violation.limit
        ),
        HostDedicatedCpu => format!(
            "Host dedicated CPU pool exhausted (active: {}, requested: {}, limit: {})",
            violation.current, violation.requested, violation.limit
        ),
        HostDedicatedRam => format!(
            "Host dedicated RAM pool exhausted (active: {}, requested: {}, limit: {})",
            violation.current, violation.requested, violation.limit
        ),
        HostSharedCpu => format!(
            "Host shared CPU fuse exceeded (active: {}, requested: {}, limit: {})",
            violation.current, violation.requested, violation.limit
        ),
        HostSharedRam => format!(
            "Host shared RAM fuse exceeded (active: {}, requested: {}, limit: {})",
            violation.current, violation.requested, violation.limit
        ),
    };
    serde_json::json!({
        "error": message,
        "quota": violation,
    })
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

    let mode = input.persistence.unwrap_or_else(|| {
        if input.mount_persistent.unwrap_or(false) {
            PersistenceMode::UsePersistent
        } else {
            PersistenceMode::NoPersistent
        }
    });

    let wants_persistence = matches!(mode, PersistenceMode::UsePersistent | PersistenceMode::ResetPersistent);
    let resolved_path = if wants_persistence {
        match resolve_persistent_host_path_opt(
            template.persistent_storage_path.as_deref(),
            &template.name,
            &auth.user_id.to_string(),
        ) {
            Ok(path) => path,
            Err(e) => {
                tracing::warn!(
                    "Persistent launch rejected for template '{}' (owner={}): invalid persistent_storage_path: {:?}",
                    template.name,
                    auth.user_id,
                    e
                );
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": format!("Template has an invalid persistent storage path: {:?}", e)
                    })),
                ));
            }
        }
    } else {
        None
    };

    // Run the quota pre-flight and reserve the instance atomically (spec
    // Decision 1/3): the persistent-uniqueness rule and the reservation commit
    // in one transaction serialized by the user-row lock, so a rejection
    // leaves no DB row behind. The auto-name is derived together with
    // `instance_number` inside the helper.
    let mount = resolved_path.is_some();
    let user = user_repo
        .find_by_id(auth.user_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find user {}: {}", auth.user_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to launch instance"})),
            )
        })?
        .ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Failed to launch instance"})),
        ))?;
    let activation_request = ActivationRequest {
        kind: ActivationKind::Launch(LaunchPayload {
            mount_persistent: mount,
            resolved_volume_host_path: resolved_path.clone(),
        }),
        template: &template,
        user_id: auth.user_id,
        role: auth.role.clone(),
        user_overrides: QuotaOverride {
            instance_limit: user.instance_limit,
            max_cpu_cores: user.max_cpu_cores,
            max_ram_bytes: user.max_ram_bytes,
        },
    };
    let reservation = match crate::quota_activation::activate(&state.db, &activation_request).await {
        Ok(reservation) => reservation,
        Err(ActivationError::Quota(violation)) => {
            return Err((StatusCode::CONFLICT, Json(quota_rejection_json(&violation))));
        }
        Err(ActivationError::Conflict(message)) => {
            return Err((StatusCode::CONFLICT, Json(serde_json::json!({ "error": message }))));
        }
        Err(ActivationError::Db(e)) => {
            tracing::error!(
                "Failed to activate launch (template={}, owner={}): {}",
                input.template_id,
                auth.user_id,
                e
            );
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Failed to launch instance"})),
            ));
        }
    };
    let instance = reservation.instance;
    let replace_broken = reservation.replaced_broken;

    // Prepare the persistent volume only after the DB record exists, so a
    // helper/volume failure leaves a visible `error` Instance (DB record kept,
    // spec §3) instead of silently failing the launch with no trace. A broken
    // (error) record being replaced is wiped first, then re-prepared.
    if let Some(host_path) = resolved_path.as_deref() {
        let volume_name = persistent_volume_name(host_path);
        let wipe = replace_broken || mode == PersistenceMode::ResetPersistent;
        if wipe {
            if let Err(e) = state.docker
                .remove_persistent_volume(host_path, &volume_name)
                .await
            {
                tracing::warn!(
                    "Failed to remove persistent volume before reset (template={}, owner={}): {}",
                    input.template_id,
                    auth.user_id,
                    e
                );
                instance_repo.update_status(instance.id, "error").await.ok();
                return Ok(Json(launch_error_response(&state, instance, &template, &e).await));
            }
        }
        if let Err(e) = state.docker
            .prepare_persistent_volume(host_path, &volume_name)
            .await
        {
            tracing::warn!(
                "Failed to prepare persistent volume for instance (template={}, owner={}): {}",
                input.template_id,
                auth.user_id,
                e
            );
            instance_repo.update_status(instance.id, "error").await.ok();
            return Ok(Json(launch_error_response(&state, instance, &template, &e).await));
        }
    }

    tracing::info!(
        "Instance '{}' launched (id={}, template={})",
        instance.name,
        instance.id,
        template.name
    );

    let remote_type: RemoteType = template.remote_type.parse().unwrap_or(RemoteType::KasmVnc);

    let host_gateway_ip = state.settings.host_gateway_ip.clone();
    let mut used_ports: BTreeSet<u16> = collect_used_host_ports(&instance_repo).await;
    let mut host_port = match crate::host_port::allocate_host_port(
        &used_ports,
        state.settings.host_port_start,
        state.settings.host_port_end,
        &host_gateway_ip,
    ) {
        Some(port) => port,
        None => {
            let msg = "Host port pool exhausted".to_string();
            instance_repo.update_status(instance.id, "error").await.ok();
            return Ok(Json(launch_error_response(&state, instance, &template, &msg).await));
        }
    };

    // Ensure the instance's dedicated `/30` network before the container create
    // (spec §5: allocate host port → ensure instance network → create container).
    // The port-conflict retry loop below reuses this same already-created
    // network — only the host port changes between retries, so no subnet is
    // re-scanned or re-allocated.
    let network = match ensure_instance_network(&state, &instance).await {
        Ok(network) => network,
        Err(msg) => {
            instance_repo.update_status(instance.id, "error").await.ok();
            return Ok(Json(launch_error_response(&state, instance, &template, &msg).await));
        }
    };

    let mut container_config = ContainerConfig {
        image: template.image.clone(),
        cores: template.cores,
        memory: template.memory,
        gpu_count: template.gpu_count,
        remote_type: remote_type.clone(),
        run_config: template.run_config.clone(),
        exec_config: template.exec_config.clone(),
        volume_mappings: template.volume_mappings.clone(),
        persistent_volume_name: resolved_path.as_deref().map(persistent_volume_name),
        command: None,
        runtime: Some(resolve_runtime(&template.container_runtime, &state.settings.container_runtime)),
        network_bandwidth_up_mbps: template.network_bandwidth_up_mbps,
        network_bandwidth_down_mbps: template.network_bandwidth_down_mbps,
        host_port: Some(host_port),
        host_gateway_ip: Some(host_gateway_ip.clone()),
        docker_in_instance: template.docker_in_instance,
        network_name: Some(network.clone()),
        instance_dns: Some(state.settings.instance_dns.clone()),
    };

    // Bounded retry on the (rare) race where another launch binds our probe-free
    // port between allocation and the container bind: re-allocate skipping the
    // lost port and retry up to 5 times. Each retry scans the pool *circularly*
    // from a token-derived offset, so concurrent launches don't all re-pick the
    // same lowest free port and re-collide.
    let base_from = host_port_spread_base(
        state.settings.host_port_start,
        state.settings.host_port_end,
        &instance.access_token,
    );
    let mut create_result = state.docker
        .create_container_from_template(&instance.name, instance.instance_number, &container_config, &instance.access_password, &instance.access_token)
        .await;
    let mut retries = 0;
    while retries < 5 {
        match &create_result {
            Err(e) if crate::host_port::is_port_conflict(e) => {
                retries += 1;
                used_ports.insert(host_port);
                let from = base_from.wrapping_add(retries as u16);
                match crate::host_port::allocate_host_port_from(
                    &used_ports,
                    state.settings.host_port_start,
                    state.settings.host_port_end,
                    &host_gateway_ip,
                    from,
                ) {
                    Some(next) => {
                        host_port = next;
                        container_config.host_port = Some(next);
                        create_result = state.docker
                            .create_container_from_template(&instance.name, instance.instance_number, &container_config, &instance.access_password, &instance.access_token)
                            .await;
                    }
                    None => break,
                }
            }
            _ => break,
        }
    }

    match create_result {
        Ok(container_id) => {
            instance_repo.update_container_id(instance.id, &container_id).await.ok();
            instance_repo.update_host_port(instance.id, Some(host_port as i32)).await.map_err(|e| {
                tracing::error!("Failed to commit host port {} for '{}': {}", host_port, instance.name, e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Failed to allocate host port"})),
                )
            })?;
            instance_repo.update_status(instance.id, "starting").await.ok();

            if let Err(e) = crate::route_writer::write_route(&remote_type, &instance.access_token, host_port, &instance.access_password) {
                tracing::error!("Failed to write Traefik VNC route: {}", e);
            }
            state.vnc_cache.insert(&instance.access_token, "starting");

            tracing::info!(
                "Container started for instance '{}' (container={})",
                instance.name,
                &container_id[..12]
            );

            let mut inst = instance;
            inst.container_id = Some(container_id);
            inst.status = "starting".to_string();
            let owner = user_repo.find_by_id(inst.owner_id).await.ok().flatten();
            let owner_username = owner.as_ref().map(|u| u.username.as_str());
            let owner_role = owner.as_ref().map(|u| u.role.as_str());
            Ok(Json(serde_json::json!({ "instance": instance_to_json(&inst, Some(&template.name), Some(&template.remote_type), owner_username, owner_role, template.max_run_seconds, Some(&template.timeout_action), template.keep_time_seconds, Some(&template.keep_time_action)) })))
        }
        Err(e) => {
            tracing::warn!(
                "Failed to create container for instance '{}': {} (DB record kept)",
                instance.name,
                e
            );
            instance_repo.update_status(instance.id, "error").await.ok();
            Ok(Json(launch_error_response(&state, instance, &template, &e).await))
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
    let owner_username = owner.as_ref().map(|u| u.username.as_str());
    let owner_role = owner.as_ref().map(|u| u.role.as_str());

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

    // Remove the instance's dedicated network after the container is gone
    // (Docker refuses to remove a network with attached containers). The seam
    // treats a missing network as idempotent success, so a double-delete or a
    // crash-cleaned network is not an error; any real failure is logged and
    // does not block the deletion.
    let network_name = crate::instance_net::network_name(&instance.id.to_string());
    if let Err(e) = state.docker.remove_network(&network_name).await {
        tracing::warn!(
            "Failed to remove network '{}' for instance '{}': {}",
            network_name,
            instance.name,
            e
        );
    }

    // Persistent data (host dir + Volume declaration) is deliberately kept on
    // delete: "remove" only destroys the container and DB record so the data
    // can be reused by a later launch. Only `reset_persistent` wipes it.
    if instance.mount_persistent {
        tracing::info!(
            "Instance '{}' deleted; persistent data kept for reuse",
            instance.name
        );
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
    let user_repo = UserRepository::new(&state.db);

    let mut instance = instance_repo
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

    // Port the route was last written with. On a plain restart the port is
    // unchanged, the route still exists, and we skip rewriting it (no churn).
    let persisted_host_port = instance.host_port.map(|p| p as u16);

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

    // Re-run the quota pre-flight: a restart re-consumes the quota the
    // instance released while `stopped` (spec Decision 1), so it is gated
    // exactly like a launch. A rejection returns a structured `409` and leaves
    // the instance `stopped`.
    let template = template_repo
        .find_by_id(instance.template_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find template {}: {}", instance.template_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal error"})),
            )
        })?
        .ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Template not found for instance"})),
        ))?;
    let owner = user_repo
        .find_by_id(instance.owner_id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find owner {}: {}", instance.owner_id, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Internal error"})),
            )
        })?
        .ok_or((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": "Internal error"})),
        ))?;
    let owner_role = crate::auth::Role::from_str(&owner.role).ok_or((
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": "Internal error"})),
    ))?;
    let activation_request = ActivationRequest {
        kind: ActivationKind::Restart { instance_id: instance.id },
        template: &template,
        // The restarted instance consumes quota from its owner, not from the
        // acting user (an Admin/Manager may be managing someone else's).
        user_id: instance.owner_id,
        role: owner_role,
        user_overrides: QuotaOverride {
            instance_limit: owner.instance_limit,
            max_cpu_cores: owner.max_cpu_cores,
            max_ram_bytes: owner.max_ram_bytes,
        },
    };
    if let Err(e) = crate::quota_activation::activate(&state.db, &activation_request).await {
        return match e {
            ActivationError::Quota(violation) => {
                Err((StatusCode::CONFLICT, Json(quota_rejection_json(&violation))))
            }
            ActivationError::Conflict(message) => {
                Err((StatusCode::CONFLICT, Json(serde_json::json!({ "error": message }))))
            }
            ActivationError::Db(e) => {
                tracing::error!("Failed to activate restart (instance={}): {}", id, e);
                Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Internal error"})),
                ))
            }
        };
    }

    let template = Some(template);
    let remote_type: RemoteType = template.as_ref().map(|t| t.remote_type.parse().ok()).flatten().unwrap_or(RemoteType::KasmVnc);

    // Migration backfill: a legacy `mount_persistent = true` instance may have
    // no stored host path. Resolve it from the template now and persist it so
    // the volume can be ensured and mounted.
    if instance.mount_persistent && instance.resolved_volume_host_path.is_none() {
        if let Some(t) = template.as_ref() {
            match t.persistent_storage_path.as_deref() {
                Some(root) => match resolve_persistent_host_path(root, &t.name, &auth.user_id.to_string()) {
                    Ok(path) => {
                        tracing::info!(
                            "Backfilling resolved persistent host path for instance '{}'",
                            instance.name
                        );
                        instance_repo
                            .update_resolved_volume_host_path(instance.id, Some(&path))
                            .await
                            .ok();
                        instance.resolved_volume_host_path = Some(path);
                    }
                    Err(e) => tracing::warn!(
                        "Could not backfill resolved persistent path for instance '{}': {:?}",
                        instance.name,
                        e
                    ),
                },
                None => tracing::warn!(
                    "Instance '{}' is persistent but template '{}' has no persistent_storage_path",
                    instance.name,
                    t.name
                ),
            }
        }
    }
    let resolved_host_path = instance.resolved_volume_host_path.as_deref();

    // Migration backfill: a legacy instance created before the host-port pool
    // has no allocation. Allocate one now and persist it so the route writer
    // and container bind share the same port.
    let host_port = match instance.host_port {
        Some(p) => p as u16,
        None => {
            let used: BTreeSet<u16> = collect_used_host_ports(&instance_repo).await;
            match crate::host_port::allocate_host_port(
                &used,
                state.settings.host_port_start,
                state.settings.host_port_end,
                &state.settings.host_gateway_ip,
            ) {
                Some(p) => {
                    if let Err(e) = instance_repo.update_host_port(instance.id, Some(p as i32)).await {
                        tracing::error!("Failed to commit host port {} for '{}': {}", p, instance.name, e);
                        instance_repo.update_status(instance.id, "stopped").await.ok();
                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(serde_json::json!({"error": "Failed to allocate host port"})),
                        ));
                    }
                    p
                }
                None => {
                    instance_repo.update_status(instance.id, "stopped").await.ok();
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(serde_json::json!({"error": "Host port pool exhausted"})),
                    ));
                }
            }
        }
    };

    let (new_container_id, host_port) = match ensure_container_running(
        &state,
        &template,
        &instance,
        &remote_type,
        resolved_host_path,
        &instance_repo,
        host_port,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            // The quota gate reserved the instance as `starting`; an infra
            // failure must roll it back to `stopped` so the user can retry.
            instance_repo.update_status(instance.id, "stopped").await.ok();
            return Err(e);
        }
    };

    if let Some(ref cid) = new_container_id {
        instance_repo.update_container_id(instance.id, cid).await.ok();

        // Rewrite the route only when the published port changed (a legacy
        // backfill or a recreate-on-stolen-port). On an unchanged restart the
        // route survives stop and needs no rewrite.
        if persisted_host_port != Some(host_port) {
            if let Err(e) = crate::route_writer::write_route(&remote_type, &instance.access_token, host_port, &instance.access_password) {
                tracing::error!("Failed to write Traefik route: {}", e);
            }
        }
        state.vnc_cache.insert(&instance.access_token, "starting");
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

/// All host ports currently allocated to non-deleted instances — the live set
/// the pure allocator must not hand out again.
async fn collect_used_host_ports(
    instance_repo: &WorkspaceInstanceRepository<'_>,
) -> BTreeSet<u16> {
    instance_repo
        .list_host_ports()
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|p| p as u16)
        .collect()
}

/// Reduce a Docker network's IPv4 subnet CIDR to its aligned `/30` network
/// address — the key the subnet allocator compares. Docker hands back aligned
/// `a.b.c.d/30` strings (unchanged by the mask); any non-`/30` subnet that
/// overlaps the base range is folded down to the `/30` block it occupies.
fn subnet_network_address(subnet: &str) -> Option<Ipv4Addr> {
    let addr: Ipv4Addr = subnet.split('/').next()?.parse().ok()?;
    Some(Ipv4Addr::from(u32::from(addr) & !0b11))
}

/// Docker rejects a network whose pool overlaps an existing one on the same
/// address space (HTTP 403, `invalid pool request: Pool overlaps ...`). Two
/// *concurrent* launches can both compute the same free subnet from the same
/// `list_networks` snapshot; this error means the launch should re-scan and
/// re-allocate rather than fail outright.
fn is_network_pool_overlap(err: &str) -> bool {
    err.contains("Pool overlaps") || err.contains("pool overlaps")
}

/// Ensure `instance`'s dedicated `/30` network exists (spec §5) and return its
/// name. Idempotent: if the network already exists, its subnet is reused
/// unchanged; otherwise the lowest free `/30` from the base range is allocated
/// and created. The `network_lock` serializes concurrent ensures in this
/// process; a concurrent pool-overlap (cross-process or a manual
/// `docker network create` landing between list and create) triggers a bounded
/// re-allocation from a per-instance spread so retries don't stampede the same
/// block. Shared by the `launch` and `start` (backfill) paths.
async fn ensure_instance_network(
    state: &AppState,
    instance: &WorkspaceInstance,
) -> Result<String, String> {
    let base = crate::instance_net::NetBase::parse(&state.settings.instance_net_base)
        .map_err(|e| format!("Invalid instance network base: {}", e))?;
    let network_name = crate::instance_net::network_name(&instance.id.to_string());
    let max_network_attempts = 4;
    {
        let _network_guard = state.network_lock.lock().await;
        for attempt in 0..max_network_attempts {
            let networks = state.docker
                .list_networks()
                .await
                .map_err(|e| format!("Failed to list Docker networks: {}", e))?;
            // Reuse the existing network (idempotent ensure / legacy backfill):
            // a pre-existing instance keeps its subnet across stops and starts.
            if let Some(_existing) = networks
                .iter()
                .find(|info| info.name == network_name)
                .and_then(|info| info.subnet.as_deref())
                .and_then(subnet_network_address)
            {
                return Ok(network_name);
            }
            let used_subnets: BTreeSet<Ipv4Addr> = networks
                .iter()
                .filter_map(|info| info.subnet.as_deref())
                .filter_map(subnet_network_address)
                .collect();
            let instance_network = match if attempt == 0 {
                crate::instance_net::lowest_free_subnet(&used_subnets, &base)
            } else {
                // After a pool-overlap collision, re-scan from a per-instance
                // spread so concurrent retries don't all stampede the same
                // lowest free block.
                crate::instance_net::lowest_free_subnet_from(
                    &used_subnets,
                    &base,
                    crate::instance_net::spread_block_offset(&instance.access_token, &base),
                )
            } {
                Some(network) => network,
                None => return Err("Instance subnet pool exhausted".to_string()),
            };
            let subnet_cidr = format!("{}/30", instance_network);
            let gateway_ip = crate::instance_net::gateway_ip(instance_network).to_string();
            match state.docker
                .create_network(&network_name, &subnet_cidr, &gateway_ip)
                .await
            {
                Ok(()) => return Ok(network_name),
                Err(e) if attempt + 1 < max_network_attempts && is_network_pool_overlap(&e) => {
                    tracing::warn!(
                        "Instance network subnet {} for '{}' collided with a concurrent launch, re-allocating: {}",
                        subnet_cidr,
                        instance.name,
                        e
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        "Failed to create instance network for '{}': {} (DB record kept)",
                        instance.name,
                        e
                    );
                    return Err(format!("Failed to create network '{}': {}", network_name, e));
                }
            }
        }
    }
    Err("Instance subnet pool exhausted".to_string())
}

/// Circular-scan base port for a port-conflict retry: a token-derived offset
/// into the pool so concurrent launches don't all retry the same lowest port.
fn host_port_spread_base(start: u16, end: u16, token: &str) -> u16 {
    let width = end as u32 - start as u32;
    start + crate::host_port::spread_offset(token, width.max(1) as u16)
}

/// Build the `ContainerConfig` and create the container for `instance`. The
/// container is placed on `instance`'s dedicated network (`network_name`) with
/// the `OW_DNS` nameservers. The persistent-volume ensure happens once in
/// `create_container_with_port_retry` (idempotent) before any create attempt.
/// Returns the raw Docker error string so callers can distinguish the
/// port-conflict race.
async fn build_and_create_container(
    state: &AppState,
    template: &WorkspaceTemplate,
    instance: &WorkspaceInstance,
    remote_type: &RemoteType,
    resolved_host_path: Option<&str>,
    host_port: u16,
    network_name: &str,
) -> Result<String, String> {
    let container_config = ContainerConfig {
        image: template.image.clone(),
        cores: template.cores,
        memory: template.memory,
        gpu_count: template.gpu_count,
        remote_type: remote_type.clone(),
        run_config: template.run_config.clone(),
        exec_config: template.exec_config.clone(),
        volume_mappings: template.volume_mappings.clone(),
        persistent_volume_name: resolved_host_path.map(persistent_volume_name),
        command: None,
        runtime: Some(resolve_runtime(&template.container_runtime, &state.settings.container_runtime)),
        network_bandwidth_up_mbps: template.network_bandwidth_up_mbps,
        network_bandwidth_down_mbps: template.network_bandwidth_down_mbps,
        host_port: Some(host_port),
        host_gateway_ip: Some(state.settings.host_gateway_ip.clone()),
        docker_in_instance: template.docker_in_instance,
        network_name: Some(network_name.to_string()),
        instance_dns: Some(state.settings.instance_dns.clone()),
    };
    state.docker.create_container_from_template(
        &instance.name,
        instance.instance_number,
        &container_config,
        &instance.access_password,
        &instance.access_token,
    ).await
}

/// Create `instance`'s container with a bounded retry on the port-conflict
/// race (mirrors the launch path): each retry re-allocates the next free port
/// scanning circularly from a token-derived offset. `to_remove` is an optional
/// stale container to drop first so its name is free for re-creation. The
/// container always attaches to `network_name` (already ensured by the caller),
/// so a recreate-on-stolen-port reuses the same network and subnet.
async fn create_container_with_port_retry<'a>(
    state: &AppState,
    template: &WorkspaceTemplate,
    instance: &WorkspaceInstance,
    remote_type: &RemoteType,
    resolved_host_path: Option<&str>,
    initial_port: u16,
    used_ports: &mut BTreeSet<u16>,
    to_remove: Option<&'a str>,
    network_name: &str,
) -> Result<(String, u16), (StatusCode, Json<serde_json::Value>)> {
    let instance_repo = WorkspaceInstanceRepository::new(&state.db);
    let base_from = host_port_spread_base(
        state.settings.host_port_start,
        state.settings.host_port_end,
        &instance.access_token,
    );
    // Re-declare a lost local-bind Volume before creating (per spec §補充), so
    // Docker never silently creates a plain named volume instead.
    if let Some(host_path) = resolved_host_path {
        let volume_name = persistent_volume_name(host_path);
        state.docker
            .ensure_persistent_volume(host_path, &volume_name)
            .await
            .map_err(|e| {
                tracing::error!("Failed to ensure persistent volume for '{}': {}", instance.name, e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Failed to ensure persistent volume"})),
                )
            })?;
    }
    let mut host_port = initial_port;
    let mut retries = 0;
    let mut to_remove = to_remove;
    while retries < 5 {
        if let Some(cid) = to_remove {
            state.docker.remove_container_by_id(cid).await.map_err(|e| {
                tracing::error!(
                    "Failed to remove container '{}' during port-conflict recreate: {}",
                    cid,
                    e
                );
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Failed to create container"})),
                )
            })?;
            to_remove = None;
        }
        match build_and_create_container(state, template, instance, remote_type, resolved_host_path, host_port, network_name).await {
            Ok(id) => {
            instance_repo.update_host_port(instance.id, Some(host_port as i32)).await.map_err(|e| {
                tracing::error!("Failed to commit host port {} for '{}': {}", host_port, instance.name, e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Failed to allocate host port"})),
                )
            })?;
                return Ok((id, host_port));
            }
            Err(e) if crate::host_port::is_port_conflict(&e) => {
                retries += 1;
                used_ports.insert(host_port);
                match crate::host_port::allocate_host_port_from(
                    used_ports,
                    state.settings.host_port_start,
                    state.settings.host_port_end,
                    &state.settings.host_gateway_ip,
                    base_from.wrapping_add(retries as u16),
                ) {
                    Some(next) => host_port = next,
                    None => {
                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(serde_json::json!({"error": "Host port pool exhausted"})),
                        ));
                    }
                }
            }
            Err(e) => {
                tracing::error!("Failed to create container for '{}': {}", instance.name, e);
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Failed to create container"})),
                ));
            }
        }
    }
    Err((
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(serde_json::json!({"error": "Failed to create container after repeated port conflicts"})),
    ))
}

/// Ensure `instance` has a running container on `host_port`, handling all
/// start-path shapes: already-running, stopped (plain restart, or recreate on
/// a fresh port when a concurrent launch stole the freed port), missing
/// container, and no container. Returns the container id (if any) and the
/// final committed host port.
async fn ensure_container_running(
    state: &AppState,
    template: &Option<WorkspaceTemplate>,
    instance: &WorkspaceInstance,
    remote_type: &RemoteType,
    resolved_host_path: Option<&str>,
    instance_repo: &WorkspaceInstanceRepository<'_>,
    host_port: u16,
) -> Result<(Option<String>, u16), (StatusCode, Json<serde_json::Value>)> {
    // Ensure the instance's dedicated `/30` network exists before any restart
    // or recreate (spec §5). Idempotent: an unchanged restart reuses the same
    // network and subnet, and a pre-existing instance created before this
    // feature gets its network backfilled on its next start.
    let network = match ensure_instance_network(state, instance).await {
        Ok(network) => network,
        Err(e) => {
            tracing::error!("Failed to ensure network for instance '{}': {}", instance.name, e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Failed to ensure instance network: {}", e)})),
            ));
        }
    };

    let mut used_ports: BTreeSet<u16> = collect_used_host_ports(instance_repo).await;
    used_ports.insert(host_port);
    let base_from = host_port_spread_base(
        state.settings.host_port_start,
        state.settings.host_port_end,
        &instance.access_token,
    );

    let mut current_port = host_port;
    let container_id = match instance.container_id {
        Some(ref cid) => match state.docker.inspect_container_state(cid).await {
            Ok(Some(state_str)) if state_str.to_lowercase().contains("running") => {
                tracing::info!("Container for '{}' already running, updating DB", instance.name);
                Some(cid.clone())
            }
            Ok(Some(_)) => {
                tracing::info!("Starting stopped container for '{}' (id: {})", instance.name, &cid[..12]);
                match state.docker.start_container_by_id(cid).await {
                    Ok(()) => {}
                    Err(e) if crate::host_port::is_port_conflict(&e.to_string()) => {
                        // The container's host port was freed on stop and a
                        // concurrent launch bound it before this restart. Drop
                        // the stale container and recreate it on a fresh port.
                        tracing::warn!(
                            "Port conflict restarting '{}': {}; re-creating on a fresh port",
                            instance.name,
                            e
                        );
                        let fresh = crate::host_port::allocate_host_port_from(
                            &used_ports,
                            state.settings.host_port_start,
                            state.settings.host_port_end,
                            &state.settings.host_gateway_ip,
                            base_from.wrapping_add(1),
                        )
                        .ok_or((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(serde_json::json!({"error": "Host port pool exhausted"})),
                        ))?;
                        let t = template.as_ref().ok_or((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(serde_json::json!({"error": "Template not found for instance"})),
                        ))?;
                        let (new_id, new_port) = create_container_with_port_retry(
                            state,
                            t,
                            instance,
                            remote_type,
                            resolved_host_path,
                            fresh,
                            &mut used_ports,
                            Some(cid),
                            &network,
                        )
                        .await?;
                        current_port = new_port;
                        return Ok((Some(new_id), current_port));
                    }
                    Err(e) => {
                        tracing::error!("Failed to start container for '{}': {}", instance.name, e);
                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(serde_json::json!({"error": "Failed to start container"})),
                        ));
                    }
                }
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
                Some(cid.clone())
            }
            _ => {
                tracing::warn!("Container for '{}' not found, creating new one", instance.name);
                let t = template.as_ref().ok_or((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(serde_json::json!({"error": "Template not found for instance"})),
                ))?;
                let (new_id, new_port) = create_container_with_port_retry(
                    state,
                    t,
                    instance,
                    remote_type,
                    resolved_host_path,
                    current_port,
                    &mut used_ports,
                    None,
                    &network,
                )
                .await?;
                current_port = new_port;
                return Ok((Some(new_id), current_port));
            }
        },
        None => {
            tracing::info!("No container for instance '{}', creating new one", instance.name);
            let t = template.as_ref().ok_or((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": "Template not found for instance"})),
            ))?;
            let (new_id, new_port) = create_container_with_port_retry(
                state,
                t,
                instance,
                remote_type,
                resolved_host_path,
                current_port,
                &mut used_ports,
                None,
                &network,
            )
            .await?;
            current_port = new_port;
            return Ok((Some(new_id), current_port));
        }
    };
    Ok((container_id, current_port))
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

    // The Traefik route is deliberately kept: the host port is reserved for the
    // lifetime of the instance, so the route stays valid across stop/start with
    // zero churn (user story: stable bookmarked URL).
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
