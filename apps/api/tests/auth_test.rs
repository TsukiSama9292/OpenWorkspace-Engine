mod common;

use common::TestContext;
use openworkspace_api::db::{group, user_group};
use sea_orm::ActiveModelTrait;
use sea_orm::{ConnectionTrait, DatabaseBackend, Set, Statement};

#[tokio::test]
async fn test_login_admin() {
    let ctx = TestContext::new().await;

    let resp = ctx.login_admin().await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["context"]["username"], "admin");
    assert_eq!(body["context"]["is_admin"], true);
    assert_eq!(body["context"]["tier"], 2);
    assert!(body["context"]["user_id"].is_string());
}

#[tokio::test]
async fn test_login_context_envelope_fields() {
    let ctx = TestContext::new().await;

    let resp = ctx.login_admin().await;
    assert_eq!(resp.status(), 200);

    let body: serde_json::Value = resp.json().await.unwrap();
    let context = &body["context"];
    for key in [
        "user_id",
        "username",
        "is_admin",
        "tier",
        "can_create_template",
        "can_manage_users",
        "can_manage_group_instances",
        "can_manage_docker",
        "can_manage_registry",
        "effective_max_instances",
        "allowed_template_ids",
        "group_ids",
        "direct_max_instances",
    ] {
        assert!(context.get(key).is_some(), "missing context field: {key}");
    }
    assert_eq!(context["is_admin"], true);
    assert_eq!(context["tier"], 2);
    assert_eq!(context["can_create_template"], true);
    assert_eq!(context["can_manage_users"], true);
    assert_eq!(context["effective_max_instances"], 0);
    assert_eq!(context["allowed_template_ids"].as_array().unwrap().len(), 0);
    assert_eq!(context["group_ids"].as_array().unwrap().len(), 1, "admin holds the Admin group");
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
    assert_eq!(body["context"]["username"], "admin");
    assert_eq!(body["context"]["is_admin"], true);
    assert_eq!(body["context"]["tier"], 2);
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
async fn test_jwt_carries_no_role_claim() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;
    let token = ctx.login_token().await;

    let token_data = jsonwebtoken::decode::<serde_json::Value>(
        &token,
        &jsonwebtoken::DecodingKey::from_secret("test-secret-key-for-testing".as_bytes()),
        &jsonwebtoken::Validation::default(),
    )
    .unwrap();

    assert!(token_data.claims.get("role").is_none(), "JWT must not carry a role claim");
    assert!(token_data.claims["sub"].is_string());
    assert!(token_data.claims["exp"].is_number());
}

#[tokio::test]
async fn test_me_reflects_group_flag_flip_without_relogin() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx.post("/api/users", &serde_json::json!({
        "username": "grouper",
        "password": "pass123",
    })).await;
    assert_eq!(resp.status(), 200);
    let user_id = resp.json::<serde_json::Value>().await.unwrap()["user"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    ctx.login_user("grouper", "pass123").await;

    let body: serde_json::Value = ctx.get("/api/auth/me").await.json().await.unwrap();
    assert_eq!(body["context"]["is_admin"], false);
    assert_eq!(body["context"]["tier"], 0);
    assert_eq!(body["context"]["can_manage_docker"], false);
    // The User system group is the default membership for a created user.
    assert_eq!(body["context"]["group_ids"].as_array().unwrap().len(), 1);

    let db = sea_orm::Database::connect(&common::pg_url(&ctx.db_name))
        .await
        .unwrap();
    let user_group_id = body["context"]["group_ids"][0].as_str().unwrap().to_string();
    let group_id = uuid::Uuid::new_v4();
    group::ActiveModel {
        id: Set(group_id),
        name: Set("docker-team".to_string()),
        description: Set(None),
        can_create_template: Set(false),
        can_manage_users: Set(false),
        can_manage_group_instances: Set(false),
        can_manage_docker: Set(true),
        can_manage_registry: Set(false),
        max_instances: Set(Some(7)),
        ..Default::default()
    }
    .insert(&db)
    .await
    .unwrap();

    let user_uuid = uuid::Uuid::parse_str(&user_id).unwrap();
    user_group::ActiveModel {
        user_id: Set(user_uuid),
        group_id: Set(group_id),
    }
    .insert(&db)
    .await
    .unwrap();

    // Same cookie, no re-login: the very next /auth/me reflects the change.
    let body: serde_json::Value = ctx.get("/api/auth/me").await.json().await.unwrap();
    assert_eq!(body["context"]["can_manage_docker"], true);
    assert_eq!(body["context"]["effective_max_instances"], 7);
    let group_ids = body["context"]["group_ids"].as_array().unwrap();
    assert_eq!(group_ids.len(), 2, "User group + the custom docker-team group");
    assert!(group_ids.iter().any(|g| g == &serde_json::json!(group_id.to_string())));
    assert!(group_ids.iter().any(|g| g == &serde_json::json!(user_group_id)));

    // Flip the flag off server-side: the next request shows it again.
    db.execute(Statement::from_string(
        DatabaseBackend::Postgres,
        format!(
            "UPDATE groups SET can_manage_docker = FALSE WHERE id = '{}'",
            group_id
        ),
    ))
    .await
    .unwrap();

    let body: serde_json::Value = ctx.get("/api/auth/me").await.json().await.unwrap();
    assert_eq!(body["context"]["can_manage_docker"], false);
}

#[tokio::test]
async fn test_me_after_user_deleted_returns_401() {
    let ctx = TestContext::new().await;

    let admin = reqwest::Client::builder()
        .cookie_store(true)
        .build()
        .unwrap();
    admin
        .post(format!("{}/api/auth/login", ctx.base_url))
        .json(&serde_json::json!({ "username": "admin", "password": "admin" }))
        .send()
        .await
        .unwrap();

    let resp = admin
        .post(format!("{}/api/users", ctx.base_url))
        .json(&serde_json::json!({
            "username": "ephemeral",
            "password": "pass123",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200);
    let user_id = resp.json::<serde_json::Value>().await.unwrap()["user"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    ctx.login_user("ephemeral", "pass123").await;
    assert_eq!(ctx.get("/api/auth/me").await.status(), 200);

    let del = admin
        .delete(format!("{}/api/users/{}", ctx.base_url, user_id))
        .send()
        .await
        .unwrap();
    assert_eq!(del.status(), 204);

    // The token is still valid but the user no longer exists → 401.
    assert_eq!(ctx.get("/api/auth/me").await.status(), 401);
}

#[tokio::test]
async fn test_context_helpers() {
    let ctx = TestContext::new().await;
    let _ = ctx.put("/health", &serde_json::json!({})).await;
    let _ = ctx.delete("/health").await;
    let _ = ctx.login_token().await;
}

#[tokio::test]
async fn test_change_password_flow() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx
        .post(
            "/api/auth/change-password",
            &serde_json::json!({
                "current_password": "wrong-password",
                "new_password": "newpass123"
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    let resp = ctx
        .post(
            "/api/auth/change-password",
            &serde_json::json!({
                "current_password": "admin",
                "new_password": ""
            }),
        )
        .await;
    assert_eq!(resp.status(), 400);

    let resp = ctx
        .post(
            "/api/auth/change-password",
            &serde_json::json!({
                "current_password": "admin",
                "new_password": "newpass123"
            }),
        )
        .await;
    assert_eq!(resp.status(), 200);

    assert_eq!(ctx.login_user("admin", "admin").await.status(), 401);
    assert_eq!(ctx.login_user("admin", "newpass123").await.status(), 200);
}

#[tokio::test]
async fn test_change_password_requires_auth() {
    let ctx = TestContext::new().await;

    let resp = ctx
        .post(
            "/api/auth/change-password",
            &serde_json::json!({
                "current_password": "admin",
                "new_password": "newpass123"
            }),
        )
        .await;
    assert_eq!(resp.status(), 401);
}
