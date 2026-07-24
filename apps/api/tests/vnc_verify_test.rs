mod common;

use common::TestContext;
use sea_orm::ActiveModelTrait;
use sea_orm::Set;
use openworkspace_api::db::workspace_instance;

#[tokio::test]
async fn test_vnc_verify_no_cookie() {
    let ctx = TestContext::new().await;
    let resp = ctx
        .client
        .get(format!("{}/api/vnc/verify", ctx.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_vnc_verify_invalid_token() {
    let ctx = TestContext::new().await;
    let resp = ctx
        .client
        .get(format!("{}/api/vnc/verify", ctx.base_url))
        .header("Cookie", "ow_token=invalid.token.here")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_vnc_verify_no_forwarded_uri() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let token = ctx.login_token().await;
    let resp = ctx
        .client
        .get(format!("{}/api/vnc/verify", ctx.base_url))
        .header("Cookie", format!("ow_token={}", token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_vnc_verify_bad_uri_format() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let token = ctx.login_token().await;
    let resp = ctx
        .client
        .get(format!("{}/api/vnc/verify", ctx.base_url))
        .header("Cookie", format!("ow_token={}", token))
        .header("X-Forwarded-Uri", "not-a-valid-websockify-path")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_vnc_verify_unknown_token_not_in_db() {
    let ctx = TestContext::new().await;

    let token = ctx.login_token().await;
    let resp = ctx
        .client
        .get(format!("{}/api/vnc/verify", ctx.base_url))
        .header("Cookie", format!("ow_token={}", token))
        .header("X-Forwarded-Uri", "/vnc/nonexistent-token/websockify")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_context_helpers() {
    let ctx = TestContext::new().await;
    let _ = ctx.login_user("admin", "admin").await;
    let _ = ctx.get("/health").await;
    let _ = ctx.post("/health", &serde_json::json!({})).await;
    let _ = ctx.put("/health", &serde_json::json!({})).await;
    let _ = ctx.delete("/health").await;
}

#[tokio::test]
async fn test_vnc_verify_db_hit_running() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let config_resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "vnc-verify-test",
        "image": "busybox:1"
    })).await;
    let config_id = config_resp.json::<serde_json::Value>().await.unwrap()["config"]["id"].as_str().unwrap().to_string();

    let launch_resp = ctx.post("/api/instances", &serde_json::json!({
        "config_id": config_id
    })).await;
    let launch_body: serde_json::Value = launch_resp.json().await.unwrap();
    let instance_id = launch_body["instance"]["id"].as_str().unwrap();
    let vnc_token = launch_body["instance"]["vnc_token"].as_str().unwrap().to_string();

    // Update instance status to "running" via DB
    let inst_id = uuid::Uuid::parse_str(instance_id).unwrap();
    let db_url = common::pg_url(&ctx.db_name);
    let db = sea_orm::Database::connect(&db_url).await.unwrap();
    let model = workspace_instance::ActiveModel {
        id: Set(inst_id),
        status: Set("running".to_string()),
        ..Default::default()
    };
    model.update(&db).await.unwrap();

    let token = ctx.login_token().await;
    let resp = ctx
        .client
        .get(format!("{}/api/vnc/verify", ctx.base_url))
        .header("Cookie", format!("ow_token={}", token))
        .header("X-Forwarded-Uri", format!("/vnc/{}/websockify", vnc_token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    assert!(resp.headers().contains_key("x-forwarded-user"));
    assert!(resp.headers().contains_key("x-forwarded-role"));
}

#[tokio::test]
async fn test_vnc_verify_db_hit_not_running() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let config_resp = ctx.post("/api/configs", &serde_json::json!({
        "name": "vnc-verify-stopped",
        "image": "busybox:1"
    })).await;
    let config_id = config_resp.json::<serde_json::Value>().await.unwrap()["config"]["id"].as_str().unwrap().to_string();

    let launch_resp = ctx.post("/api/instances", &serde_json::json!({
        "config_id": config_id
    })).await;
    let launch_body: serde_json::Value = launch_resp.json().await.unwrap();
    let vnc_token = launch_body["instance"]["vnc_token"].as_str().unwrap();

    // Instance is "stopped" by default
    let token = ctx.login_token().await;
    let resp = ctx
        .client
        .get(format!("{}/api/vnc/verify", ctx.base_url))
        .header("Cookie", format!("ow_token={}", token))
        .header("X-Forwarded-Uri", format!("/vnc/{}/websockify", vnc_token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}
