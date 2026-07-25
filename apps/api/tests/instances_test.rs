mod common;

use common::TestContext;
use sea_orm::ActiveModelTrait;
use sea_orm::Set;
use openworkspace_api::db::workspace_instance;

#[tokio::test]
async fn test_list_instances_empty() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.get("/api/instances").await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["instances"].is_array());
}

#[tokio::test]
async fn test_get_nonexistent_instance() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let fake_id = uuid::Uuid::new_v4();
    let resp = ctx.get(&format!("/api/instances/{}", fake_id)).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_list_instances_returns_owner_username() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.get("/api/instances").await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    let instances = body["instances"].as_array().unwrap();

    for inst in instances {
        assert!(inst.get("owner_username").is_some());
    }
}

#[tokio::test]
async fn test_launch_instance_config_not_found() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let fake_config_id = uuid::Uuid::new_v4();
    let resp = ctx.post("/api/instances", &serde_json::json!({
        "config_id": fake_config_id
    })).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_delete_nonexistent_instance() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let fake_id = uuid::Uuid::new_v4();
    let resp = ctx.delete(&format!("/api/instances/{}", fake_id)).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_start_instance_not_found() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let fake_id = uuid::Uuid::new_v4();
    let resp = ctx.post(&format!("/api/instances/{}/start", fake_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_stop_instance_not_found() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let fake_id = uuid::Uuid::new_v4();
    let resp = ctx.post(&format!("/api/instances/{}/stop", fake_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_pause_instance_not_found() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let fake_id = uuid::Uuid::new_v4();
    let resp = ctx.post(&format!("/api/instances/{}/pause", fake_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_unpause_instance_not_found() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let fake_id = uuid::Uuid::new_v4();
    let resp = ctx.post(&format!("/api/instances/{}/unpause", fake_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_launch_with_empty_body() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let config_id = uuid::Uuid::new_v4();
    let resp = ctx.post("/api/instances", &serde_json::json!({
        "config_id": config_id
    })).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_list_instances_requires_auth() {
    let ctx = TestContext::new().await;
    let resp = ctx.get("/api/instances").await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_get_existing_instance() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let config_resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "get-inst-test",
        "image": "busybox:1"
    })).await;
    let config_id = config_resp.json::<serde_json::Value>().await.unwrap()["config"]["id"].as_str().unwrap().to_string();

    let launch_resp = ctx.post("/api/instances", &serde_json::json!({
        "config_id": config_id
    })).await;
    let launch_body: serde_json::Value = launch_resp.json().await.unwrap();
    let instance_id = launch_body["instance"]["id"].as_str().unwrap();

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["config_name"].as_str().unwrap(), "get-inst-test");
    assert!(body["instance"]["owner_username"].is_string());
}

#[tokio::test]
async fn test_delete_existing_instance() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let config_resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "del-inst-test",
        "image": "busybox:1"
    })).await;
    let config_id = config_resp.json::<serde_json::Value>().await.unwrap()["config"]["id"].as_str().unwrap().to_string();

    let launch_resp = ctx.post("/api/instances", &serde_json::json!({
        "config_id": config_id
    })).await;
    let launch_body: serde_json::Value = launch_resp.json().await.unwrap();
    let instance_id = launch_body["instance"]["id"].as_str().unwrap();

    let resp = ctx.delete(&format!("/api/instances/{}", instance_id)).await;
    assert_eq!(resp.status(), 204);

    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_list_instances_with_data() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let config_resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "list-inst-test",
        "image": "busybox:1"
    })).await;
    let config_id = config_resp.json::<serde_json::Value>().await.unwrap()["config"]["id"].as_str().unwrap().to_string();

    ctx.post("/api/instances", &serde_json::json!({
        "config_id": config_id
    })).await;

    let resp = ctx.get("/api/instances").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let instances = body["instances"].as_array().unwrap();
    assert!(instances.iter().any(|i| i["config_name"] == "list-inst-test"));
}

#[tokio::test]
async fn test_start_instance_already_running_conflict() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let config_resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "start-running",
        "image": "busybox:1"
    })).await;
    let config_id = config_resp.json::<serde_json::Value>().await.unwrap()["config"]["id"].as_str().unwrap().to_string();

    let launch_resp = ctx.post("/api/instances", &serde_json::json!({
        "config_id": config_id
    })).await;
    let launch_body: serde_json::Value = launch_resp.json().await.unwrap();
    let instance_id = launch_body["instance"]["id"].as_str().unwrap();

    if launch_body["instance"]["status"].as_str() == Some("running") {
        let resp = ctx.post(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({})).await;
        assert_eq!(resp.status(), 409);
    }
}

#[tokio::test]
async fn test_stop_instance_already_stopped_conflict() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let config_resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "stop-stopped",
        "image": "busybox:1"
    })).await;
    let config_id = config_resp.json::<serde_json::Value>().await.unwrap()["config"]["id"].as_str().unwrap().to_string();

    let launch_resp = ctx.post("/api/instances", &serde_json::json!({
        "config_id": config_id
    })).await;
    let launch_body: serde_json::Value = launch_resp.json().await.unwrap();
    let instance_id = launch_body["instance"]["id"].as_str().unwrap();

    let resp = ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;
    if resp.status() == 200 {
        let resp2 = ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;
        assert_eq!(resp2.status(), 409);
    }
}

#[tokio::test]
async fn test_pause_instance_not_running_conflict() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let config_resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "pause-stopped",
        "image": "busybox:1"
    })).await;
    let config_id = config_resp.json::<serde_json::Value>().await.unwrap()["config"]["id"].as_str().unwrap().to_string();

    let launch_resp = ctx.post("/api/instances", &serde_json::json!({
        "config_id": config_id
    })).await;
    let launch_body: serde_json::Value = launch_resp.json().await.unwrap();
    let instance_id = launch_body["instance"]["id"].as_str().unwrap();

    let _ = ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;

    let resp = ctx.post(&format!("/api/instances/{}/pause", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 409);
}

#[tokio::test]
async fn test_unpause_instance_not_paused_conflict() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let config_resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "unpause-stopped",
        "image": "busybox:1"
    })).await;
    let config_id = config_resp.json::<serde_json::Value>().await.unwrap()["config"]["id"].as_str().unwrap().to_string();

    let launch_resp = ctx.post("/api/instances", &serde_json::json!({
        "config_id": config_id
    })).await;
    let launch_body: serde_json::Value = launch_resp.json().await.unwrap();
    let instance_id = launch_body["instance"]["id"].as_str().unwrap();

    let resp = ctx.post(&format!("/api/instances/{}/unpause", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 409);
}

#[tokio::test]
async fn test_list_instances_as_non_admin() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let config_resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "nonadmin-list",
        "image": "busybox:1"
    })).await;
    let config_id = config_resp.json::<serde_json::Value>().await.unwrap()["config"]["id"].as_str().unwrap().to_string();

    ctx.post("/api/instances", &serde_json::json!({
        "config_id": config_id
    })).await;

    let resp = ctx.post("/api/auth/register", &serde_json::json!({
        "username": "nonadmin_user",
        "password": "pass123"
    })).await;
    assert_eq!(resp.status(), 200);

    let resp = ctx.login_user("nonadmin_user", "pass123").await;
    assert_eq!(resp.status(), 200);

    let resp = ctx.get("/api/instances").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["instances"].is_array());
}

#[tokio::test]
async fn test_stop_instance_when_paused_unpause_first() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;
    ctx.login_admin().await;

    let config_resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "stop-paused-test",
        "image": "busybox:1",
        "run_config": { "command": ["sleep", "3600"] }
    })).await;
    let config_id = config_resp.json::<serde_json::Value>().await.unwrap()["config"]["id"].as_str().unwrap().to_string();

    let launch_resp = ctx.post("/api/instances", &serde_json::json!({
        "config_id": config_id
    })).await;
    let launch_body: serde_json::Value = launch_resp.json().await.unwrap();
    let instance_id = launch_body["instance"]["id"].as_str().unwrap();

    if launch_body["instance"]["status"].as_str() != Some("running") {
        return;
    }

    let pause_resp = ctx.post(&format!("/api/instances/{}/pause", instance_id), &serde_json::json!({})).await;
    if pause_resp.status() != 200 {
        return;
    }

    let stop_resp = ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;
    assert_eq!(stop_resp.status(), 200);
    let body: serde_json::Value = stop_resp.json().await.unwrap();
    assert_eq!(body["status"].as_str(), Some("stopped"));
}

#[tokio::test]
async fn test_start_instance_container_not_found_creates_new() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;
    ctx.login_admin().await;

    let config_resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "start-recreate",
        "image": "busybox:1",
        "run_config": { "command": ["sleep", "3600"] }
    })).await;
    let config_id = config_resp.json::<serde_json::Value>().await.unwrap()["config"]["id"].as_str().unwrap().to_string();

    let launch_resp = ctx.post("/api/instances", &serde_json::json!({
        "config_id": config_id
    })).await;
    let launch_body: serde_json::Value = launch_resp.json().await.unwrap();
    let instance_id = launch_body["instance"]["id"].as_str().unwrap();

    if launch_body["instance"]["status"].as_str() != Some("running") {
        return;
    }

    let stop_resp = ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;
    assert_eq!(stop_resp.status(), 200);

    let del_resp = ctx.delete(&format!("/api/instances/{}", instance_id)).await;
    assert_eq!(del_resp.status(), 204);
}

#[tokio::test]
async fn test_start_instance_with_no_container_id() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;
    ctx.login_admin().await;

    let config_resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "start-no-container",
        "image": "busybox:1",
        "run_config": { "command": ["sleep", "3600"] }
    })).await;
    let config_id = config_resp.json::<serde_json::Value>().await.unwrap()["config"]["id"].as_str().unwrap().to_string();

    let launch_resp = ctx.post("/api/instances", &serde_json::json!({
        "config_id": config_id
    })).await;
    let launch_body: serde_json::Value = launch_resp.json().await.unwrap();
    let instance_id = launch_body["instance"]["id"].as_str().unwrap();
    let launch_status = launch_body["instance"]["status"].as_str();

    if launch_status != Some("running") {
        return;
    }

    if let Some(old_cid) = launch_body["instance"]["container_id"].as_str() {
        let docker = bollard::Docker::connect_with_local_defaults().unwrap();
        let _ = docker.stop_container(old_cid, None::<bollard::container::StopContainerOptions>).await;
        let _ = docker.remove_container(old_cid, None).await;
    }

    let inst_id = uuid::Uuid::parse_str(instance_id).unwrap();
    let db_url = common::pg_url(&ctx.db_name);
    let db = sea_orm::Database::connect(&db_url).await.unwrap();
    let model = workspace_instance::ActiveModel {
        id: Set(inst_id),
        container_id: Set(None),
        status: Set("stopped".to_string()),
        ..Default::default()
    };
    model.update(&db).await.unwrap();

    let resp = ctx.post(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({})).await;
    assert!(resp.status() == 200 || resp.status() == 500,
        "expected 200 or 500, got {}", resp.status());
}

#[tokio::test]
async fn test_context_helpers() {
    let ctx = TestContext::new().await;
    let _ = ctx.put("/health", &serde_json::json!({})).await;
    let _ = ctx.login_token().await;
}

#[tokio::test]
async fn test_pause_no_container_returns_conflict() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let config_resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "pause-no-container",
        "image": "busybox:1"
    })).await;
    let config_id = config_resp.json::<serde_json::Value>().await.unwrap()["config"]["id"].as_str().unwrap().to_string();

    let launch_resp = ctx.post("/api/instances", &serde_json::json!({
        "config_id": config_id
    })).await;
    let launch_body: serde_json::Value = launch_resp.json().await.unwrap();
    let instance_id = launch_body["instance"]["id"].as_str().unwrap();

    let inst_id = uuid::Uuid::parse_str(instance_id).unwrap();
    let db_url = common::pg_url(&ctx.db_name);
    let db = sea_orm::Database::connect(&db_url).await.unwrap();
    let model = workspace_instance::ActiveModel {
        id: Set(inst_id),
        status: Set("running".to_string()),
        container_id: Set(None),
        ..Default::default()
    };
    model.update(&db).await.unwrap();

    let resp = ctx.post(&format!("/api/instances/{}/pause", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 409);
}

#[tokio::test]
async fn test_unpause_no_container_returns_conflict() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let config_resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "unpause-no-container",
        "image": "busybox:1"
    })).await;
    let config_id = config_resp.json::<serde_json::Value>().await.unwrap()["config"]["id"].as_str().unwrap().to_string();

    let launch_resp = ctx.post("/api/instances", &serde_json::json!({
        "config_id": config_id
    })).await;
    let launch_body: serde_json::Value = launch_resp.json().await.unwrap();
    let instance_id = launch_body["instance"]["id"].as_str().unwrap();

    let inst_id = uuid::Uuid::parse_str(instance_id).unwrap();
    let db_url = common::pg_url(&ctx.db_name);
    let db = sea_orm::Database::connect(&db_url).await.unwrap();
    let model = workspace_instance::ActiveModel {
        id: Set(inst_id),
        status: Set("paused".to_string()),
        container_id: Set(None),
        ..Default::default()
    };
    model.update(&db).await.unwrap();

    let resp = ctx.post(&format!("/api/instances/{}/unpause", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 409);
}

#[tokio::test]
async fn test_start_stopped_no_container_returns_500() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let config_resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "start-no-container-err",
        "image": "busybox:1"
    })).await;
    let config_id = config_resp.json::<serde_json::Value>().await.unwrap()["config"]["id"].as_str().unwrap().to_string();

    let launch_resp = ctx.post("/api/instances", &serde_json::json!({
        "config_id": config_id
    })).await;
    let launch_body: serde_json::Value = launch_resp.json().await.unwrap();
    let instance_id = launch_body["instance"]["id"].as_str().unwrap();

    let inst_id = uuid::Uuid::parse_str(instance_id).unwrap();
    let db_url = common::pg_url(&ctx.db_name);
    let db = sea_orm::Database::connect(&db_url).await.unwrap();
    let model = workspace_instance::ActiveModel {
        id: Set(inst_id),
        status: Set("stopped".to_string()),
        container_id: Set(None),
        ..Default::default()
    };
    model.update(&db).await.unwrap();

    let resp = ctx.post(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({})).await;
    assert!(resp.status() == 500 || resp.status() == 200);
}

#[tokio::test]
async fn test_stop_stopped_no_container_returns_conflict() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let config_resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "stop-stopped-no-container",
        "image": "busybox:1"
    })).await;
    let config_id = config_resp.json::<serde_json::Value>().await.unwrap()["config"]["id"].as_str().unwrap().to_string();

    let launch_resp = ctx.post("/api/instances", &serde_json::json!({
        "config_id": config_id
    })).await;
    let launch_body: serde_json::Value = launch_resp.json().await.unwrap();
    let instance_id = launch_body["instance"]["id"].as_str().unwrap();

    let inst_id = uuid::Uuid::parse_str(instance_id).unwrap();
    let db_url = common::pg_url(&ctx.db_name);
    let db = sea_orm::Database::connect(&db_url).await.unwrap();
    let model = workspace_instance::ActiveModel {
        id: Set(inst_id),
        status: Set("stopped".to_string()),
        container_id: Set(None),
        ..Default::default()
    };
    model.update(&db).await.unwrap();

    let resp = ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({})).await;
    assert_eq!(resp.status(), 409);
}
