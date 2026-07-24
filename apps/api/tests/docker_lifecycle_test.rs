#![cfg(feature = "docker")]

mod common;

use common::TestContext;

async fn create_test_config(ctx: &TestContext, suffix: &str) -> String {
    common::ensure_network().await;

    ctx.login_admin().await;
    let name = format!("docker_lt_{}_{}", std::process::id(), suffix);
    let resp = ctx
        .post("/api/configs", &serde_json::json!({
            "name": name,
            "image": "busybox:1",
            "cores": 0,
            "memory": 0,
            "run_config": { "command": ["sleep", "3600"] },
        }))
        .await;
    assert_eq!(resp.status(), 200, "create config failed");
    let body: serde_json::Value = resp.json().await.unwrap();
    body["config"]["id"].as_str().unwrap().to_string()
}

async fn launch_instance(ctx: &TestContext, config_id: &str) -> String {
    let resp = ctx
        .post("/api/instances", &serde_json::json!({
            "config_id": config_id,
        }))
        .await;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap();
    if status != 200 || body["instance"]["status"].as_str() != Some("running") {
        panic!(
            "launch failed: status={}, body={}",
            status,
            serde_json::to_string_pretty(&body).unwrap()
        );
    }
    body["instance"]["id"].as_str().unwrap().to_string()
}

async fn cleanup_instance(ctx: &TestContext, instance_id: &str) {
    ctx.delete(&format!("/api/instances/{}", instance_id)).await;
}

#[tokio::test]
async fn test_launch_and_delete_instance() {
    let ctx = TestContext::new().await;
    let config_id = create_test_config(&ctx, "launch_del").await;

    let instance_id = launch_instance(&ctx, &config_id).await;

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"].as_str().unwrap(), "running");
    assert!(body["instance"]["container_id"].as_str().is_some());

    let resp = ctx.delete(&format!("/api/instances/{}", instance_id)).await;
    assert_eq!(resp.status(), 204);
}

#[tokio::test]
async fn test_stop_and_start_instance() {
    let ctx = TestContext::new().await;
    let config_id = create_test_config(&ctx, "stop_start").await;

    let instance_id = launch_instance(&ctx, &config_id).await;

    let resp = ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"].as_str().unwrap(), "stopped");

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"].as_str().unwrap(), "stopped");

    let resp = ctx.post(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200, "start failed: {:?}", resp.text().await);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"].as_str().unwrap(), "running");

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"].as_str().unwrap(), "running");

    cleanup_instance(&ctx, &instance_id).await;
}

#[tokio::test]
async fn test_pause_and_unpause_instance() {
    let ctx = TestContext::new().await;
    let config_id = create_test_config(&ctx, "pause_unpause").await;

    let instance_id = launch_instance(&ctx, &config_id).await;

    let resp = ctx.post(&format!("/api/instances/{}/pause", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200, "pause failed: {:?}", resp.text().await);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"].as_str().unwrap(), "paused");

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"].as_str().unwrap(), "paused");

    let resp = ctx.post(&format!("/api/instances/{}/unpause", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200, "unpause failed: {:?}", resp.text().await);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"].as_str().unwrap(), "running");

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"].as_str().unwrap(), "running");

    cleanup_instance(&ctx, &instance_id).await;
}

#[tokio::test]
async fn test_stop_already_stopped_returns_conflict() {
    let ctx = TestContext::new().await;
    let config_id = create_test_config(&ctx, "stop_conflict").await;

    let instance_id = launch_instance(&ctx, &config_id).await;

    let resp = ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200);

    let resp = ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 409);

    cleanup_instance(&ctx, &instance_id).await;
}

#[tokio::test]
async fn test_pause_not_running_returns_conflict() {
    let ctx = TestContext::new().await;
    let config_id = create_test_config(&ctx, "pause_conflict").await;

    let instance_id = launch_instance(&ctx, &config_id).await;

    ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;

    let resp = ctx.post(&format!("/api/instances/{}/pause", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 409);

    cleanup_instance(&ctx, &instance_id).await;
}

#[tokio::test]
async fn test_unpause_not_paused_returns_conflict() {
    let ctx = TestContext::new().await;
    let config_id = create_test_config(&ctx, "unpause_conflict").await;

    let instance_id = launch_instance(&ctx, &config_id).await;

    let resp = ctx.post(&format!("/api/instances/{}/unpause", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 409);

    cleanup_instance(&ctx, &instance_id).await;
}

#[tokio::test]
async fn test_start_already_running_returns_conflict() {
    let ctx = TestContext::new().await;
    let config_id = create_test_config(&ctx, "start_conflict").await;

    let instance_id = launch_instance(&ctx, &config_id).await;

    let resp = ctx.post(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 409);

    cleanup_instance(&ctx, &instance_id).await;
}

#[tokio::test]
async fn test_context_helpers() {
    let ctx = TestContext::new().await;
    let _ = ctx.login_user("admin", "admin").await;
    let _ = ctx.put("/health", &serde_json::json!({})).await;
    let _ = ctx.login_token().await;
}

#[tokio::test]
async fn test_launch_with_bad_image_returns_error_status() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;
    ctx.login_admin().await;

    let name = format!("docker_lt_badimg_{}", std::process::id());
    let resp = ctx
        .post("/api/configs", &serde_json::json!({
            "name": name,
            "image": "nonexistent-image-12345:latest",
            "run_config": { "command": ["sleep", "3600"] },
        }))
        .await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let config_id = body["config"]["id"].as_str().unwrap().to_string();

    let resp = ctx
        .post("/api/instances", &serde_json::json!({
            "config_id": config_id,
        }))
        .await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let status = body["instance"]["status"].as_str().unwrap();
    if status == "error" {
        assert!(body.get("docker_error").is_some());
    }
    let instance_id = body["instance"]["id"].as_str().unwrap();
    cleanup_instance(&ctx, instance_id).await;
}

#[tokio::test]
async fn test_launch_with_mount_persistent() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;
    ctx.login_admin().await;

    let name = format!("docker_lt_mount_{}", std::process::id());
    let resp = ctx
        .post("/api/configs", &serde_json::json!({
            "name": name,
            "image": "busybox:1",
            "run_config": { "command": ["sleep", "3600"] },
        }))
        .await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let config_id = body["config"]["id"].as_str().unwrap().to_string();

    let resp = ctx
        .post("/api/instances", &serde_json::json!({
            "config_id": config_id,
            "mount_persistent": true,
            "resolved_volume_host_path": "/tmp/ow_test_mount"
        }))
        .await;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap();
    let instance_id = body["instance"]["id"].as_str().unwrap();

    if status == 200 && body["instance"]["status"].as_str() == Some("running") {
        assert_eq!(body["instance"]["mount_persistent"], true);
        assert_eq!(body["instance"]["resolved_volume_host_path"], "/tmp/ow_test_mount");
        cleanup_instance(&ctx, instance_id).await;
    }
}

#[tokio::test]
async fn test_start_existing_stopped_container() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;
    ctx.login_admin().await;

    let config_id = create_test_config(&ctx, "start_stopped").await;
    let instance_id = launch_instance(&ctx, &config_id).await;

    let resp = ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200);

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"].as_str().unwrap(), "stopped");
    assert!(body["instance"]["container_id"].as_str().is_some());

    let resp = ctx.post(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200, "start failed: {:?}", resp.text().await);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"].as_str(), Some("running"));

    cleanup_instance(&ctx, &instance_id).await;
}

#[tokio::test]
async fn test_delete_with_container() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;
    ctx.login_admin().await;

    let config_id = create_test_config(&ctx, "del_container").await;
    let instance_id = launch_instance(&ctx, &config_id).await;

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["instance"]["container_id"].as_str().is_some());

    let resp = ctx.delete(&format!("/api/instances/{}", instance_id)).await;
    assert_eq!(resp.status(), 204);

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    assert_eq!(resp.status(), 404);
}
