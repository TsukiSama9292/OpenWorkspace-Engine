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

async fn create_user_via_admin(ctx: &TestContext, username: &str) -> String {
    let resp = ctx
        .post("/api/users", &serde_json::json!({
            "username": username,
            "password": "pass123",
        }))
        .await;
    assert_eq!(resp.status(), 200);
    resp.json::<serde_json::Value>().await.unwrap()["user"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn test_manager_can_manage_user() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let manager_id = create_user_via_admin(&ctx, "manage_manager").await;
    let target_id = create_user_via_admin(&ctx, "manage_target").await;

    // A manager is a user holding `can_manage_users` via group membership.
    let resp = ctx.post("/api/groups", &serde_json::json!({
        "name": "user-mgrs",
        "description": null,
        "can_manage_users": true,
        "max_instances": 2,
        "template_ids": [],
    })).await;
    assert_eq!(resp.status(), 200);
    let group_id = resp.json::<serde_json::Value>().await.unwrap()["group"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = ctx.put(&format!("/api/users/{}", manager_id), &serde_json::json!({
        "group_ids": [group_id],
    })).await;
    assert_eq!(resp.status(), 200);

    ctx.login_user("manage_manager", "pass123").await;

    // `can_manage_users` lets a manager update a user's name/password.
    let resp = ctx
        .put(
            &format!("/api/users/{}", target_id),
            &serde_json::json!({ "username": "manage_target_renamed" }),
        )
        .await;
    assert_eq!(resp.status(), 200, "update failed: {:?}", resp.text().await);
    assert_eq!(
        resp.json::<serde_json::Value>().await.unwrap()["user"]["username"],
        "manage_target_renamed"
    );
}

// ── Admin account & group protection (admin-protection spec) ────────────────

/// The id of a system group (kind = admin/user/manager) from GET /api/groups.
async fn system_group_id(ctx: &TestContext, kind: &str) -> String {
    let resp = ctx.get("/api/groups").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    body["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["kind"] == kind)
        .unwrap_or_else(|| panic!("system group kind={kind} must exist"))
        ["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn admin_id(ctx: &TestContext) -> String {
    let resp = ctx.get("/api/users").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    body["users"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["username"] == "admin")
        .unwrap()["id"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Promote an existing user into the Admin group directly in the DB — the API
/// forbids it by design (`can_assign_groups`), but the delete/demote guards
/// must still hold for a second Admin member.
async fn add_user_to_admin_group(ctx: &TestContext, user_id: &str, admin_group_id: &str) {
    let (client, connection) =
        tokio_postgres::connect(&common::pg_url(&ctx.db_name), tokio_postgres::NoTls)
            .await
            .expect("failed to connect to test db");
    tokio::spawn(async move {
        let _ = connection.await;
    });
    let user_uuid = uuid::Uuid::parse_str(user_id).expect("user id must parse");
    let group_uuid = uuid::Uuid::parse_str(admin_group_id).expect("group id must parse");
    client
        .execute(
            "INSERT INTO user_groups (user_id, group_id) VALUES ($1, $2)",
            &[&user_uuid, &group_uuid],
        )
        .await
        .expect("failed to promote user to Admin group");
}

#[tokio::test]
async fn test_admin_cannot_delete_self() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;
    let admin_id = admin_id(&ctx).await;

    let resp = ctx.delete(&format!("/api/users/{}", admin_id)).await;
    assert_eq!(resp.status(), 403, "admin must not be able to delete itself");

    let resp = ctx.get(&format!("/api/users/{}", admin_id)).await;
    assert_eq!(resp.status(), 200, "admin must still exist after rejected delete");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["user"]["username"], "admin");
    assert_eq!(body["user"]["is_admin"], true);
}

#[tokio::test]
async fn test_admin_cannot_delete_another_admin() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;
    let admin_group_id = system_group_id(&ctx, "admin").await;
    let second_admin_id = create_user_via_admin(&ctx, "second_admin").await;
    add_user_to_admin_group(&ctx, &second_admin_id, &admin_group_id).await;

    let resp = ctx.delete(&format!("/api/users/{}", second_admin_id)).await;
    assert_eq!(resp.status(), 403, "an admin must not delete another admin");

    let resp = ctx.get(&format!("/api/users/{}", second_admin_id)).await;
    assert_eq!(resp.status(), 200, "second admin must still exist");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["user"]["is_admin"], true);
}

#[tokio::test]
async fn test_non_admin_cannot_delete_admin() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;
    let admin_id = admin_id(&ctx).await;
    create_user_via_admin(&ctx, "plain_del_admin").await;

    ctx.login_user("plain_del_admin", "pass123").await;
    let resp = ctx.delete(&format!("/api/users/{}", admin_id)).await;
    assert_eq!(resp.status(), 403, "a plain user must not delete the admin");
}

#[tokio::test]
async fn test_admin_cannot_demote_self() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;
    let admin_id = admin_id(&ctx).await;
    let user_group_id = system_group_id(&ctx, "user").await;

    // Dropping Admin membership entirely (replace with the User group).
    let resp = ctx
        .put(
            &format!("/api/users/{}", admin_id),
            &serde_json::json!({ "group_ids": [user_group_id] }),
        )
        .await;
    assert_eq!(resp.status(), 403, "admin must not be able to demote itself");

    // Empty list also drops Admin membership.
    let resp = ctx
        .put(
            &format!("/api/users/{}", admin_id),
            &serde_json::json!({ "group_ids": [] }),
        )
        .await;
    assert_eq!(resp.status(), 403, "empty membership list must not demote admin");

    let resp = ctx.get(&format!("/api/users/{}", admin_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["user"]["is_admin"], true, "admin must still be admin");
}

#[tokio::test]
async fn test_admin_cannot_demote_another_admin() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;
    let admin_group_id = system_group_id(&ctx, "admin").await;
    let second_admin_id = create_user_via_admin(&ctx, "second_admin_demote").await;
    add_user_to_admin_group(&ctx, &second_admin_id, &admin_group_id).await;

    let resp = ctx
        .put(
            &format!("/api/users/{}", second_admin_id),
            &serde_json::json!({ "group_ids": [] }),
        )
        .await;
    assert_eq!(resp.status(), 403, "an admin must not demote another admin");

    let resp = ctx.get(&format!("/api/users/{}", second_admin_id)).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["user"]["is_admin"], true, "second admin must stay admin");
}

#[tokio::test]
async fn test_non_admin_cannot_remove_admin_membership() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;
    let admin_id = admin_id(&ctx).await;

    // A manager (can_manage_users via group membership) may manage users, but
    // must not strip the Admin group from an admin.
    let manager_id = create_user_via_admin(&ctx, "mgmt_demote_admin").await;
    let resp = ctx
        .post(
            "/api/groups",
            &serde_json::json!({
                "name": "user-mgrs-demote",
                "description": null,
                "can_manage_users": true,
                "max_instances": 2,
                "template_ids": [],
            }),
        )
        .await;
    assert_eq!(resp.status(), 200);
    let group_id = resp.json::<serde_json::Value>().await.unwrap()["group"]["id"]
        .as_str()
        .unwrap()
        .to_string();

    let resp = ctx
        .put(
            &format!("/api/users/{}", manager_id),
            &serde_json::json!({ "group_ids": [group_id] }),
        )
        .await;
    assert_eq!(resp.status(), 200);

    ctx.login_user("mgmt_demote_admin", "pass123").await;
    let resp = ctx
        .put(
            &format!("/api/users/{}", admin_id),
            &serde_json::json!({ "group_ids": [] }),
        )
        .await;
    assert_eq!(resp.status(), 403, "a non-admin must not remove Admin membership");
}

#[tokio::test]
async fn test_admin_identity_edits_still_allowed() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;
    let admin_id = admin_id(&ctx).await;

    let resp = ctx
        .put(
            &format!("/api/users/{}", admin_id),
            &serde_json::json!({ "username": "admin_renamed" }),
        )
        .await;
    assert_eq!(resp.status(), 200, "username edit on admin must still work");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["user"]["username"], "admin_renamed");
    assert_eq!(body["user"]["is_admin"], true);
}

#[tokio::test]
async fn test_admin_non_admin_membership_stays_editable() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;
    let user_group_id = system_group_id(&ctx, "user").await;
    let user_id = create_user_via_admin(&ctx, "membership_target").await;

    // Admin edits a plain user's membership (no Admin group involved) — the
    // new guard must not affect non-admin membership writes.
    let resp = ctx
        .put(
            &format!("/api/users/{}", user_id),
            &serde_json::json!({ "group_ids": [user_group_id] }),
        )
        .await;
    assert_eq!(resp.status(), 200, "editing a plain user's membership must still work");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["user"]["group_ids"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_cannot_create_user_in_admin_group() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;
    let admin_group_id = system_group_id(&ctx, "admin").await;

    let resp = ctx
        .post(
            "/api/users",
            &serde_json::json!({
                "username": "wannabe_admin",
                "password": "pass123",
                "group_ids": [admin_group_id],
            }),
        )
        .await;
    assert_eq!(resp.status(), 403, "create must not place a user in the Admin group");
}

#[tokio::test]
async fn test_cannot_assign_user_into_admin_group() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;
    let admin_group_id = system_group_id(&ctx, "admin").await;
    let user_id = create_user_via_admin(&ctx, "assign_target").await;

    let resp = ctx
        .put(
            &format!("/api/users/{}", user_id),
            &serde_json::json!({ "group_ids": [admin_group_id] }),
        )
        .await;
    assert_eq!(resp.status(), 403, "update must not place a user in the Admin group");
}
