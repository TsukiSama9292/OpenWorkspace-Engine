mod common;

use common::TestContext;

#[tokio::test]
async fn test_create_config() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "test-config",
        "image": "kasmweb/desktop:1.19.0-rolling-daily",
        "cores": 2,
        "memory": 4294967296_i64
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["config"]["name"], "test-config");
    assert_eq!(body["config"]["cores"], 2);
    assert!(body["config"]["id"].is_string());
}

#[tokio::test]
async fn test_list_configs() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    ctx.post("/api/configs", &serde_json::json!({
        "name": "list-test-config",
        "image": "kasmweb/desktop:1.19.0-rolling-daily"
    })).await;

    let resp = ctx.get("/api/configs").await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["configs"].is_array());
    let configs = body["configs"].as_array().unwrap();
    assert!(configs.iter().any(|c| c["name"] == "list-test-config"));
}

#[tokio::test]
async fn test_get_config() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "get-test-config",
        "image": "kasmweb/desktop:1.19.0-rolling-daily"
    })).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let config_id = body["config"]["id"].as_str().unwrap();

    let resp = ctx.get(&format!("/api/configs/{}", config_id)).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["config"]["name"], "get-test-config");
    assert_eq!(body["config"]["instance_count"], 0);
}

#[tokio::test]
async fn test_update_config() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "update-test",
        "image": "kasmweb/desktop:1.19.0-rolling-daily",
        "cores": 1
    })).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let config_id = body["config"]["id"].as_str().unwrap();

    let resp = ctx.put(&format!("/api/configs/{}", config_id), &serde_json::json!({
        "name": "update-test-renamed",
        "image": "kasmweb/desktop:1.19.0-rolling-daily",
        "cores": 4,
        "memory": 8589934592_i64,
        "gpu_count": 0,
        "run_config": {},
        "exec_config": {},
        "volume_mappings": {}
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["config"]["name"], "update-test-renamed");
    assert_eq!(body["config"]["cores"], 4);
}

#[tokio::test]
async fn test_delete_config() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "delete-test",
        "image": "kasmweb/desktop:1.19.0-rolling-daily"
    })).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let config_id = body["config"]["id"].as_str().unwrap();

    let resp = ctx.delete(&format!("/api/configs/{}", config_id)).await;
    assert_eq!(resp.status(), 204);

    let resp = ctx.get(&format!("/api/configs/{}", config_id)).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_get_nonexistent_config() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let fake_id = uuid::Uuid::new_v4();
    let resp = ctx.get(&format!("/api/configs/{}", fake_id)).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_update_nonexistent_config() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let fake_id = uuid::Uuid::new_v4();
    let resp = ctx.put(&format!("/api/configs/{}", fake_id), &serde_json::json!({
        "name": "updated",
        "image": "test:latest",
        "cores": 2,
        "memory": 4294967296_i64,
        "gpu_count": 0,
        "run_config": {},
        "exec_config": {},
        "volume_mappings": {}
    })).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_delete_nonexistent_config() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let fake_id = uuid::Uuid::new_v4();
    let resp = ctx.delete(&format!("/api/configs/{}", fake_id)).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_create_config_requires_auth() {
    let ctx = TestContext::new().await;
    let resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "no-auth-config",
        "image": "test:latest"
    })).await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_list_configs_requires_auth() {
    let ctx = TestContext::new().await;
    let resp = ctx.get("/api/configs").await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_get_config_requires_auth() {
    let ctx = TestContext::new().await;
    let fake_id = uuid::Uuid::new_v4();
    let resp = ctx.get(&format!("/api/configs/{}", fake_id)).await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_create_config_with_all_fields() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "full-config",
        "description": "A full config",
        "image": "kasmweb/desktop:1.19.0",
        "cores": 4,
        "memory": 8589934592_i64,
        "gpu_count": 1,
        "docker_registry": "https://myregistry.com",
        "persistent_storage_path": "/data/{workspace_name}/{user_id}",
        "run_config": { "ports": [8080] },
        "exec_config": { "command": ["/bin/sh"] },
        "volume_mappings": { "/host": "/container" }
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["config"]["name"], "full-config");
    assert_eq!(body["config"]["cores"], 4);
    assert_eq!(body["config"]["gpu_count"], 1);
}

#[tokio::test]
async fn test_context_helpers() {
    let ctx = TestContext::new().await;
    let _ = ctx.login_user("admin", "admin").await;
    let _ = ctx.login_token().await;
}

#[tokio::test]
async fn test_create_config_with_defaults() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "defaults-config"
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["config"]["image"], "kasmweb/desktop:1.19.0-rolling-daily");
    assert_eq!(body["config"]["cores"], 2);
    assert_eq!(body["config"]["memory"], 4_294_967_296_i64);
}

#[tokio::test]
async fn test_list_configs_as_non_admin() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    ctx.post("/api/configs", &serde_json::json!({
        "name": "admin-owned-config",
        "image": "busybox:1"
    })).await;

    ctx.post("/api/auth/register", &serde_json::json!({
        "username": "cfglist_nonadmin",
        "password": "pass123"
    })).await;
    ctx.login_user("cfglist_nonadmin", "pass123").await;

    let resp = ctx.get("/api/configs").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let configs = body["configs"].as_array().unwrap();
    assert!(configs.is_empty(), "non-admin should not see admin's configs");
}

#[tokio::test]
async fn test_create_config_with_null_optional_fields() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "null-fields-config",
        "image": "busybox:1",
        "cores": 2,
        "memory": 4294967296_i64,
        "run_config": null,
        "exec_config": null,
        "volume_mappings": null
    })).await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["config"]["run_config"], serde_json::json!({}));
    assert_eq!(body["config"]["exec_config"], serde_json::json!({}));
    assert_eq!(body["config"]["volume_mappings"], serde_json::json!({}));
}
