mod common;

use common::TestContext;

#[tokio::test]
async fn test_get_registry_url_requires_auth() {
    let ctx = TestContext::new().await;
    let resp = ctx.get("/api/registry/url").await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_get_registry_url_empty() {
    let ctx = TestContext::new().await;
    let _ = ctx.login_admin().await;
    let resp = ctx.get("/api/registry/url").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["url"].is_null());
}

#[tokio::test]
async fn test_set_and_get_registry_url() {
    let ctx = TestContext::new().await;
    let _ = ctx.login_admin().await;

    let resp = ctx
        .put(
            "/api/registry/url",
            &serde_json::json!({ "url": "https://example.com/registry.json" }),
        )
        .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        body["url"].as_str().unwrap(),
        "https://example.com/registry.json"
    );

    let resp = ctx.get("/api/registry/url").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        body["url"].as_str().unwrap(),
        "https://example.com/registry.json"
    );
}

#[tokio::test]
async fn test_set_registry_url_requires_admin() {
    let ctx = TestContext::new().await;
    ctx.login_user("user1", "wrong").await;
    let resp = ctx
        .put(
            "/api/registry/url",
            &serde_json::json!({ "url": "https://example.com" }),
        )
        .await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_get_registry_no_cache() {
    let ctx = TestContext::new().await;
    let _ = ctx.login_admin().await;
    let resp = ctx.get("/api/registry").await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_set_cached_and_get_registry() {
    let ctx = TestContext::new().await;
    let _ = ctx.login_admin().await;

    let data = serde_json::json!({ "workspaces": [{"name": "test"}] });
    let db_url = common::pg_url(&ctx.db_name);
    let db = sea_orm::Database::connect(&db_url).await.unwrap();
    let repo = openworkspace_api::db::RegistryRepository::new(&db);
    repo.set_cached(&data).await.unwrap();

    let resp = ctx.get("/api/registry").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["workspaces"][0]["name"].as_str().unwrap(), "test");
}

#[tokio::test]
async fn test_sync_registry_no_url() {
    let ctx = TestContext::new().await;
    let _ = ctx.login_admin().await;
    let resp = ctx.post("/api/registry/sync", &serde_json::json!({})).await;
    assert_eq!(resp.status(), 400);
}

#[tokio::test]
async fn test_sync_registry_requires_admin() {
    let ctx = TestContext::new().await;
    ctx.login_user("user1", "wrong").await;
    let resp = ctx.post("/api/registry/sync", &serde_json::json!({})).await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_context_helpers() {
    let ctx = TestContext::new().await;
    let _ = ctx.delete("/health").await;
    let _ = ctx.login_token().await;
}

#[tokio::test]
async fn test_sync_registry_forbidden_for_non_admin() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;
    ctx.post("/api/auth/register", &serde_json::json!({
        "username": "reg_nonadmin_sync",
        "password": "pass123"
    })).await;
    ctx.login_user("reg_nonadmin_sync", "pass123").await;

    let resp = ctx.post("/api/registry/sync", &serde_json::json!({})).await;
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn test_get_registry_url_forbidden_for_non_admin() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;
    ctx.post("/api/auth/register", &serde_json::json!({
        "username": "reg_nonadmin_get",
        "password": "pass123"
    })).await;
    ctx.login_user("reg_nonadmin_get", "pass123").await;

    let resp = ctx.get("/api/registry/url").await;
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn test_set_registry_url_forbidden_for_non_admin() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;
    ctx.post("/api/auth/register", &serde_json::json!({
        "username": "reg_nonadmin_set",
        "password": "pass123"
    })).await;
    ctx.login_user("reg_nonadmin_set", "pass123").await;

    let resp = ctx.put("/api/registry/url", &serde_json::json!({ "url": "https://example.com" })).await;
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn test_sync_registry_fetch_failure() {
    let ctx = TestContext::new().await;
    let _ = ctx.login_admin().await;

    ctx.put("/api/registry/url", &serde_json::json!({ "url": "http://127.0.0.1:1" })).await;

    let resp = ctx.post("/api/registry/sync", &serde_json::json!({})).await;
    assert_eq!(resp.status(), 502);
}

#[tokio::test]
async fn test_sync_registry_bad_json() {
    let ctx = TestContext::new().await;
    let _ = ctx.login_admin().await;

    // Start a tiny mock server returning non-JSON
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            if let Ok((mut stream, _)) = listener.accept().await {
                let response = "HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\nhello";
                use tokio::io::AsyncWriteExt;
                let _ = stream.write_all(response.as_bytes()).await;
            }
        }
    });

    ctx.put("/api/registry/url", &serde_json::json!({ "url": format!("http://127.0.0.1:{}", addr.port()) })).await;

    let resp = ctx.post("/api/registry/sync", &serde_json::json!({})).await;
    assert_eq!(resp.status(), 502);
}

#[tokio::test]
async fn test_sync_registry_happy_path() {
    let ctx = TestContext::new().await;
    let _ = ctx.login_admin().await;

    let registry_data = serde_json::json!({
        "workspaces": [{"name": "synced_ws", "image": "ubuntu:22.04"}]
    });

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let data = registry_data.clone();
    tokio::spawn(async move {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        loop {
            if let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = vec![0u8; 1024];
                let _ = stream.read(&mut buf).await;
                let body = serde_json::to_string(&data).unwrap();
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = stream.write_all(response.as_bytes()).await;
            }
        }
    });

    ctx.put("/api/registry/url", &serde_json::json!({ "url": format!("http://127.0.0.1:{}", addr.port()) })).await;

    let resp = ctx.post("/api/registry/sync", &serde_json::json!({})).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["workspaces"][0]["name"], "synced_ws");

    // Verify cache was set
    let resp = ctx.get("/api/registry").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["workspaces"][0]["name"], "synced_ws");
}
