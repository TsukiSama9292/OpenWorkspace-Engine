use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use bcrypt::{hash, verify, DEFAULT_COST};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::auth::{clear_cookie, create_token, set_cookie, Claims, AuthUser};
use crate::db::{InstanceRepository, UserRepository};
use crate::docker::DockerClient;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
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
struct CreateInstanceRequest {
    name: String,
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
            "/api/instances",
            get(list_instances).post(create_instance),
        )
        .route(
            "/api/instances/{id}",
            get(get_instance).delete(delete_instance),
        )
        .route("/api/instances/{id}/start", post(start_instance))
        .route("/api/instances/{id}/stop", post(stop_instance))
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

async fn list_instances(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let repo = InstanceRepository::new(&state.db);

    let instances = if auth.role == "admin" {
        repo.list_all()
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    } else {
        repo.list_by_owner(auth.user_id)
            .await
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    };

    let instances_json: Vec<_> = instances
        .into_iter()
        .map(|i| {
            serde_json::json!({
                "id": i.0, "name": i.1, "instance_number": i.2,
                "container_id": i.3, "status": i.4, "owner_id": i.5,
                "vnc_token": i.7
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "instances": instances_json })))
}

async fn create_instance(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<CreateInstanceRequest>,
) -> Response {
    let repo = InstanceRepository::new(&state.db);

    let (id, instance_number, vnc_token) = match repo
        .create(&input.name, auth.user_id)
        .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Failed to create instance record: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Failed to create instance"}))).into_response();
        }
    };

    tracing::info!(
        "Instance '{}' created (id={}, #{}, token={})",
        input.name,
        id,
        instance_number,
        vnc_token
    );

    let docker = match DockerClient::new().await {
        Ok(d) => d,
        Err(e) => {
            tracing::error!("Failed to connect to Docker: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "Docker unavailable"}))).into_response();
        }
    };

    match docker
        .create_kasm_container(&input.name, instance_number)
        .await
    {
        Ok(container_id) => {
            repo.update_container_id(id, &container_id).await.ok();
            repo.update_status(id, "running").await.ok();

            // Write Traefik route
            match docker.get_container_ip(&container_id, "openworkspace-engin").await {
                Ok(ip) => {
                    if let Err(e) = crate::vnc_trafik::write_vnc_route(&vnc_token, &ip) {
                        tracing::error!("Failed to write Traefik VNC route: {}", e);
                    }
                }
                Err(e) => tracing::error!("Failed to get container IP for Traefik route: {}", e),
            }

            tracing::info!(
                "KasmVNC container started for instance '{}' (container={})",
                input.name,
                &container_id[..12]
            );
            Json(serde_json::json!({
                "instance": {
                    "id": id,
                    "name": input.name,
                    "instance_number": instance_number,
                    "container_id": container_id,
                    "status": "running",
                    "vnc_token": vnc_token
                }
            }))
            .into_response()
        }
        Err(e) => {
            tracing::warn!(
                "Failed to create KasmVNC container for instance '{}': {} (DB record kept)",
                input.name,
                e
            );
            repo.update_status(id, "error").await.ok();
            Json(serde_json::json!({
                "instance": {
                    "id": id,
                    "name": input.name,
                    "instance_number": instance_number,
                    "status": "error",
                    "vnc_token": vnc_token
                }
            }))
            .into_response()
        }
    }
}

async fn get_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let repo = InstanceRepository::new(&state.db);

    let instance = repo
        .find_by_id(id)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(serde_json::json!({
        "instance": {
            "id": instance.0, "name": instance.1, "instance_number": instance.2,
            "container_id": instance.3, "status": instance.4, "owner_id": instance.5,
            "vnc_token": instance.7
        }
    })))
}

async fn delete_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Response {
    let repo = InstanceRepository::new(&state.db);

    let instance = match repo
        .find_by_id(id)
        .await
    {
        Ok(Some(i)) => i,
        Ok(None) => return StatusCode::NOT_FOUND.into_response(),
        Err(_) => return StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    };

    // Delete Traefik route
    if let Some(ref token) = instance.7 {
        if let Err(e) = crate::vnc_trafik::delete_vnc_route(token) {
            tracing::error!("Failed to delete Traefik VNC route: {}", e);
        }
    }

    if let Some(container_id) = instance.3 {
        if let Ok(docker) = DockerClient::new().await {
            let _ = docker.stop_container_by_id(&container_id).await;
            match docker.remove_container_by_id(&container_id).await {
                Ok(()) => tracing::info!("Container removed for instance '{}'", instance.1),
                Err(e) => tracing::warn!("Failed to remove container for '{}': {}", instance.1, e),
            }
        }
    }

    match repo.delete(id).await {
        Ok(true) => StatusCode::NO_CONTENT.into_response(),
        Ok(false) => StatusCode::NOT_FOUND.into_response(),
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
    }
}

async fn start_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let repo = InstanceRepository::new(&state.db);

    let instance = repo
        .find_by_id(id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find instance {}: {}", id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let (instance_id, name, instance_number, container_id, status, _, _, vnc_token) = instance;

    if status == "running" {
        return Err(StatusCode::CONFLICT);
    }

    let docker = DockerClient::new().await.map_err(|e| {
        tracing::error!("Failed to connect to Docker: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    let new_container_id = match container_id {
        Some(cid) => {
            match docker.inspect_container_state(&cid).await {
                Ok(Some(state_str)) => {
                    if state_str.contains("Running") {
                        tracing::info!("Container for '{}' already running, updating DB", name);
                    } else {
                        tracing::info!("Starting stopped container for '{}' (id: {})", name, &cid[..12]);
                        docker.start_container_by_id(&cid).await.map_err(|e| {
                            tracing::error!("Failed to start container for '{}': {}", name, e);
                            StatusCode::INTERNAL_SERVER_ERROR
                        })?;
                    }
                    Some(cid)
                }
                _ => {
                    tracing::warn!("Container for '{}' not found, creating new one", name);
                    let new_id = docker.create_kasm_container(&name, instance_number).await.map_err(|e| {
                        tracing::error!("Failed to create container for '{}': {}", name, e);
                        StatusCode::INTERNAL_SERVER_ERROR
                    })?;
                    Some(new_id)
                }
            }
        }
        None => {
            tracing::info!("No container for instance '{}', creating new KasmVNC container", name);
            let new_id = docker.create_kasm_container(&name, instance_number).await.map_err(|e| {
                tracing::error!("Failed to create container for '{}': {}", name, e);
                StatusCode::INTERNAL_SERVER_ERROR
            })?;
            Some(new_id)
        }
    };

    if let Some(ref cid) = new_container_id {
        repo.update_container_id(instance_id, cid).await.ok();

        // Write Traefik route
        match docker.get_container_ip(cid, "openworkspace-engin").await {
            Ok(ip) => {
                if let Err(e) = crate::vnc_trafik::write_vnc_route(vnc_token.as_deref().unwrap_or(""), &ip) {
                    tracing::error!("Failed to write Traefik VNC route: {}", e);
                }
            }
            Err(e) => tracing::error!("Failed to get container IP for Traefik route: {}", e),
        }
    }
    repo.update_status(instance_id, "running").await.map_err(|e| {
        tracing::error!("Failed to update instance status: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tracing::info!("Instance '{}' (#{}) started", name, instance_number);

    Ok(Json(serde_json::json!({
        "status": "running",
        "container_id": new_container_id
    })))
}

async fn stop_instance(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    _auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let repo = InstanceRepository::new(&state.db);

    let instance = repo
        .find_by_id(id)
        .await
        .map_err(|e| {
            tracing::error!("Failed to find instance {}: {}", id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or(StatusCode::NOT_FOUND)?;

    let (instance_id, name, _, container_id, status, _, _, vnc_token) = instance;

    if status == "stopped" {
        return Err(StatusCode::CONFLICT);
    }

    if let Some(cid) = container_id {
        let docker = DockerClient::new().await.map_err(|e| {
            tracing::error!("Failed to connect to Docker: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

        match docker.stop_container_by_id(&cid).await {
            Ok(()) => {
                tracing::info!("Container for '{}' stopped (id: {})", name, &cid[..12]);
            }
            Err(e) => {
                tracing::warn!("Failed to stop container for '{}': {} (updating DB anyway)", name, e);
            }
        }
    }

    // Delete Traefik route
    if let Some(ref token) = vnc_token {
        if let Err(e) = crate::vnc_trafik::delete_vnc_route(token) {
            tracing::error!("Failed to delete Traefik VNC route: {}", e);
        }
    }

    repo.update_status(instance_id, "stopped").await.map_err(|e| {
        tracing::error!("Failed to update instance status: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    tracing::info!("Instance '{}' stopped", name);

    Ok(Json(serde_json::json!({ "status": "stopped" })))
}

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

    let repo = InstanceRepository::new(&state.db);
    let instance = repo
        .find_by_vnc_token(vnc_token)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let (_id, _name, _instance_number, _container_id, status, _owner_id) = &instance;

    if status != "running" {
        return Err(StatusCode::NOT_FOUND);
    }

    let mut resp_headers = axum::http::HeaderMap::new();
    resp_headers.insert("X-Forwarded-User", user_id.to_string().parse().unwrap());
    resp_headers.insert("X-Forwarded-Role", role.parse().unwrap());

    Ok(resp_headers)
}
