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

const USER_DEFAULT_RAM: u64 = 8 * 1024 * 1024 * 1024;
const MANAGER_DEFAULT_RAM: u64 = 32 * 1024 * 1024 * 1024;

async fn create_user_via_admin(ctx: &TestContext, username: &str, role: &str) -> String {
    let resp = ctx
        .post("/api/users", &serde_json::json!({
            "username": username,
            "password": "pass123",
            "role": role,
        }))
        .await;
    assert_eq!(resp.status(), 200);
    resp.json::<serde_json::Value>().await.unwrap()["user"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn test_user_role_defaults_effective_quota() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let user_id = create_user_via_admin(&ctx, "default_quota_user", "user").await;
    let resp = ctx.get(&format!("/api/users/{}", user_id)).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let user = &body["user"];
    assert_eq!(user["instance_limit"], serde_json::Value::Null);
    assert_eq!(user["max_cpu_cores"], serde_json::Value::Null);
    assert_eq!(user["max_ram_bytes"], serde_json::Value::Null);
    assert_eq!(user["effective_instance_limit"], 2);
    assert_eq!(user["effective_max_cpu_cores"], 4);
    assert_eq!(
        user["effective_max_ram_bytes"].as_u64().unwrap(),
        USER_DEFAULT_RAM
    );
    assert_eq!(user["quota_exempt"], false);
}

#[tokio::test]
async fn test_manager_role_defaults_effective_quota() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let manager_id = create_user_via_admin(&ctx, "default_quota_manager", "manager").await;
    let resp = ctx.get(&format!("/api/users/{}", manager_id)).await;
    assert_eq!(resp.status(), 200);
    let user = &resp.json::<serde_json::Value>().await.unwrap()["user"];
    assert_eq!(user["effective_instance_limit"], 5);
    assert_eq!(user["effective_max_cpu_cores"], 12);
    assert_eq!(
        user["effective_max_ram_bytes"].as_u64().unwrap(),
        MANAGER_DEFAULT_RAM
    );
    assert_eq!(user["quota_exempt"], false);
}

#[tokio::test]
async fn test_admin_is_quota_exempt() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.get("/api/users").await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let admin = body["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["username"] == "admin")
        .unwrap();
    assert_eq!(admin["quota_exempt"], true);
}

#[tokio::test]
async fn test_admin_sets_quota_overrides() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let user_id = create_user_via_admin(&ctx, "override_user", "user").await;

    let resp = ctx
        .put(
            &format!("/api/users/{}", user_id),
            &serde_json::json!({
                "instance_limit": 7,
                "max_cpu_cores": 9,
                "max_ram_bytes": 10737418240_i64,
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "update failed: {:?}", resp.text().await);
    let user = &resp.json::<serde_json::Value>().await.unwrap()["user"];
    assert_eq!(user["instance_limit"], 7);
    assert_eq!(user["max_cpu_cores"], 9);
    assert_eq!(user["max_ram_bytes"].as_u64().unwrap(), 10737418240);
    assert_eq!(user["effective_instance_limit"], 7);
    assert_eq!(user["effective_max_cpu_cores"], 9);
    assert_eq!(user["effective_max_ram_bytes"].as_u64().unwrap(), 10737418240);
    assert_eq!(user["quota_exempt"], false);

    // Persisted: a fresh GET reflects the override.
    let resp = ctx.get(&format!("/api/users/{}", user_id)).await;
    let user = &resp.json::<serde_json::Value>().await.unwrap()["user"];
    assert_eq!(user["instance_limit"], 7);
    assert_eq!(user["max_ram_bytes"].as_u64().unwrap(), 10737418240);
}

#[tokio::test]
async fn test_null_restores_role_default_quota() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let user_id = create_user_via_admin(&ctx, "null_restore_user", "user").await;

    ctx.put(
        &format!("/api/users/{}", user_id),
        &serde_json::json!({
            "instance_limit": 7,
            "max_cpu_cores": 9,
            "max_ram_bytes": 10737418240_i64,
        }),
    )
    .await;

    let resp = ctx
        .put(
            &format!("/api/users/{}", user_id),
            &serde_json::json!({
                "instance_limit": null,
                "max_cpu_cores": null,
                "max_ram_bytes": null,
            }),
        )
        .await;
    assert_eq!(resp.status(), 200, "restore failed: {:?}", resp.text().await);
    let user = &resp.json::<serde_json::Value>().await.unwrap()["user"];
    assert_eq!(user["instance_limit"], serde_json::Value::Null);
    assert_eq!(user["max_cpu_cores"], serde_json::Value::Null);
    assert_eq!(user["max_ram_bytes"], serde_json::Value::Null);
    assert_eq!(user["effective_instance_limit"], 2);
    assert_eq!(user["effective_max_cpu_cores"], 4);
    assert_eq!(
        user["effective_max_ram_bytes"].as_u64().unwrap(),
        USER_DEFAULT_RAM
    );
}

#[tokio::test]
async fn test_regular_user_cannot_set_own_quota() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let user_id = create_user_via_admin(&ctx, "cannot_set_quota", "user").await;
    ctx.login_user("cannot_set_quota", "pass123").await;

    let resp = ctx
        .put(
            &format!("/api/users/{}", user_id),
            &serde_json::json!({ "instance_limit": 10 }),
        )
        .await;
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn test_manager_cannot_set_quota() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    create_user_via_admin(&ctx, "quota_manager", "manager").await;
    let target_id = create_user_via_admin(&ctx, "quota_target", "user").await;

    ctx.login_user("quota_manager", "pass123").await;

    let resp = ctx
        .put(
            &format!("/api/users/{}", target_id),
            &serde_json::json!({ "max_cpu_cores": 6 }),
        )
        .await;
    assert_eq!(resp.status(), 403);

    // The manager can still manage the user (name/role) — just not quotas.
    let resp = ctx
        .put(
            &format!("/api/users/{}", target_id),
            &serde_json::json!({ "role": "user" }),
        )
        .await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_quota_override_partial_update_keeps_other_columns() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let user_id = create_user_via_admin(&ctx, "partial_quota_user", "user").await;

    ctx.put(
        &format!("/api/users/{}", user_id),
        &serde_json::json!({
            "instance_limit": 7,
            "max_cpu_cores": 9,
            "max_ram_bytes": 10737418240_i64,
        }),
    )
    .await;

    // Update only instance_limit: the other overrides must survive.
    let resp = ctx
        .put(
            &format!("/api/users/{}", user_id),
            &serde_json::json!({ "instance_limit": 3 }),
        )
        .await;
    assert_eq!(resp.status(), 200);
    let user = &resp.json::<serde_json::Value>().await.unwrap()["user"];
    assert_eq!(user["instance_limit"], 3);
    assert_eq!(user["max_cpu_cores"], 9);
    assert_eq!(user["max_ram_bytes"].as_u64().unwrap(), 10737418240);
    assert_eq!(user["effective_instance_limit"], 3);
    assert_eq!(user["effective_max_cpu_cores"], 9);
}

#[tokio::test]
async fn test_list_users_includes_effective_quota() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.get("/api/users").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let users = body["users"].as_array().unwrap();
    assert!(!users.is_empty());
    for u in users {
        assert!(u.get("effective_instance_limit").is_some());
        assert!(u.get("effective_max_cpu_cores").is_some());
        assert!(u.get("effective_max_ram_bytes").is_some());
        assert!(u.get("quota_exempt").is_some());
    }
}
