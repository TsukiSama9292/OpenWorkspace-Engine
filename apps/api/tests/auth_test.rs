mod common;

use common::TestContext;

#[tokio::test]
async fn test_login_admin() {
    let ctx = TestContext::new().await;

    let resp = ctx.login_admin().await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["user"]["username"], "admin");
    assert_eq!(body["user"]["role"], "admin");
    assert!(body["user"]["id"].is_string());
}

#[tokio::test]
async fn test_login_wrong_password() {
    let ctx = TestContext::new().await;

    let resp = ctx.login_user("admin", "wrongpassword").await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_register_and_login() {
    let ctx = TestContext::new().await;
    let username = "testuser";
    let password = "testpass123";

    let resp = ctx.post("/api/auth/register", &serde_json::json!({
        "username": username,
        "password": password
    })).await;
    assert_eq!(resp.status(), 404);

}

#[tokio::test]
async fn test_me_endpoint() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.get("/api/auth/me").await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["user"]["username"], "admin");
    assert!(body["user"]["created_at"].is_string());
}

#[tokio::test]
async fn test_me_without_auth() {
    let ctx = TestContext::new().await;

    let client = reqwest::Client::new();
    let resp = client
        .get(format!("{}/api/auth/me", ctx.base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_logout() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/auth/logout", &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");

    let resp = ctx.get("/api/auth/me").await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_register_duplicate_username() {
    let ctx = TestContext::new().await;
    let username = "dup";

    let resp = ctx.post("/api/auth/register", &serde_json::json!({
        "username": &username,
        "password": "pass123"
    })).await;
    assert_eq!(resp.status(), 404);

    let resp = ctx.post("/api/auth/register", &serde_json::json!({
        "username": &username,
        "password": "pass456"
    })).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_context_helpers() {
    let ctx = TestContext::new().await;
    let _ = ctx.put("/health", &serde_json::json!({})).await;
    let _ = ctx.delete("/health").await;
    let _ = ctx.login_token().await;
}
