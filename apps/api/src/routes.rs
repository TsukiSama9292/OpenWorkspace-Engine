use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use bcrypt::{hash, verify, DEFAULT_COST};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::{clear_cookie, create_token, set_cookie, Claims, AuthUser};
use crate::db::{RegistryRepository, UserRepository, WorkspaceRepository};
use crate::docker::DockerClient;
use crate::vnc_cache::VncCache;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub vnc_cache: VncCache,
}

#[derive(Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Deserialize)]
struct RegisterRequest {
    username: String,
    password: String,
    role: Option<String>,
}

#[derive(Deserialize)]
struct CreateWorkspaceRequest {
    name: String,
    #[serde(default = "default_image")]
    image: String,
    #[serde(default = "default_cores")]
    cores: i32,
    #[serde(default = "default_memory")]
    memory: i64,
    #[serde(default)]
    gpu_count: i32,
    #[serde(default = "default_true")]
    persistent_storage: bool,
    volume_host_path: Option<String>,
    #[serde(default = "default_volume_container_path")]
    volume_container_path: String,
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
fn default_true() -> bool {
    true
}
fn default_volume_container_path() -> String {
    "/home/kasm_user".to_string()
}

#[derive(Deserialize)]
struct SetRegistryUrlRequest {
    url: String,
}

pub fn api_routes() -> Router<AppState> {
    Router::new()
        .route("/health", get(health))
        .route("/api/auth/login", post(login))
        .route("/api/auth/register", post(register))
        .route("/api/auth/validate", get(validate))
        .route("/api/auth/me", get(me))
        .route("/api/auth/logout", post(logout))
        .route("/api/users", get(list_users))
        .route("/api/users/{id}", get(get_user).delete(delete_user))
        .route(
            "/api/workspaces",
            get(list_workspaces).post(create_workspace),
        )
        .route(
            "/api/workspaces/{id}",
            get(get_workspace).delete(delete_workspace),
        )
        .route("/api/workspaces/{id}/start", post(start_workspace))
        .route("/api/workspaces/{id}/stop", post(stop_workspace))
        .route("/api/workspaces/{id}/pause", post(pause_workspace))
        .route("/api/workspaces/{id}/unpause", post(unpause_workspace))
        .route("/api/registry", get(get_registry))
        .route("/api/registry/sync", post(sync_registry))
        .route("/api/registry/url", get(get_registry_url).put(set_registry_url))
        .route("/api/docker/containers", get(list_docker_containers))
        .route("/api/docker/containers/create", post(create_docker_container))
        .route("/api/vnc/verify", get(vnc_verify))
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok" }))
}

async fn login(
    State(state): State<AppState>,
    Json(input): Json<LoginRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let repo = UserRepository::new(&state.db);

    let user = repo
        .find_by_username(&input.username)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let valid = verify(&input.password, &user.2).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if !valid {
        return Err(StatusCode::UNAUTHORIZED);
    }

    let token = create_token(&user.0, &user.3)?;

    let mut headers = axum::http::HeaderMap::new();
    set_cookie(&mut headers, &token);

    Ok((
        headers,
        Json(serde_json::json!({
            "user": { "id": user.0, "username": user.1, "role": user.3 }
        })),
    ))
}

async fn register(
    State(state): State<AppState>,
    Json(input): Json<RegisterRequest>,
) -> Result<impl IntoResponse, StatusCode> {
    let repo = UserRepository::new(&state.db);

    let password_hash = hash(&input.password, DEFAULT_COST)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let role = input.role.unwrap_or_else(|| "user".to_string());

    let user_id = repo
        .create(&input.username, &password_hash, &role)
        .await
        .map_err(|_| StatusCode::CONFLICT)?;

    Ok(Json(serde_json::json!({
        "user": { "id": user_id, "username": input.username, "role": role }
    })))
}

async fn validate(AuthUser { user_id, role }: AuthUser) -> impl IntoResponse {
    Json(serde_json::json!({ "user_id": user_id, "role": role }))
}

async fn me(
    State(state): State<AppState>,
    AuthUser { user_id, .. }: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let repo = UserRepository::new(&state.db);

    let user = repo
        .find_by_id(user_id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(serde_json::json!({
        "user": { "id": user.0, "username": user.1, "role": user.3, "created_at": user.4 }
    })))
}

async fn logout() -> impl IntoResponse {
    let mut headers = axum::http::HeaderMap::new();
    clear_cookie(&mut headers);
    (headers, Json(serde_json::json!({ "status": "ok" })))
}

async fn list_users(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let repo = UserRepository::new(&state.db);

    let users = repo
        .list_all()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let users_json: Vec<_> = users
        .into_iter()
        .map(|u| {
            serde_json::json!({
                "id": u.0, "username": u.1, "role": u.2, "created_at": u.3
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "users": users_json })))
}

async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let repo = UserRepository::new(&state.db);

    let user = repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(serde_json::json!({
        "user": { "id": user.0, "username": user.1, "role": user.3, "created_at": user.4 }
    })))
}

async fn delete_user(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    auth: AuthUser,
) -> Result<StatusCode, StatusCode> {
    if auth.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    let repo = UserRepository::new(&state.db);

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

fn workspace_to_json(ws: &crate::db::Workspace) -> serde_json::Value {
    serde_json::json!({
        "id": ws.id,
        "name": ws.name,
        "workspace_number": ws.instance_number,
        "container_id": ws.container_id,
        "status": ws.status,
        "owner_id": ws.owner_id,
        "owner_username": ws.owner_username,
        "vnc_token": ws.vnc_token,
        "image": ws.image,
        "cores": ws.cores,
        "memory": ws.memory,
        "gpu_count": ws.gpu_count,
        "persistent_storage": ws.persistent_storage,
        "volume_host_path": ws.volume_host_path,
        "volume_container_path": ws.volume_container_path,
    })
}

async fn list_workspaces(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let repo = WorkspaceRepository::new(&state.db);

    let workspaces = if auth.role == "admin" {
        repo.list_all()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        repo.list_by_owner(auth.user_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    let workspaces_json: Vec<_> = workspaces.iter().map(workspace_to_json).collect();

    Ok(Json(serde_json::json!({ "workspaces": workspaces_json })))
}

async fn create_workspace(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<CreateWorkspaceRequest>,
) -> Response {
    let repo = WorkspaceRepository::new(&state.db);

    let workspace = match repo
        .create(
            &input.name,
            auth.user_id,
            &input.image,
            input.cores,
            input.memory,
            input.gpu_count,
            input.persistent_storage,
            input.volume_host_path.as_deref(),
            &input.volume_container_path,
        )
        .await
    {
        Ok(ws) => ws,
        Err(e) => {
            tracing::error!("Failed to create workspace record: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Failed to create workspace"}))).into_response();
        }
    };

    tracing::info!(
        "Workspace '{}' created (id={}, #{}, token={})",
        workspace.name,
        workspace.id,
        workspace.instance_number,
        workspace.vnc_token.as_deref().unwrap_or("")
    );

    let docker = match DockerClient::new().await {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("Failed to connect to Docker: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Docker unavailable"}))).into_response();
        }
    };

    match docker
        .create_kasm_container(&workspace.name, workspace.instance_number)
        .await
    {
        Ok(container_id) => {
            repo.update_container_id(workspace.id, &container_id).await.ok();
            repo.update_status(workspace.id, "running").await.ok();

            // Write Traefik route
            if let Some(ref token) = workspace.vnc_token {
                match docker.get_container_ip(&container_id, "openworkspace-engin").await {
                    Ok(ip) => {
                        if let Err(e) = crate::vnc_trafik::write_vnc_route(token, &ip) {
                            tracing::error!("Failed to write Traefik VNC route: {}", e);
                        }
                        state.vnc_cache.insert(token, "running", auth.user_id);
                    }
                    Err(e) => tracing::error!("Failed to get container IP for Traefik route: {}", e),
                }
            }

            tracing::info!(
                "KasmVNC container started for workspace '{}' (container={})",
                workspace.name,
                &container_id[..12]
            );

            let mut ws = workspace;
            ws.container_id = Some(container_id);
            ws.status = "running".to_string();
            Json(serde_json::json!({ "workspace": workspace_to_json(&ws) }))
                .into_response()
        }
        Err(e) => {
            tracing::warn!(
                "Failed to create KasmVNC container for workspace '{}': {} (DB record kept)",
                workspace.name,
                e
            );
            repo.update_status(workspace.id, "error").await.ok();
            let mut ws = workspace;
            ws.status = "error".to_string();
            Json(serde_json::json!({ "workspace": workspace_to_json(&ws) }))
                .into_response()
        }
    }
}

async fn get_workspace(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let repo = WorkspaceRepository::new(&state.db);

    let workspace = repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(serde_json::json!({ "workspace": workspace_to_json(&workspace) })))
}

async fn delete_workspace(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Response {
    let repo = WorkspaceRepository::new(&state.db);

    let workspace = match repo.find_by_id(id).await {
        Ok(Some(ws)) => ws,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    // Delete Traefik route
    if let Some(ref token) = workspace.vnc_token {
        if let Err(e) = crate::vnc_trafik::delete_vnc_route(token) {
            tracing::error!("Failed to delete Traefik VNC route: {}", e);
        }
        state.vnc_cache.remove(token);
    }

    if let Some(ref container_id) = workspace.container_id {
        if let Ok(docker) = DockerClient::new().await {
            let _ = docker.stop_container_by_id(container_id).await;
            match docker.remove_container_by_id(container_id).await {
                Ok(()) => tracing::info!("Container removed for workspace '{}'", workspace.name),
                Err(e) => tracing::warn!("Failed to remove container for '{}': {}", workspace.name, e),
            }
        }
    }

    match repo.delete(id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn start_workspace(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let repo = WorkspaceRepository::new(&state.db);

    let workspace = repo
        .find_by_id(id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find workspace {}: {}", id, e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Internal error"})))
        })?
        .ok_or((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Workspace not found"}))))?;

    if workspace.status == "running" {
        return Err((StatusCode::CONFLICT, Json(serde_json::json!({"error": "Workspace is already running"}))).into_response());
    }

    let docker = DockerClient::new().await.map_err(|e| {
        tracing::error!("Failed to connect to Docker: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Docker unavailable"})))
    })?;

    let new_container_id = match workspace.container_id {
        Some(ref cid) => {
            match docker.inspect_container_state(cid).await {
                Ok(Some(state_str)) => {
                    if state_str.contains("Running") {
                        tracing::info!("Container for '{}' already running, updating DB", workspace.name);
                    } else {
                        tracing::info!("Starting stopped container for '{}' (id: {})", workspace.name, &cid[..12]);
                        docker.start_container_by_id(cid).await.map_err(|e| {
                            tracing::error!("Failed to start container for '{}': {}", workspace.name, e);
                            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Failed to start container"})))
                        })?;
                    }
                    Some(cid.clone())
                }
                _ => {
                    tracing::warn!("Container for '{}' not found, creating new one", workspace.name);
                    let new_id = docker.create_kasm_container(&workspace.name, workspace.instance_number).await.map_err(|e| {
                        tracing::error!("Failed to create container for '{}': {}", workspace.name, e);
                        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Failed to create container"})))
                    })?;
                    Some(new_id)
                }
            }
        }
        None => {
            tracing::info!("No container for workspace '{}', creating new KasmVNC container", workspace.name);
            let new_id = docker.create_kasm_container(&workspace.name, workspace.instance_number).await.map_err(|e| {
                tracing::error!("Failed to create container for '{}': {}", workspace.name, e);
                (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Failed to create container"})))
            })?;
            Some(new_id)
        }
    };

    if let Some(ref cid) = new_container_id {
        repo.update_container_id(workspace.id, cid).await.ok();

        // Write Traefik route
        if let Some(ref token) = workspace.vnc_token {
            match docker.get_container_ip(cid, "openworkspace-engin").await {
                Ok(ip) => {
                    if let Err(e) = crate::vnc_trafik::write_vnc_route(token, &ip) {
                        tracing::error!("Failed to write Traefik VNC route: {}", e);
                    }
                    state.vnc_cache.insert(token, "running", workspace.owner_id);
                }
                Err(e) => tracing::error!("Failed to get container IP for Traefik route: {}", e),
            }
        }
    }
    repo.update_status(workspace.id, "running").await.map_err(|e| {
        tracing::error!("Failed to update workspace status: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Failed to update status"})))
    })?;

    tracing::info!("Workspace '{}' (#{}) started", workspace.name, workspace.instance_number);

    Ok(Json(serde_json::json!({
        "status": "running",
        "container_id": new_container_id
    })))
}

async fn stop_workspace(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let repo = WorkspaceRepository::new(&state.db);

    let workspace = repo
        .find_by_id(id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find workspace {}: {}", id, e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Internal error"})))
        })?
        .ok_or((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Workspace not found"}))))?;

    if workspace.status == "stopped" {
        return Err((StatusCode::CONFLICT, Json(serde_json::json!({"error": "Workspace is already stopped"}))).into_response());
    }

    // If paused, unpause first before stopping
    if workspace.status == "paused" {
        if let Some(ref cid) = workspace.container_id {
            let docker = DockerClient::new().await.map_err(|e| {
                tracing::error!("Failed to connect to Docker: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            let _ = docker.unpause_container_by_id(cid).await;
        }
    }

    if let Some(ref cid) = workspace.container_id {
        let docker = DockerClient::new().await.map_err(|e| {
            tracing::error!("Failed to connect to Docker: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Docker unavailable"})))
        })?;

        match docker.stop_container_by_id(cid).await {
            Ok(()) => {
                tracing::info!("Container for '{}' stopped (id: {})", workspace.name, &cid[..12]);
            }
            Err(e) => {
                tracing::warn!("Failed to stop container for '{}': {} (updating DB anyway)", workspace.name, e);
            }
        }
    }

    // Delete Traefik route
    if let Some(ref token) = workspace.vnc_token {
        if let Err(e) = crate::vnc_trafik::delete_vnc_route(token) {
            tracing::error!("Failed to delete Traefik VNC route: {}", e);
        }
        state.vnc_cache.remove(token);
    }

    repo.update_status(workspace.id, "stopped").await.map_err(|e| {
        tracing::error!("Failed to update workspace status: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Failed to update status"})))
    })?;

    tracing::info!("Workspace '{}' stopped", workspace.name);

    Ok(Json(serde_json::json!({ "status": "stopped" })))
}

async fn pause_workspace(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let repo = WorkspaceRepository::new(&state.db);

    let workspace = repo
        .find_by_id(id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find workspace {}: {}", id, e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Internal error"})))
        })?
        .ok_or((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Workspace not found"}))))?;

    if workspace.status != "running" {
        return Err((StatusCode::CONFLICT, Json(serde_json::json!({"error": "Workspace must be running to pause"}))).into_response());
    }

    let cid = workspace.container_id.as_ref().ok_or(StatusCode::CONFLICT)?;

    let docker = DockerClient::new().await.map_err(|e| {
        tracing::error!("Failed to connect to Docker: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Docker unavailable"})))
    })?;

    docker.pause_container_by_id(cid).await.map_err(|e| {
        tracing::error!("Failed to pause container for '{}': {}", workspace.name, e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Failed to pause container"})))
    })?;

    repo.update_status(workspace.id, "paused").await.map_err(|e| {
        tracing::error!("Failed to update workspace status: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Failed to update status"})))
    })?;

    tracing::info!("Workspace '{}' paused", workspace.name);

    Ok(Json(serde_json::json!({ "status": "paused" })))
}

async fn unpause_workspace(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let repo = WorkspaceRepository::new(&state.db);

    let workspace = repo
        .find_by_id(id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find workspace {}: {}", id, e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Internal error"})))
        })?
        .ok_or((StatusCode::NOT_FOUND, Json(serde_json::json!({"error": "Workspace not found"}))))?;

    if workspace.status != "paused" {
        return Err((StatusCode::CONFLICT, Json(serde_json::json!({"error": "Workspace must be paused to resume"}))).into_response());
    }

    let cid = workspace.container_id.as_ref().ok_or(StatusCode::CONFLICT)?;

    let docker = DockerClient::new().await.map_err(|e| {
        tracing::error!("Failed to connect to Docker: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Docker unavailable"})))
    })?;

    docker.unpause_container_by_id(cid).await.map_err(|e| {
        tracing::error!("Failed to unpause container for '{}': {}", workspace.name, e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Failed to resume container"})))
    })?;

    repo.update_status(workspace.id, "running").await.map_err(|e| {
        tracing::error!("Failed to update workspace status: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Failed to update status"})))
    })?;

    tracing::info!("Workspace '{}' unpaused", workspace.name);

    Ok(Json(serde_json::json!({ "status": "running" })))
}

// ── Registry endpoints ───────────────────────────────────────────

async fn get_registry(
    State(state): State<AppState>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let repo = RegistryRepository::new(&state.db);

    let cached = repo
        .get_cached()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    match cached {
        Some(json) => Ok(Json(json)),
        None => Err(StatusCode::NOT_FOUND),
    }
}

async fn sync_registry(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if auth.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    let repo = RegistryRepository::new(&state.db);

    let url = repo
        .get_url()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::BAD_REQUEST)?;

    let body = reqwest::get(&url)
        .await
        .map_err(|e| {
            tracing::error!("Failed to fetch registry from '{}': {}", url, e);
            StatusCode::BAD_GATEWAY
        })?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| {
            tracing::error!("Failed to parse registry JSON from '{}': {}", url, e);
            StatusCode::BAD_GATEWAY
        })?;

    repo.set_cached(&body)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!("Registry synced from '{}'", url);

    Ok(Json(body))
}

async fn get_registry_url(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if auth.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    let repo = RegistryRepository::new(&state.db);

    let url = repo
        .get_url()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({
        "url": url
    })))
}

async fn set_registry_url(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<SetRegistryUrlRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if auth.role != "admin" {
        return Err(StatusCode::FORBIDDEN);
    }

    let repo = RegistryRepository::new(&state.db);

    repo.set_url(&input.url)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    tracing::info!("Registry URL set to '{}'", input.url);

    Ok(Json(serde_json::json!({ "url": input.url })))
}

// ── Docker raw endpoints ─────────────────────────────────────────

#[derive(Deserialize)]
struct CreateDockerContainerRequest {
    name: String,
    image: String,
}

async fn list_docker_containers(
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let client = DockerClient::new()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let containers = client
        .list_containers(true)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let containers_json: Vec<_> = containers
        .into_iter()
        .map(|c| {
            serde_json::json!({
                "id": c.id.unwrap_or_default(),
                "names": c.names.unwrap_or_default(),
                "image": c.image.unwrap_or_default(),
                "status": c.status.unwrap_or_default(),
                "state": c.state.unwrap_or_default(),
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "containers": containers_json })))
}

async fn create_docker_container(
    _auth: AuthUser,
    Json(input): Json<CreateDockerContainerRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let client = DockerClient::new()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let container_id = client
        .create_container(&input.name, &input.image)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "container_id": container_id })))
}

// ── VNC ForwardAuth Verify ─────────────────────────────────────────

async fn vnc_verify(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
) -> Result<axum::http::HeaderMap, StatusCode> {
    let secret = std::env::var("JWT_SECRET").expect("JWT_SECRET must be set");

    let cookie = headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token = cookie
        .split(';')
        .find(|c| c.trim().starts_with("ow_token="))
        .and_then(|c| c.trim().strip_prefix("ow_token="))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let token_data = jsonwebtoken::decode::<Claims>(
        token,
        &jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
        &jsonwebtoken::Validation::default(),
    )
    .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let user_id: Uuid = token_data
        .claims
        .sub
        .parse()
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    let role = token_data.claims.role;

    // Extract VNC token from the forwarded URI: /vnc/{token}/websockify
    let forwarded_uri = headers
        .get("X-Forwarded-Uri")
        .and_then(|v| v.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;

    let vnc_token = forwarded_uri
        .strip_prefix("/vnc/")
        .and_then(|rest| rest.strip_suffix("/websockify"))
        .ok_or(StatusCode::UNAUTHORIZED)?;

    // Fast path: check in-memory cache (no DB query)
    match state.vnc_cache.get(vnc_token) {
        Some(entry) => {
            if entry.status != "running" {
                return Err(StatusCode::NOT_FOUND);
            }
        }
        None => {
            // Cache miss: fall back to DB, then populate cache
            let repo = WorkspaceRepository::new(&state.db);
            let workspace = repo
                .find_by_vnc_token(vnc_token)
                .await
                .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
                .ok_or(StatusCode::NOT_FOUND)?;

            if workspace.status != "running" {
                return Err(StatusCode::NOT_FOUND);
            }

            state.vnc_cache.insert(vnc_token, &workspace.status, workspace.owner_id);
        }
    }

    let mut resp_headers = axum::http::HeaderMap::new();
    resp_headers.insert("X-Forwarded-User", user_id.to_string().parse().unwrap());
    resp_headers.insert("X-Forwarded-Role", role.parse().unwrap());

    Ok(resp_headers)
}
