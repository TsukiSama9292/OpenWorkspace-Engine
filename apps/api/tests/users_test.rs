mod common;

use common::TestContext;

#[tokio::test]
async fn test_list_users() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.get("/api/users").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let users = body["users"].as_array().unwrap();
    assert!(!users.is_empty());
    assert!(users.iter().any(|u| u["username"] == "admin"));
}

#[tokio::test]
async fn test_get_user() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.get("/api/users").await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let admin = body["users"].as_array().unwrap().iter().find(|u| u["username"] == "admin").unwrap();
    let admin_id = admin["id"].as_str().unwrap();

    let resp = ctx.get(&format!("/api/users/{}", admin_id)).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["user"]["username"], "admin");
}

#[tokio::test]
async fn test_get_nonexistent_user() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.get("/api/users/00000000-0000-0000-0000-000000000000").await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_delete_user_not_found() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.delete("/api/users/00000000-0000-0000-0000-000000000000").await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_delete_user_created() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    // Create a user via admin endpoint
    let resp = ctx.post("/api/users", &serde_json::json!({
        "username": "deleteme_test",
        "password": "testpass123",
    })).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let user_id = body["user"]["id"].as_str().unwrap();

    // Delete the user
    let resp = ctx.delete(&format!("/api/users/{}", user_id)).await;
    assert_eq!(resp.status(), 204);

    // Verify deleted
    let resp = ctx.get(&format!("/api/users/{}", user_id)).await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_get_user_invalid_uuid() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.get("/api/users/not-a-uuid").await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_list_users_requires_auth() {
    let ctx = TestContext::new().await;
    // Don't login
    let resp = ctx.get("/api/users").await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_context_helpers() {
    let ctx = TestContext::new().await;
    let _ = ctx.login_user("admin", "admin").await;
    let _ = ctx.put("/health", &serde_json::json!({})).await;
    let _ = ctx.login_token().await;
}

#[tokio::test]
async fn test_delete_user_forbidden_for_non_admin() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/users", &serde_json::json!({
        "username": "regular_user_del",
        "password": "pass123",
    })).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let user_id = body["user"]["id"].as_str().unwrap();

    ctx.login_user("regular_user_del", "pass123").await;

    let resp = ctx.delete(&format!("/api/users/{}", user_id)).await;
    assert_eq!(resp.status(), 403);
}
