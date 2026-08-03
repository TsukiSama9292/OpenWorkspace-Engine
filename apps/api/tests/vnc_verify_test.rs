mod common;

use common::TestContext;

// Creates an instance row directly in the DB (bypassing the launch API so the
// VNC cache stays empty). This exercises vnc_verify's DB-hit path: the cache
// is checked first, and a launch would have seeded it with status "starting".
async fn create_db_instance(ctx: &TestContext, template_name: &str, status: &str) -> String {
    let config_resp = ctx.post("/api/templates", &serde_json::json!({
        "name": template_name,
        "image": "busybox:1"
    })).await;
    let template_id = config_resp.json::<serde_json::Value>().await.unwrap()["template"]["id"].as_str().unwrap().to_string();

    let db_url = common::pg_url(&ctx.db_name);
    let db = sea_orm::Database::connect(&db_url).await.unwrap();
    let admin_id = openworkspace_api::db::UserRepository::new(&db)
        .find_by_username("admin")
        .await
        .unwrap()
        .expect("admin user missing")
        .id;
    let repo = openworkspace_api::db::WorkspaceInstanceRepository::new(&db);
    let instance = repo
        .launch(
            uuid::Uuid::parse_str(&template_id).unwrap(),
            admin_id,
            template_name,
            false,
            None,
        )
        .await
        .unwrap();
    repo.update_status(instance.id, status).await.unwrap();
    instance.access_token
}

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
        .header("X-Forwarded-Uri", "/kasmvnc/nonexistent-token/websockify")
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

    let access_token = create_db_instance(&ctx, "vnc-verify-test", "running").await;

    let token = ctx.login_token().await;
    let resp = ctx
        .client
        .get(format!("{}/api/vnc/verify", ctx.base_url))
        .header("Cookie", format!("ow_token={}", token))
        .header("X-Forwarded-Uri", format!("/kasmvnc/{}/websockify", access_token))
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

    let access_token = create_db_instance(&ctx, "vnc-verify-stopped", "stopped").await;

    let token = ctx.login_token().await;
    let resp = ctx
        .client
        .get(format!("{}/api/vnc/verify", ctx.base_url))
        .header("Cookie", format!("ow_token={}", token))
        .header("X-Forwarded-Uri", format!("/kasmvnc/{}/websockify", access_token))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 404);
}
