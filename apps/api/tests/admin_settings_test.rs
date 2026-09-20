mod common;

use common::TestContext;

async fn create_user(ctx: &TestContext, username: &str) {
    let resp = ctx
        .post("/api/users", &serde_json::json!({ "username": username, "password": "pass123" }))
        .await;
    assert_eq!(resp.status(), 200);
}

fn settings_body(host_instance_limit: i32) -> serde_json::Value {
    serde_json::json!({
        "host_instance_limit": host_instance_limit,
        "host_cpu_cores": 0,
        "host_memory_mb": 0,
        "host_gpu_count": 0,
    })
}

#[tokio::test]
async fn test_get_settings_requires_auth() {
    let ctx = TestContext::new().await;
    let resp = ctx.get("/api/admin/settings").await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_put_settings_requires_auth() {
    let ctx = TestContext::new().await;
    let resp = ctx
        .put("/api/admin/settings", &serde_json::json!({}))
        .await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_get_settings_forbidden_for_user() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;
    create_user(&ctx, "plain_user").await;
    ctx.login_user("plain_user", "pass123").await;

    let resp = ctx.get("/api/admin/settings").await;
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn test_put_settings_forbidden_for_user() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;
    create_user(&ctx, "plain_user_put").await;
    ctx.login_user("plain_user_put", "pass123").await;

    let resp = ctx
        .put("/api/admin/settings", &settings_body(0))
        .await;
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn test_get_settings_admin_returns_migration_defaults() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.get("/api/admin/settings").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let s = &body["settings"];
    assert_eq!(s["host_instance_limit"], -1);
    assert_eq!(s["host_cpu_cores"], -1);
    assert_eq!(s["host_memory_mb"], -1);
    assert_eq!(s["host_gpu_count"], -1);
}

#[tokio::test]
async fn test_put_settings_admin_updates_and_persists() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx
        .put("/api/admin/settings", &serde_json::json!({
            "host_instance_limit": 10,
            "host_cpu_cores": 16,
            "host_memory_mb": 65536,
            "host_gpu_count": 2,
        }))
        .await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let s = &body["settings"];
    assert_eq!(s["host_instance_limit"], 10);
    assert_eq!(s["host_cpu_cores"], 16);
    assert_eq!(s["host_memory_mb"], 65536);
    assert_eq!(s["host_gpu_count"], 2);

    // Values persisted: a fresh read returns the edited row.
    let resp = ctx.get("/api/admin/settings").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let s = &body["settings"];
    assert_eq!(s["host_instance_limit"], 10);
    assert_eq!(s["host_cpu_cores"], 16);
    assert_eq!(s["host_memory_mb"], 65536);
    assert_eq!(s["host_gpu_count"], 2);
}

#[tokio::test]
async fn test_put_settings_rejects_negative_values() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let cases = [
        settings_body(-2),
        serde_json::json!({
            "host_instance_limit": 0,
            "host_cpu_cores": -2,
            "host_memory_mb": 0,
            "host_gpu_count": 0,
        }),
        serde_json::json!({
            "host_instance_limit": 0,
            "host_cpu_cores": 0,
            "host_memory_mb": -5,
            "host_gpu_count": 0,
        }),
        serde_json::json!({
            "host_instance_limit": 0,
            "host_cpu_cores": 0,
            "host_memory_mb": 0,
            "host_gpu_count": -3,
        }),
    ];
    for body in cases {
        let resp = ctx.put("/api/admin/settings", &body).await;
        assert_eq!(resp.status(), 400, "expected 400 for body {}", body);
    }
}

#[tokio::test]
async fn test_put_settings_accepts_zero_values() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx
        .put("/api/admin/settings", &settings_body(0))
        .await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_context_helpers() {
    let ctx = TestContext::new().await;
    let _ = ctx.login_user("admin", "admin").await;
    let _ = ctx.put("/health", &serde_json::json!({})).await;
    let _ = ctx.login_token().await;
}
