#![cfg(feature = "docker")]

mod common;

use common::{DockerContainerGuard, TestContext};

#[tokio::test]
async fn test_list_docker_containers() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.get("/api/docker/containers").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let containers = body["containers"].as_array().unwrap();
    assert!(!containers.is_empty(), "should have at least one container (Docker daemon)");
    let first = &containers[0];
    assert!(first["id"].is_string());
    assert!(first["image"].is_string());
    assert!(first["state"].is_string());
}

#[tokio::test]
async fn test_create_and_list_docker_container() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let name = format!("ow_test_docker_api_{}", std::process::id());
    let resp = ctx.post("/api/docker/containers/create", &serde_json::json!({
        "name": name,
        "image": "busybox:1",
    })).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let container_id = body["container_id"].as_str().unwrap();
    assert!(!container_id.is_empty());
    let _guard = DockerContainerGuard::new(container_id);

    let resp = ctx.get("/api/docker/containers").await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let containers = body["containers"].as_array().unwrap();
    assert!(containers.iter().any(|c| {
        c["id"].as_str().unwrap_or_default().starts_with(container_id)
            || c["names"].as_array().map(|n| n.iter().any(|n| n.as_str().unwrap_or_default().contains(&name))).unwrap_or(false)
    }));
}

#[tokio::test]
async fn test_docker_containers_requires_auth() {
    let ctx = TestContext::new().await;
    let resp = ctx.get("/api/docker/containers").await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_context_helpers() {
    let ctx = TestContext::new().await;
    let _ = ctx.login_user("admin", "admin").await;
    let _ = ctx.put("/health", &serde_json::json!({})).await;
    let _ = ctx.delete("/health").await;
    let _ = ctx.login_token().await;
}
