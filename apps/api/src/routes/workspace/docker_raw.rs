use axum::{
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;

use super::super::AppState;
use crate::auth::AuthUser;
use crate::docker::DockerClient;

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
