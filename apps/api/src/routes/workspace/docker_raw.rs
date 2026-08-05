use axum::{
    extract::State,
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use super::super::AppState;
use crate::auth::AuthUser;

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/docker/containers", get(list_docker_containers))
        .route("/api/docker/containers/create", post(create_docker_container))
}

#[derive(Deserialize)]
struct CreateDockerContainerRequest {
    name: String,
    image: String,
}

async fn list_docker_containers(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !auth.can_manage_docker() {
        return Err(StatusCode::FORBIDDEN);
    }

    let containers = state.docker
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
    State(state): State<AppState>,
    auth: AuthUser,
    Json(input): Json<CreateDockerContainerRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if !auth.can_manage_docker() {
        return Err(StatusCode::FORBIDDEN);
    }

    let container_id = state.docker
        .create_container(&input.name, &input.image)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(serde_json::json!({ "container_id": container_id })))
}
