mod common;

use common::TestContext;
use openworkspace_api::db::{group, group_template, user_group};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};

/// Create a plain user via the admin API and return their id.
async fn create_user(ctx: &TestContext, username: &str) -> String {
    let resp = ctx
        .post(
            "/api/users",
            &serde_json::json!({
                "username": username,
                "password": "pass123",
            }),
        )
        .await;
    assert_eq!(
        resp.status(),
        200,
        "create user failed: {}",
        resp.text().await.unwrap()
    );
    resp.json::<serde_json::Value>().await.unwrap()["user"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Create a template via the admin API (defaults fill the rest) and return id.
async fn create_template(ctx: &TestContext, name: &str) -> String {
    let resp = ctx
        .post(
            "/api/templates",
            &serde_json::json!({ "name": name }),
        )
        .await;
    assert_eq!(
        resp.status(),
        200,
        "create template failed: {}",
        resp.text().await.unwrap()
    );
    resp.json::<serde_json::Value>().await.unwrap()["template"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Create a group as the admin with the given flag overrides. Returns its id.
async fn create_group(
    ctx: &TestContext,
    name: &str,
    flags: &[(&str, bool)],
    max_instances: i32,
    template_ids: &[String],
) -> String {
    let mut body = serde_json::Map::new();
    body.insert("name".to_string(), serde_json::json!(name));
    body.insert("description".to_string(), serde_json::Value::Null);
    for (flag, on) in flags {
        body.insert(flag.to_string(), serde_json::json!(on));
    }
    body.insert("max_instances".to_string(), serde_json::json!(max_instances));
    body.insert(
        "template_ids".to_string(),
        serde_json::json!(template_ids),
    );
    let resp = ctx.post("/api/groups", &serde_json::Value::Object(body)).await;
    assert_eq!(
        resp.status(),
        200,
        "create group failed: {}",
        resp.text().await.unwrap()
    );
    resp.json::<serde_json::Value>().await.unwrap()["group"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn test_admin_group_crud_round_trip() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let tpl = create_template(&ctx, "grp-tpl").await;

    // Create a group with flags, a ceiling, and a template whitelist.
    let resp = ctx
        .post(
            "/api/groups",
            &serde_json::json!({
                "name": "team-a",
                "description": "Team A",
                "can_create_template": true,
                "can_manage_users": false,
                "can_manage_group_instances": true,
                "can_manage_docker": false,
                "can_manage_registry": false,
                "max_instances": 3,
                "template_ids": [tpl],
            }),
        )
        .await;
    assert_eq!(
        resp.status(),
        200,
        "create group failed: {}",
        resp.text().await.unwrap()
    );
    let body: serde_json::Value = resp.json().await.unwrap();
    let group = &body["group"];
    let group_id = group["id"].as_str().unwrap().to_string();
    assert_eq!(group["name"], "team-a");
    assert_eq!(group["description"], "Team A");
    assert_eq!(group["can_create_template"], true);
    assert_eq!(group["can_manage_group_instances"], true);
    assert_eq!(group["can_manage_users"], false);
    assert_eq!(group["max_instances"], 3);
    assert_eq!(group["template_ids"], serde_json::json!([tpl]));

    // The list returns the same shape, whitelist included.
    let body: serde_json::Value = ctx.get("/api/groups").await.json().await.unwrap();
    let listed = body["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["id"] == group_id)
        .expect("group must appear in the list");
    assert_eq!(listed["template_ids"], serde_json::json!([tpl]));

    // Edit: flip a flag, raise the ceiling, clear the whitelist.
    let resp = ctx
        .put(
            &format!("/api/groups/{}", group_id),
            &serde_json::json!({
                "name": "team-a",
                "description": "Team A v2",
                "can_create_template": false,
                "can_manage_users": true,
                "can_manage_group_instances": false,
                "can_manage_docker": false,
                "can_manage_registry": false,
                "max_instances": 7,
                "template_ids": [],
            }),
        )
        .await;
    assert_eq!(
        resp.status(),
        200,
        "update group failed: {}",
        resp.text().await.unwrap()
    );
    let group: serde_json::Value = resp.json::<serde_json::Value>().await.unwrap()["group"].clone();
    assert_eq!(group["can_create_template"], false);
    assert_eq!(group["can_manage_users"], true);
    assert_eq!(group["max_instances"], 7);
    assert_eq!(group["template_ids"], serde_json::json!([]));

    // Delete.
    let resp = ctx.delete(&format!("/api/groups/{}", group_id)).await;
    assert_eq!(resp.status(), 204);

    let body: serde_json::Value = ctx.get("/api/groups").await.json().await.unwrap();
    assert!(
        !body["groups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["id"] == group_id),
        "deleted group must not appear in the list"
    );
}

#[tokio::test]
async fn test_groups_list_requires_auth() {
    let ctx = TestContext::new().await;
    let resp = ctx.get("/api/groups").await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_can_manage_users_holder_cannot_write_groups() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let group_id = create_group(&ctx, "user-mgrs", &[("can_manage_users", true)], 2, &[]).await;
    let user_id = create_user(&ctx, "mgru").await;

    // Give the holder `can_manage_users` via membership.
    let resp = ctx
        .put(
            &format!("/api/users/{}", user_id),
            &serde_json::json!({ "group_ids": [group_id] }),
        )
        .await;
    assert_eq!(resp.status(), 200);

    ctx.login_user("mgru", "pass123").await;

    // Reading the catalog is fine for a user manager…
    let resp = ctx.get("/api/groups").await;
    assert_eq!(resp.status(), 200);

    // …but every group-policy write is 403, even though they hold
    // `can_manage_users`.
    let resp = ctx
        .post(
            "/api/groups",
            &serde_json::json!({
                "name": "sneaky",
                "description": null,
                "can_create_template": false,
                "can_manage_users": false,
                "can_manage_group_instances": false,
                "can_manage_docker": true,
                "can_manage_registry": false,
                "max_instances": 2,
                "template_ids": [],
            }),
        )
        .await;
    assert_eq!(resp.status(), 403, "non-admin must not create groups");

    let resp = ctx
        .put(
            &format!("/api/groups/{}", group_id),
            &serde_json::json!({
                "name": "user-mgrs",
                "description": null,
                "can_create_template": false,
                "can_manage_users": false,
                "can_manage_group_instances": false,
                "can_manage_docker": false,
                "can_manage_registry": false,
                "max_instances": 1,
                "template_ids": [],
            }),
        )
        .await;
    assert_eq!(resp.status(), 403, "non-admin must not edit groups");

    let resp = ctx.delete(&format!("/api/groups/{}", group_id)).await;
    assert_eq!(resp.status(), 403, "non-admin must not delete groups");
}

#[tokio::test]
async fn test_non_admin_cannot_assign_membership_for_others() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let group_id = create_group(&ctx, "plain-team", &[], 2, &[]).await;
    let target = create_user(&ctx, "target3").await;
    let _plain = create_user(&ctx, "plain3").await;

    ctx.login_user("plain3", "pass123").await;

    // A plain user (no `can_manage_users`) cannot assign memberships at all.
    let resp = ctx
        .put(
            &format!("/api/users/{}", target),
            &serde_json::json!({ "group_ids": [group_id] }),
        )
        .await;
    assert_eq!(resp.status(), 403);
}

#[tokio::test]
async fn test_user_manager_can_assign_membership_to_others() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let group_id = create_group(&ctx, "mgr-team", &[("can_manage_users", true)], 2, &[]).await;
    let manager = create_user(&ctx, "mgr-assigner").await;
    let target = create_user(&ctx, "mgr-target").await;

    // A policy write needs the actor's tier strictly above the target's, so the
    // assigner rides the seeded Manager system group (tier 1, all flags on).
    let body: serde_json::Value = ctx.get("/api/groups").await.json().await.unwrap();
    let manager_group_id = body["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["kind"] == "manager")
        .expect("the seeded Manager group must exist")
        .get("id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    ctx.put(
        &format!("/api/users/{}", manager),
        &serde_json::json!({ "group_ids": [manager_group_id] }),
    )
    .await;

    ctx.login_user("mgr-assigner", "pass123").await;

    // Assigning ANOTHER user to an existing group is the manager's job.
    let resp = ctx
        .put(
            &format!("/api/users/{}", target),
            &serde_json::json!({ "group_ids": [group_id] }),
        )
        .await;
    assert_eq!(
        resp.status(),
        200,
        "user manager must be able to assign memberships: {}",
        resp.text().await.unwrap()
    );
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["user"]["group_ids"], serde_json::json!([group_id]));

    // The assignment round-trips into the target's effective context.
    ctx.post("/api/auth/logout", &serde_json::json!({})).await;
    ctx.login_user("mgr-target", "pass123").await;
    let body: serde_json::Value = ctx.get("/api/auth/me").await.json().await.unwrap();
    assert_eq!(body["context"]["group_ids"], serde_json::json!([group_id]));
    assert_eq!(body["context"]["can_manage_users"], true);
}

#[tokio::test]
async fn test_membership_and_personal_overrides_round_trip_into_me() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let tpl = create_template(&ctx, "roundtrip-tpl").await;
    let group_id = create_group(
        &ctx,
        "rt-group",
        &[("can_manage_docker", true)],
        5,
        &[tpl.clone()],
    )
    .await;
    let target = create_user(&ctx, "rt-user").await;

    // Admin assigns membership + personal overrides. (The personal template
    // whitelist is gone: the group whitelist is the only input.)
    let resp = ctx
        .put(
            &format!("/api/users/{}", target),
            &serde_json::json!({
                "group_ids": [group_id],
                "direct_max_instances": 9,
            }),
        )
        .await;
    assert_eq!(
        resp.status(),
        200,
        "policy update failed: {}",
        resp.text().await.unwrap()
    );
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["user"]["group_ids"], serde_json::json!([group_id]));
    assert_eq!(body["user"]["direct_max_instances"], 9);

    // The very next `/auth/me` reflects the assignment without re-login
    // (context is recomputed per request).
    ctx.login_user("rt-user", "pass123").await;
    let body: serde_json::Value = ctx.get("/api/auth/me").await.json().await.unwrap();
    let context = &body["context"];
    assert_eq!(context["is_admin"], false);
    assert_eq!(context["tier"], 0);
    assert_eq!(context["can_manage_docker"], true);
    assert_eq!(context["effective_max_instances"], 9, "personal ceiling wins");
    assert_eq!(context["group_ids"], serde_json::json!([group_id]));
    assert!(
        context["allowed_template_ids"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!(tpl)),
        "whitelist must contain the group template"
    );

    // Clearing the personal ceiling falls back to the group maximum.
    ctx.post("/api/auth/logout", &serde_json::json!({})).await;
    ctx.login_admin().await;
    let resp = ctx
        .put(
            &format!("/api/users/{}", target),
            &serde_json::json!({ "direct_max_instances": null }),
        )
        .await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        resp.json::<serde_json::Value>().await.unwrap()["user"]["direct_max_instances"],
        serde_json::Value::Null
    );

    ctx.login_user("rt-user", "pass123").await;
    let body: serde_json::Value = ctx.get("/api/auth/me").await.json().await.unwrap();
    assert_eq!(
        body["context"]["effective_max_instances"], 5,
        "group maximum is the fallback"
    );
}

#[tokio::test]
async fn test_escalation_rejected_non_admin_self_policy_write() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    // A privileged group the user manager must never be able to join…
    let privileged = create_group(&ctx, "docker-team", &[("can_manage_docker", true)], 2, &[]).await;
    // …and the group the user manager legitimately belongs to.
    let managers = create_group(&ctx, "user-mgrs", &[("can_manage_users", true)], 2, &[]).await;
    let manager_user = create_user(&ctx, "esc-mgr").await;

    let resp = ctx
        .put(
            &format!("/api/users/{}", manager_user),
            &serde_json::json!({ "group_ids": [managers] }),
        )
        .await;
    assert_eq!(resp.status(), 200);

    ctx.login_user("esc-mgr", "pass123").await;

    // Self-membership write → 403 (cannot join a group / modify own rows).
    let resp = ctx
        .put(
            &format!("/api/users/{}", manager_user),
            &serde_json::json!({ "group_ids": [privileged] }),
        )
        .await;
    assert_eq!(resp.status(), 403, "self membership write must be rejected");

    // Self personal overrides → 403 too.
    let resp = ctx
        .put(
            &format!("/api/users/{}", manager_user),
            &serde_json::json!({ "direct_max_instances": 50 }),
        )
        .await;
    assert_eq!(resp.status(), 403, "self ceiling write must be rejected");

    // The forge-and-join vector: creating a privileged group is admin-only.
    let resp = ctx
        .post(
            "/api/groups",
            &serde_json::json!({
                "name": "forge",
                "description": null,
                "can_create_template": false,
                "can_manage_users": false,
                "can_manage_group_instances": false,
                "can_manage_docker": true,
                "can_manage_registry": false,
                "max_instances": 2,
                "template_ids": [],
            }),
        )
        .await;
    assert_eq!(resp.status(), 403, "group create must stay admin-only");

    // The user manager's own context is unchanged: no escalation happened.
    let body: serde_json::Value = ctx.get("/api/auth/me").await.json().await.unwrap();
    let context = &body["context"];
    assert_eq!(context["group_ids"], serde_json::json!([managers]));
    assert_eq!(context["can_manage_docker"], false);
    assert_eq!(context["direct_max_instances"], serde_json::Value::Null);
}

#[tokio::test]
async fn test_delete_group_cascades_join_rows() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let tpl = create_template(&ctx, "cascade-tpl").await;
    let group_id = create_group(&ctx, "cascade", &[], 2, &[tpl]).await;
    let user_id = create_user(&ctx, "cascade-user").await;

    ctx.put(
        &format!("/api/users/{}", user_id),
        &serde_json::json!({ "group_ids": [group_id] }),
    )
    .await;

    let group_uuid = uuid::Uuid::parse_str(&group_id).unwrap();
    let user_uuid = uuid::Uuid::parse_str(&user_id).unwrap();
    let db = sea_orm::Database::connect(&common::pg_url(&ctx.db_name))
        .await
        .unwrap();

    // Before delete: both join rows exist.
    let memberships = user_group::Entity::find()
        .filter(user_group::Column::UserId.eq(user_uuid))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(memberships.len(), 1);
    let whitelist = group_template::Entity::find()
        .filter(group_template::Column::GroupId.eq(group_uuid))
        .all(&db)
        .await
        .unwrap();
    assert_eq!(whitelist.len(), 1);

    let resp = ctx.delete(&format!("/api/groups/{}", group_id)).await;
    assert_eq!(resp.status(), 204);

    // Deleting the group cascades to user_groups and group_templates at the
    // database level (FKs from migration 000018), and the group is gone.
    let memberships = user_group::Entity::find()
        .filter(user_group::Column::UserId.eq(user_uuid))
        .all(&db)
        .await
        .unwrap();
    assert!(memberships.is_empty(), "user_groups must cascade on delete");
    let whitelist = group_template::Entity::find()
        .filter(group_template::Column::GroupId.eq(group_uuid))
        .all(&db)
        .await
        .unwrap();
    assert!(whitelist.is_empty(), "group_templates must cascade on delete");
    assert!(
        group::Entity::find_by_id(group_uuid).one(&db).await.unwrap().is_none(),
        "group row must be gone"
    );
}

#[tokio::test]
async fn test_create_group_duplicate_name_conflict() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx
        .post(
            "/api/groups",
            &serde_json::json!({
                "name": "dup-name",
                "description": null,
                "can_create_template": false,
                "can_manage_users": false,
                "can_manage_group_instances": false,
                "can_manage_docker": false,
                "can_manage_registry": false,
                "max_instances": 2,
                "template_ids": [],
            }),
        )
        .await;
    assert_eq!(resp.status(), 200);

    let resp = ctx
        .post(
            "/api/groups",
            &serde_json::json!({
                "name": "dup-name",
                "description": null,
                "can_create_template": false,
                "can_manage_users": false,
                "can_manage_group_instances": false,
                "can_manage_docker": false,
                "can_manage_registry": false,
                "max_instances": 2,
                "template_ids": [],
            }),
        )
        .await;
    assert_eq!(resp.status(), 409, "duplicate group name must conflict");
}

#[tokio::test]
async fn test_group_create_rejects_unknown_template() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx
        .post(
            "/api/groups",
            &serde_json::json!({
                "name": "ghost-tpl",
                "description": null,
                "can_create_template": false,
                "can_manage_users": false,
                "can_manage_group_instances": false,
                "can_manage_docker": false,
                "can_manage_registry": false,
                "max_instances": 2,
                "template_ids": ["00000000-0000-0000-0000-000000000000"],
            }),
        )
        .await;
    assert_eq!(resp.status(), 400, "unknown template in the whitelist must be rejected");
}

#[tokio::test]
async fn test_membership_assign_unknown_group_rejected() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let target = create_user(&ctx, "ghost-group-user").await;

    let resp = ctx
        .put(
            &format!("/api/users/{}", target),
            &serde_json::json!({
                "group_ids": ["00000000-0000-0000-0000-000000000000"],
            }),
        )
        .await;
    assert_eq!(resp.status(), 400, "unknown group in memberships must be rejected");
}

#[tokio::test]
async fn test_context_helpers() {
    let ctx = TestContext::new().await;
    let _ = ctx.login_user("admin", "admin").await;
    let _ = ctx.put("/health", &serde_json::json!({})).await;
    let _ = ctx.delete("/health").await;
    let _ = ctx.login_token().await;
    let _ = &ctx.base_url;
    let _ = &ctx.client;
    let _ = &ctx.db_name;
}
