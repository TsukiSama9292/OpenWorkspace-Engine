#![cfg(feature = "docker")]

mod common;

use std::path::Path;

use common::TestContext;
use openworkspace_api::db::{PersistentVolume, PersistentVolumeRepository, VOLUME_STATUS_ORPHANED};
use openworkspace_api::persistent_volume::{
    persistent_volume_name, resolve_persistent_host_path,
};

const PERSISTENT_ROOT: &str = "/tmp/ow_e2e_root";

fn template_name(suffix: &str) -> String {
    format!("ow_test_e2e_{}_{}", std::process::id(), suffix)
}

/// A per-test persistent root, unique across the suite, so this test can tear
/// its whole host-data tree down without racing a concurrently-running test.
fn persistent_root(suffix: &str) -> String {
    format!("{}_{}_{}", PERSISTENT_ROOT, std::process::id(), suffix)
}

async fn create_persistent_template(ctx: &TestContext, suffix: &str) -> String {
    common::ensure_network().await;
    ctx.login_admin().await;
    let name = template_name(suffix);
    let resp = ctx
        .post("/api/templates", &serde_json::json!({
            "name": name,
            "image": "busybox:1",
            "cores": 0,
            "memory": 0,
            "run_config": { "command": ["sleep", "3600"] },
            "persistent_storage_path": persistent_root(suffix),
        }))
        .await;
    assert_eq!(resp.status(), 200, "create template failed");
    let body: serde_json::Value = resp.json().await.unwrap();
    body["template"]["id"].as_str().unwrap().to_string()
}

async fn create_plain_template(ctx: &TestContext, name: &str) -> String {
    ctx.login_admin().await;
    let resp = ctx
        .post("/api/templates", &serde_json::json!({ "name": name }))
        .await;
    assert_eq!(resp.status(), 200, "create plain template failed");
    let body: serde_json::Value = resp.json().await.unwrap();
    body["template"]["id"].as_str().unwrap().to_string()
}

async fn create_user(ctx: &TestContext, username: &str) -> String {
    ctx.login_admin().await;
    let resp = ctx
        .post("/api/users", &serde_json::json!({
            "username": username,
            "password": "pw123456",
        }))
        .await;
    assert_eq!(resp.status(), 200, "create user failed");
    let body: serde_json::Value = resp.json().await.unwrap();
    body["user"]["id"].as_str().unwrap().to_string()
}

async fn create_group(
    ctx: &TestContext,
    name: &str,
    max_instances: i32,
    template_ids: &[String],
) -> String {
    ctx.login_admin().await;
    let resp = ctx
        .post("/api/groups", &serde_json::json!({
            "name": name,
            "description": null,
            "can_create_template": false,
            "can_manage_users": false,
            "can_manage_group_instances": true,
            "can_manage_docker": false,
            "can_manage_registry": false,
            "max_instances": max_instances,
            "template_ids": template_ids,
        }))
        .await;
    assert_eq!(resp.status(), 200, "create group failed");
    let body: serde_json::Value = resp.json().await.unwrap();
    body["group"]["id"].as_str().unwrap().to_string()
}

async fn assign_user_policy(
    ctx: &TestContext,
    user_id: &str,
    group_ids: &[String],
    direct_max_instances: Option<i32>,
) {
    ctx.login_admin().await;
    let mut body = serde_json::Map::new();
    body.insert("group_ids".to_string(), serde_json::json!(group_ids));
    if let Some(ceiling) = direct_max_instances {
        body.insert(
            "direct_max_instances".to_string(),
            serde_json::json!(ceiling),
        );
    }
    let resp = ctx
        .put(&format!("/api/users/{}", user_id), &serde_json::Value::Object(body))
        .await;
    assert_eq!(resp.status(), 200, "assign user policy failed");
}

/// The id of a seeded system group (`admin`/`manager`/`user`) by kind.
async fn system_group_id(ctx: &TestContext, kind: &str) -> String {
    ctx.login_admin().await;
    let body: serde_json::Value = ctx.get("/api/groups").await.json().await.unwrap();
    body["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["kind"] == kind)
        .expect("seeded system group must exist")
        .get("id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string()
}

async fn launch_plain(ctx: &TestContext, template_id: &str) -> String {
    let resp = ctx
        .post("/api/instances", &serde_json::json!({ "template_id": template_id }))
        .await;
    assert_eq!(resp.status(), 200, "launch failed: {:?}", resp.text().await);
    let body: serde_json::Value = resp.json().await.unwrap();
    let status = body["instance"]["status"].as_str();
    assert!(
        status == Some("starting") || status == Some("running"),
        "launch must succeed, got status {:?}: {}",
        status,
        serde_json::to_string_pretty(&body).unwrap()
    );
    assert!(
        body["instance"]["container_id"].as_str().is_some(),
        "launch returned no container_id"
    );
    body["instance"]["id"].as_str().unwrap().to_string()
}

/// The id of the seeded `admin` account.
async fn admin_user_id(ctx: &TestContext) -> String {
    ctx.login_admin().await;
    let body: serde_json::Value = ctx.get("/api/users").await.json().await.unwrap();
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

/// The current effective instance ceiling for a user, read from their session.
async fn effective_ceiling(ctx: &TestContext, username: &str) -> i64 {
    assert_eq!(ctx.login_user(username, "pw123456").await.status(), 200);
    let body: serde_json::Value = ctx.get("/api/auth/me").await.json().await.unwrap();
    body["context"]["effective_max_instances"].as_i64().unwrap()
}

async fn launch_persistent(ctx: &TestContext, template_id: &str) -> serde_json::Value {
    let resp = ctx
        .post("/api/instances", &serde_json::json!({
            "template_id": template_id,
            "persistence": "use_persistent",
        }))
        .await;
    assert_eq!(resp.status(), 200, "launch failed: {:?}", resp.text().await);
    let body: serde_json::Value = resp.json().await.unwrap();
    let status = body["instance"]["status"].as_str();
    assert!(
        status == Some("starting") || status == Some("running"),
        "launch must succeed, got status {:?}: {}",
        status,
        serde_json::to_string_pretty(&body).unwrap()
    );
    assert!(
        body["instance"]["container_id"].as_str().is_some(),
        "launch returned no container_id"
    );
    body
}

fn resolved_host_path(root: &str, template_name: &str, owner_id: &str) -> String {
    resolve_persistent_host_path(root, template_name, owner_id).unwrap()
}

async fn find_volume(ctx: &TestContext, host_path: &str) -> Option<PersistentVolume> {
    let db = sea_orm::Database::connect(&common::pg_url(&ctx.db_name))
        .await
        .unwrap();
    let volume = PersistentVolumeRepository::new(&db)
        .find_by_host_path(host_path)
        .await
        .unwrap();
    drop(db);
    volume
}

/// Remove the per-test host-data tree (leaf dir, template parent, test root)
/// left behind by the cleanup endpoint, which empties but keeps the chain.
fn remove_host_dirs(host_path: &str) {
    let path = Path::new(host_path);
    std::fs::remove_dir_all(path).ok();
    if let Some(parent) = path.parent() {
        std::fs::remove_dir_all(parent).ok();
        if let Some(root) = parent.parent() {
            std::fs::remove_dir_all(root).ok();
        }
    }
}

// ── The full flat-RBAC story against the real backend (ticket 15) ──

/// Admin creates a group (template whitelist + max_instances), assigns a member;
/// the member launches a whitelisted persistent template, is refused a
/// non-whitelisted one (403), hits the group ceiling (409), and a
/// same-group manager (riding the Manager system group for the tier guard) 
/// controls the member's instance (GET/stop/delete, while a disjoint-group
/// user is 403); deleting the instance orphans its persistent volume, which
/// the admin then cleans up after confirmation.
#[tokio::test]
async fn test_flat_rbac_end_to_end() {
    let ctx = TestContext::new().await;

    // 1. Admin sets up two templates (a persistent one for the launch, two
    //    plain ones for the refusal paths) and a group with a template
    //    whitelist + a group ceiling + same-group instance control.
    let tpl_a = create_persistent_template(&ctx, "a").await;
    let tpl_b = create_plain_template(&ctx, template_name("b").as_str()).await;
    let tpl_c = create_plain_template(&ctx, template_name("c").as_str()).await;
    // The group's ceiling of 1 IS the effective ceiling: the max rule keeps
    // it, since no member carries a personal ceiling above it.
    let group_g = create_group(&ctx, "e2e_team", 1, &[tpl_a.clone(), tpl_b.clone()]).await;

    // A second, flagless group for the disjoint-group user.
    let group_h = create_group(&ctx, "e2e_others", 1, &[]).await;
    // The seeded Manager system group: the manager rides it so the tier guard
    // (actor tier > owner tier) lets them control the tier-0 member's instance.
    let manager_group = system_group_id(&ctx, "manager").await;

    // 2. Members: the owner rides the group ceiling (1); a manager shares the
    //    group; an outsider is in the disjoint flagless group.
    let member_id = create_user(&ctx, "e2e_member").await;
    let _manager_id = create_user(&ctx, "e2e_manager").await;
    let _outsider_id = create_user(&ctx, "e2e_outsider").await;

    assign_user_policy(&ctx, &member_id, std::slice::from_ref(&group_g), None).await;
    assign_user_policy(
        &ctx,
        &_manager_id,
        &[manager_group.clone(), group_g.clone()],
        None,
    )
    .await;
    assign_user_policy(&ctx, &_outsider_id, &[group_h], None).await;

    // 3. The member logs in and their effective context reflects the group
    //    ceiling and the group-union whitelist.
    assert_eq!(ctx.login_user("e2e_member", "pw123456").await.status(), 200);
    let body: serde_json::Value = ctx.get("/api/auth/me").await.json().await.unwrap();
    let context = &body["context"];
    assert_eq!(context["effective_max_instances"], 1, "group ceiling is the cap");
    assert_eq!(context["is_admin"], false);
    let allowed = context["allowed_template_ids"].as_array().unwrap();
    assert!(allowed.contains(&serde_json::json!(tpl_a)), "group whitelist");
    assert!(allowed.contains(&serde_json::json!(tpl_b)), "group whitelist union");

    // 4. Launch the whitelisted persistent template → success, owned by the
    //    member, carrying the owner's group ids.
    let launch = launch_persistent(&ctx, &tpl_a).await;
    let instance_id = launch["instance"]["id"].as_str().unwrap().to_string();
    let body: serde_json::Value = launch["instance"].clone();
    assert_eq!(body["owner_id"], member_id);
    assert_eq!(body["owner_group_ids"], serde_json::json!([group_g]));
    assert_eq!(body["owner_tier"], 0, "member rides the tier-0 User group");

    let host_path = resolved_host_path(
        &persistent_root("a"),
        &template_name("a"),
        &member_id,
    );

    // While the instance is live the volume is referenced → not orphaned.
    assert_eq!(ctx.login_admin().await.status(), 200);
    let resp = ctx.get("/api/persistent-volumes").await;
    assert_eq!(resp.status(), 200);
    let vols: serde_json::Value = resp.json().await.unwrap();
    assert!(
        vols["volumes"].as_array().unwrap().is_empty(),
        "active volume must not be listed as orphaned"
    );
    assert_eq!(ctx.login_user("e2e_member", "pw123456").await.status(), 200);

    // 5. A non-whitelisted template → 403 with the machine-readable rejection.
    let resp = ctx
        .post("/api/instances", &serde_json::json!({ "template_id": tpl_c }))
        .await;
    assert_eq!(resp.status(), 403);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["rejection"]["scope"], "template_not_allowed");
    assert_eq!(body["rejection"]["current"], 0);
    assert_eq!(body["rejection"]["limit"], 0);
    assert_eq!(body["rejection"]["requested"], 1);

    // 6. Hitting the group ceiling → 409 with the rejection's numbers.
    let resp = ctx
        .post("/api/instances", &serde_json::json!({ "template_id": tpl_b }))
        .await;
    assert_eq!(resp.status(), 409);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["rejection"]["scope"], "user_instance");
    assert_eq!(body["rejection"]["current"], 1);
    assert_eq!(body["rejection"]["limit"], 1);
    assert_eq!(body["rejection"]["requested"], 1);

    // Neither rejection left a row behind: the member still owns exactly one.
    let list: serde_json::Value = ctx.get("/api/instances").await.json().await.unwrap();
    assert_eq!(list["instances"].as_array().unwrap().len(), 1);

    // 7. A same-group manager may read and stop the member's instance; the
    //    disjoint-group user is 403 on every lifecycle op.
    assert_eq!(ctx.login_user("e2e_manager", "pw123456").await.status(), 200);
    let resp = ctx.get(&format!("/api/instances/{}", instance_id)).await;
    assert_eq!(resp.status(), 200, "manager must read the member instance");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["owner_id"], member_id);
    assert_eq!(body["instance"]["owner_group_ids"], serde_json::json!([group_g]));
    let resp = ctx
        .post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({}))
        .await;
    assert_eq!(resp.status(), 200, "manager must stop the member instance");

    assert_eq!(ctx.login_user("e2e_outsider", "pw123456").await.status(), 200);
    assert_eq!(
        ctx.get(&format!("/api/instances/{}", instance_id)).await.status(),
        403,
        "disjoint-group user must not read"
    );
    assert_eq!(
        ctx.post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({}))
            .await
            .status(),
        403
    );
    assert_eq!(
        ctx.delete(&format!("/api/instances/{}", instance_id)).await.status(),
        403,
        "disjoint-group user must not delete"
    );

    // 8. The manager deletes the instance → 204, and the persistent volume is
    //    orphaned while its host data survives (delete keeps it for reuse).
    assert_eq!(ctx.login_user("e2e_manager", "pw123456").await.status(), 200);
    assert_eq!(
        ctx.delete(&format!("/api/instances/{}", instance_id)).await.status(),
        204
    );
    let volume = find_volume(&ctx, &host_path)
        .await
        .expect("persistent launch must leave a registry row");
    assert_eq!(volume.status, VOLUME_STATUS_ORPHANED);
    assert!(Path::new(&host_path).exists(), "delete must keep host data");

    // 9. Group-scoped powers alone do NOT grant the orphaned view or cleanup:
    //    the member (can_manage_group_instances) is 403.
    assert_eq!(ctx.login_user("e2e_member", "pw123456").await.status(), 200);
    assert_eq!(ctx.get("/api/persistent-volumes").await.status(), 403);
    let resp = ctx
        .post(
            &format!("/api/persistent-volumes/{}/cleanup", volume.id),
            &serde_json::json!({}),
        )
        .await;
    assert_eq!(resp.status(), 403, "member must not clean up volumes");

    // 10. The admin sees the orphan with its owner and cleans it up (the
    //     frontend's double confirmation happens there).
    assert_eq!(ctx.login_admin().await.status(), 200);
    let resp = ctx.get("/api/persistent-volumes").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let vol = body["volumes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["host_path"] == host_path)
        .expect("orphaned volume must be listed");
    assert_eq!(vol["owner_id"], member_id);
    assert_eq!(vol["owner_username"], "e2e_member");
    assert_eq!(vol["status"], VOLUME_STATUS_ORPHANED);

    // Seed a host-data file to prove the cleanup empties it.
    let data_file = Path::new(&host_path).join("doomed.txt");
    std::fs::write(&data_file, "data").unwrap();
    assert!(data_file.exists(), "delete must keep host data");

    let resp = ctx
        .post(
            &format!("/api/persistent-volumes/{}/cleanup", volume.id),
            &serde_json::json!({}),
        )
        .await;
    assert_eq!(resp.status(), 204, "admin cleanup must succeed");
    assert!(find_volume(&ctx, &host_path).await.is_none(), "row must be gone");
    let docker = bollard::Docker::connect_with_local_defaults().unwrap();
    assert!(
        docker
            .inspect_volume(&persistent_volume_name(&host_path))
            .await
            .is_err(),
        "cleanup must remove the docker volume"
    );
    assert!(
        !data_file.exists(),
        "cleanup must delete the host data"
    );
    assert!(
        !Path::new(&host_path).exists(),
        "cleanup must remove the host data dir itself"
    );
    remove_host_dirs(&host_path);
}

#[tokio::test]
async fn test_flat_rbac_2_tiers_end_to_end() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;

    let admin_group = system_group_id(&ctx, "admin").await;
    let manager_group = system_group_id(&ctx, "manager").await;
    let user_group = system_group_id(&ctx, "user").await;

    // ── Seeded system groups and the derived admin context ────────────
    ctx.login_admin().await;
    let body: serde_json::Value = ctx.get("/api/groups").await.json().await.unwrap();
    let groups = body["groups"].as_array().unwrap();
    let admin_json = groups.iter().find(|g| g["kind"] == "admin").unwrap();
    assert_eq!(admin_json["name"], "Admin");
    assert!(admin_json["max_instances"].is_null(), "Admin starts unlimited");
    for flag in [
        "can_create_template",
        "can_manage_users",
        "can_manage_group_instances",
        "can_manage_docker",
        "can_manage_registry",
    ] {
        assert_eq!(admin_json[flag], true, "Admin flags are all on");
    }
    let user_json = groups.iter().find(|g| g["kind"] == "user").unwrap();
    assert_eq!(user_json["name"], "User");
    assert_eq!(user_json["max_instances"], 1);
    for flag in [
        "can_create_template",
        "can_manage_users",
        "can_manage_group_instances",
        "can_manage_docker",
        "can_manage_registry",
    ] {
        assert_eq!(user_json[flag], false, "User flags are all off");
    }
    let manager_json = groups.iter().find(|g| g["kind"] == "manager").unwrap();
    assert_eq!(manager_json["name"], "Manager", "legacy Managers group is renamed");
    assert_eq!(manager_json["max_instances"], 2);

    // The seeded admin is a member of the Admin group → root, unlimited.
    let body: serde_json::Value = ctx.get("/api/auth/me").await.json().await.unwrap();
    let context = &body["context"];
    assert_eq!(context["is_admin"], true);
    assert_eq!(context["tier"], 2);
    assert!(context["group_ids"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!(admin_group)));
    assert_eq!(context["effective_max_instances"], 0);

    // System groups are undeletable and unrenameable.
    assert_eq!(ctx.delete(&format!("/api/groups/{}", admin_group)).await.status(), 403);
    assert_eq!(ctx.delete(&format!("/api/groups/{}", manager_group)).await.status(), 403);
    assert_eq!(ctx.delete(&format!("/api/groups/{}", user_group)).await.status(), 403);
    let resp = ctx
        .put(&format!("/api/groups/{}", manager_group), &serde_json::json!({
            "name": "Managers",
            "description": manager_json["description"],
            "can_create_template": true,
            "can_manage_users": true,
            "can_manage_group_instances": true,
            "can_manage_docker": true,
            "can_manage_registry": true,
            "max_instances": 2,
            "template_ids": manager_json["template_ids"],
        }))
        .await;
    assert_eq!(resp.status(), 403, "system groups cannot be renamed");

    // ── Group-only template authorization ─────────────────────────────
    let tpl1 = create_plain_template(&ctx, template_name("2tpl1").as_str()).await;
    let tpl2 = create_plain_template(&ctx, template_name("2tpl2").as_str()).await;

    // A new template whitelists the Admin group by default.
    let body: serde_json::Value = ctx.get("/api/groups").await.json().await.unwrap();
    let admin_ids = body["groups"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["kind"] == "admin")
        .unwrap()["template_ids"]
        .as_array()
        .unwrap();
    assert!(admin_ids.contains(&serde_json::json!(tpl1)), "default Admin grant");
    assert!(admin_ids.contains(&serde_json::json!(tpl2)), "default Admin grant");

    // Revoke tpl2 from the Admin group → an admin is 403 on it.
    let resp = ctx
        .put(&format!("/api/groups/{}", admin_group), &serde_json::json!({
            "name": "Admin",
            "description": null,
            "can_create_template": true,
            "can_manage_users": true,
            "can_manage_group_instances": true,
            "can_manage_docker": true,
            "can_manage_registry": true,
            "max_instances": 0,
            "template_ids": [tpl1],
        }))
        .await;
    assert_eq!(resp.status(), 200, "admin edits the Admin group whitelist");
    let resp = ctx
        .post("/api/instances", &serde_json::json!({ "template_id": tpl2 }))
        .await;
    assert_eq!(resp.status(), 403, "admins are not exempt from the whitelist");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["rejection"]["scope"], "template_not_allowed");

    // ── Create-user default: the User system group ────────────────────
    let alice_id = create_user(&ctx, "rbac2_alice").await;
    let body: serde_json::Value = ctx.get(&format!("/api/users/{}", alice_id)).await.json().await.unwrap();
    let alice_json = &body["user"];
    assert_eq!(alice_json["tier"], 0);
    assert_eq!(alice_json["is_admin"], false);
    assert!(alice_json["group_ids"]
        .as_array()
        .unwrap()
        .contains(&serde_json::json!(user_group)));

    let carol_id = create_user(&ctx, "rbac2_carol").await;
    assert_eq!(effective_ceiling(&ctx, "rbac2_carol").await, 1, "User group caps at 1");

    // ── Max-rule ceiling: personal raises, never lowers; unlimited wins ──
    ctx.login_admin().await;
    assign_user_policy(&ctx, &carol_id, std::slice::from_ref(&user_group), Some(3)).await;
    assert_eq!(effective_ceiling(&ctx, "rbac2_carol").await, 3, "personal ceiling raises");
    assign_user_policy(&ctx, &carol_id, std::slice::from_ref(&user_group), Some(1)).await;
    assert_eq!(effective_ceiling(&ctx, "rbac2_carol").await, 1, "ties with the group cap");
    assign_user_policy(&ctx, &carol_id, std::slice::from_ref(&user_group), Some(0)).await;
    assert_eq!(effective_ceiling(&ctx, "rbac2_carol").await, 0, "0 = unlimited wins");

    // A group ceiling raises above a lower personal ceiling (never lowers).
    let devs = create_group(&ctx, "rbac2_devs", 5, &[]).await;
    assign_user_policy(&ctx, &carol_id, &[user_group.clone(), devs.clone()], Some(2)).await;
    assert_eq!(effective_ceiling(&ctx, "rbac2_carol").await, 5, "group raises above personal");
    assign_user_policy(&ctx, &carol_id, &[user_group.clone(), devs.clone()], Some(6)).await;
    assert_eq!(effective_ceiling(&ctx, "rbac2_carol").await, 6, "higher personal raises above group");
    // Clear the personal ceiling → back to the group cap.
    ctx.login_admin().await;
    let resp = ctx
        .put(&format!("/api/users/{}", carol_id), &serde_json::json!({
            "group_ids": [user_group.clone()],
            "direct_max_instances": null,
        }))
        .await;
    assert_eq!(resp.status(), 200);
    assert_eq!(effective_ceiling(&ctx, "rbac2_carol").await, 1);

    // ── Manager tier: accounts, policies, and groups ──────────────────
    let admin_id = admin_user_id(&ctx).await;
    // team shares tpl1 with everyone in it (the whitelist grant), so the
    // manager's tier-guard instance control over the tier-0 owner is reachable.
    let team = create_group(&ctx, "rbac2_team", 5, std::slice::from_ref(&tpl1)).await;
    let mike_id = create_user(&ctx, "rbac2_mike").await;
    let mike2_id = create_user(&ctx, "rbac2_mike2").await;
    assign_user_policy(&ctx, &alice_id, &[user_group.clone(), team.clone()], None).await;
    assign_user_policy(&ctx, &mike_id, &[manager_group.clone(), team.clone()], None).await;
    assign_user_policy(&ctx, &mike2_id, &[manager_group.clone(), team.clone()], None).await;

    assert_eq!(ctx.login_user("rbac2_mike", "pw123456").await.status(), 200);
    let body: serde_json::Value = ctx.get("/api/auth/me").await.json().await.unwrap();
    let context = &body["context"];
    assert_eq!(context["tier"], 1);
    assert_eq!(context["is_admin"], false);
    for flag in [
        "can_create_template",
        "can_manage_users",
        "can_manage_group_instances",
        "can_manage_docker",
        "can_manage_registry",
    ] {
        assert_eq!(context[flag], true, "Manager starts with every flag");
    }
    assert_eq!(context["effective_max_instances"], 5, "max(Manager=2, team=5)");

    // A manager creates and manages User accounts.
    let resp = ctx
        .post("/api/users", &serde_json::json!({ "username": "rbac2_dave", "password": "pw123456" }))
        .await;
    assert_eq!(resp.status(), 200, "manager creates a user");
    let dave_id = resp.json::<serde_json::Value>().await.unwrap()["user"]["id"]
        .as_str().unwrap().to_string();
    assert_eq!(
        ctx.put(&format!("/api/users/{}", dave_id), &serde_json::json!({ "direct_max_instances": 4 })).await.status(),
        200
    );
    assert_eq!(
        ctx.put(&format!("/api/users/{}", carol_id), &serde_json::json!({ "direct_max_instances": 4 })).await.status(),
        200,
        "manager manages a tier-0 user's policy"
    );

    // A manager can never touch Admin or fellow-Manager accounts.
    assert_eq!(
        ctx.put(&format!("/api/users/{}", admin_id), &serde_json::json!({ "direct_max_instances": 4 })).await.status(),
        403,
        "manager cannot write an admin's policy"
    );
    assert_eq!(ctx.delete(&format!("/api/users/{}", admin_id)).await.status(), 403);
    assert_eq!(ctx.delete(&format!("/api/users/{}", mike2_id)).await.status(), 403, "no fellow-manager deletion");
    // Nor place anyone into a privileged group.
    assert_eq!(
        ctx.put(&format!("/api/users/{}", dave_id), &serde_json::json!({ "group_ids": [manager_group.clone()] })).await.status(),
        403,
        "manager cannot assign into the Manager group"
    );
    // Group CRUD stays root-only.
    assert_eq!(
        ctx.post("/api/groups", &serde_json::json!({ "name": "rbac2_forged" })).await.status(),
        403
    );
    // A manager may delete a tier-0 user they created.
    assert_eq!(ctx.delete(&format!("/api/users/{}", dave_id)).await.status(), 204);

    // ── Instance tier guardrails (real Docker) ────────────────────────
    ctx.login_admin().await;
    let admin_instance = launch_plain(&ctx, &tpl1).await;
    assert_eq!(ctx.login_user("rbac2_alice", "pw123456").await.status(), 200);
    let alice_instance = launch_plain(&ctx, &tpl1).await;
    assert_eq!(ctx.login_user("rbac2_mike2", "pw123456").await.status(), 200);
    let mike2_instance = launch_plain(&ctx, &tpl1).await;

    // A manager reads/stops/deletes a tier-0 owner's shared-group instance.
    assert_eq!(ctx.login_user("rbac2_mike", "pw123456").await.status(), 200);
    assert_eq!(ctx.get(&format!("/api/instances/{}", alice_instance)).await.status(), 200);
    assert_eq!(
        ctx.post(&format!("/api/instances/{}/stop", alice_instance), &serde_json::json!({})).await.status(),
        200
    );
    assert_eq!(ctx.delete(&format!("/api/instances/{}", alice_instance)).await.status(), 204);

    // ...but never an Admin's or a fellow Manager's instance.
    assert_eq!(ctx.get(&format!("/api/instances/{}", admin_instance)).await.status(), 403);
    assert_eq!(
        ctx.post(&format!("/api/instances/{}/stop", admin_instance), &serde_json::json!({})).await.status(),
        403
    );
    assert_eq!(ctx.delete(&format!("/api/instances/{}", admin_instance)).await.status(), 403);
    assert_eq!(ctx.get(&format!("/api/instances/{}", mike2_instance)).await.status(), 403);
    assert_eq!(
        ctx.post(&format!("/api/instances/{}/stop", mike2_instance), &serde_json::json!({})).await.status(),
        403
    );

    // An admin governs every tier's instances and accounts.
    ctx.login_admin().await;
    assert_eq!(ctx.delete(&format!("/api/instances/{}", mike2_instance)).await.status(), 204);
    assert_eq!(ctx.delete(&format!("/api/instances/{}", admin_instance)).await.status(), 204);
    assert_eq!(ctx.delete(&format!("/api/users/{}", mike2_id)).await.status(), 204);
    assert_eq!(ctx.delete(&format!("/api/users/{}", mike_id)).await.status(), 204, "admin deletes a manager");
    assert_eq!(ctx.delete(&format!("/api/users/{}", alice_id)).await.status(), 204);
    assert_eq!(ctx.delete(&format!("/api/users/{}", carol_id)).await.status(), 204);
}

/// Create a template with an explicit visibility, returning its id.
async fn create_template_with_visibility(ctx: &TestContext, name: &str, visibility: &str) -> String {
    ctx.login_admin().await;
    let resp = ctx
        .post("/api/templates", &serde_json::json!({
            "name": name,
            "visibility": visibility,
        }))
        .await;
    assert_eq!(resp.status(), 200, "create template with visibility failed");
    let body: serde_json::Value = resp.json().await.unwrap();
    body["template"]["id"].as_str().unwrap().to_string()
}

/// The template-visibility story against the real backend and real Docker:
/// a `public` template launches for a user with no template grants; a
/// `hidden` template is excluded from `allowed_template_ids` and rejected for
/// a group-whitelisted member and for the admin (no bypass); a template
/// created without the field stays `private`, so the group whitelist governs
/// exactly as before.
#[tokio::test]
async fn test_template_visibility_end_to_end() {
    let ctx = TestContext::new().await;
    common::ensure_network().await;

    // Admin creates one template per visibility.
    let tpl_public = create_template_with_visibility(&ctx, &template_name("vis_pub"), "public").await;
    let tpl_hidden = create_template_with_visibility(&ctx, &template_name("vis_hid"), "hidden").await;
    let tpl_plain = create_plain_template(&ctx, &template_name("vis_plain")).await;

    // The catalog exposes the visibility on every template — including hidden
    // ones, so the templates-management UI can display and restore them.
    let body: serde_json::Value = ctx.get("/api/templates").await.json().await.unwrap();
    let by_id = |id: &str| {
        body["templates"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == serde_json::json!(id))
            .unwrap()
            .clone()
    };
    assert_eq!(by_id(&tpl_public)["visibility"], "public");
    assert_eq!(by_id(&tpl_hidden)["visibility"], "hidden");
    assert_eq!(by_id(&tpl_plain)["visibility"], "private", "absent field defaults to private");

    // A user with no template grants (the seeded User system group's whitelist
    // is empty) launches the public template for real.
    let no_grant_id = create_user(&ctx, "vis_nogrant").await;
    assert_eq!(ctx.login_user("vis_nogrant", "pw123456").await.status(), 200);
    let body: serde_json::Value = ctx.get("/api/auth/me").await.json().await.unwrap();
    assert_eq!(
        body["context"]["allowed_template_ids"].as_array().unwrap().len(),
        0,
        "no grants anywhere"
    );
    let public_instance = launch_plain(&ctx, &tpl_public).await;

    // A group-whitelisted member is still refused a hidden template: the hidden
    // template is excluded from `allowed_template_ids` by the API (so the
    // client never advertises it), and a direct launch attempt carries the
    // machine-readable template_hidden scope.
    let group_whitelist = create_group(&ctx, "vis_team", 1, std::slice::from_ref(&tpl_hidden)).await;
    let member_id = create_user(&ctx, "vis_member").await;
    assign_user_policy(&ctx, &member_id, &[group_whitelist], None).await;
    assert_eq!(ctx.login_user("vis_member", "pw123456").await.status(), 200);
    let body: serde_json::Value = ctx.get("/api/auth/me").await.json().await.unwrap();
    let allowed = body["context"]["allowed_template_ids"].as_array().unwrap();
    assert!(
        !allowed.contains(&serde_json::json!(tpl_hidden)),
        "member's whitelisted hidden template must not appear in allowed_template_ids"
    );
    let resp = ctx
        .post("/api/instances", &serde_json::json!({ "template_id": tpl_hidden }))
        .await;
    assert_eq!(resp.status(), 403, "hidden template must reject a whitelisted member");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["rejection"]["scope"], "template_hidden");

    // The admin gets the same refusal — visibility is not bypassable.
    assert_eq!(ctx.login_admin().await.status(), 200);
    let resp = ctx
        .post("/api/instances", &serde_json::json!({ "template_id": tpl_hidden }))
        .await;
    assert_eq!(resp.status(), 403, "hidden template must reject the admin too");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["rejection"]["scope"], "template_hidden");

    // A private template stays whitelist-governed: the member (whose group only
    // whitelists the hidden template) is refused it.
    assert_eq!(ctx.login_user("vis_member", "pw123456").await.status(), 200);
    let resp = ctx
        .post("/api/instances", &serde_json::json!({ "template_id": tpl_plain }))
        .await;
    assert_eq!(resp.status(), 403, "private template outside the whitelist");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["rejection"]["scope"], "template_not_allowed");

    // A group that whitelists the private template can launch it for real.
    ctx.login_admin().await;
    let whitelist_group = create_group(&ctx, "vis_trusted", 1, std::slice::from_ref(&tpl_plain)).await;
    let trusted_id = create_user(&ctx, "vis_trusted").await;
    assign_user_policy(&ctx, &trusted_id, &[whitelist_group], None).await;
    assert_eq!(ctx.login_user("vis_trusted", "pw123456").await.status(), 200);
    let private_instance = launch_plain(&ctx, &tpl_plain).await;

    // Clean up the real containers and accounts so the run leaves no residue.
    assert_eq!(ctx.login_admin().await.status(), 200);
    assert_eq!(ctx.delete(&format!("/api/instances/{}", public_instance)).await.status(), 204);
    assert_eq!(ctx.delete(&format!("/api/instances/{}", private_instance)).await.status(), 204);
    assert_eq!(ctx.delete(&format!("/api/users/{}", no_grant_id)).await.status(), 204);
    assert_eq!(ctx.delete(&format!("/api/users/{}", member_id)).await.status(), 204);
    assert_eq!(ctx.delete(&format!("/api/users/{}", trusted_id)).await.status(), 204);
}

#[tokio::test]
async fn test_context_helpers() {
    let ctx = TestContext::new().await;
    let _ = ctx.login_user("admin", "admin").await;
    let _ = ctx.put("/health", &serde_json::json!({})).await;
    let _ = ctx.login_token().await;
    let _ = &ctx.base_url;
    let _ = &ctx.client;
    let _ = &ctx.db_name;
}
