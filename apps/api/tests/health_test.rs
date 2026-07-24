mod common;

use common::TestContext;

#[tokio::test]
async fn test_health_endpoint() {
    let ctx = TestContext::new().await;

    let resp = ctx.get("/health").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn test_context_helpers() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;
    let _ = ctx.login_user("admin", "admin").await;
    let _ = ctx.post("/health", &serde_json::json!({})).await;
    let _ = ctx.put("/health", &serde_json::json!({})).await;
    let _ = ctx.delete("/health").await;
    let _ = ctx.login_token().await;
}
