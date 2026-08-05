#![cfg(feature = "docker")]

mod common;

use std::path::Path;

use common::TestContext;
use openworkspace_api::db::{
    workspace_instance, PersistentVolume, PersistentVolumeRepository, UserRepository,
    WorkspaceInstanceRepository, VOLUME_STATUS_ACTIVE, VOLUME_STATUS_ORPHANED,
};
use openworkspace_api::persistent_volume::{
    persistent_volume_name, resolve_persistent_host_path,
};
use sea_orm::{ColumnTrait, EntityTrait, QueryFilter, Set};

const PERSISTENT_ROOT: &str = "/tmp/ow_pv_root";

fn template_name(suffix: &str) -> String {
    format!("ow_test_vol_{}_{}", std::process::id(), suffix)
}

/// A per-test persistent root, unique across the suite (tests in the same
/// binary share a process id but never a suffix), so each test can tear its
/// whole host-data tree down without racing a concurrently-running test.
fn persistent_root(suffix: &str) -> String {
    format!("{}_{}_{}", PERSISTENT_ROOT, std::process::id(), suffix)
}

async fn admin_id(ctx: &TestContext) -> uuid::Uuid {
    let db = sea_orm::Database::connect(&common::pg_url(&ctx.db_name))
        .await
        .unwrap();
    let id = UserRepository::new(&db)
        .find_by_username("admin")
        .await
        .unwrap()
        .unwrap()
        .id;
    drop(db);
    id
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

async fn launch_persistent(ctx: &TestContext, template_id: &str, persistence: &str) -> String {
    let resp = ctx
        .post("/api/instances", &serde_json::json!({
            "template_id": template_id,
            "persistence": persistence,
        }))
        .await;
    assert_eq!(resp.status(), 200, "launch failed");
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

fn resolved_host_path(root: &str, template_name: &str, owner_id: &uuid::Uuid) -> String {
    resolve_persistent_host_path(root, template_name, &owner_id.to_string()).unwrap()
}

/// Remove the per-test host-data tree: the leaf data dir, its template parent,
/// and the per-test root (unique to this test, so removal is race-free).
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

async fn cleanup_volume(ctx: &TestContext, host_path: &str) {
    ctx.login_admin().await;
    let volume = find_volume(ctx, host_path)
        .await
        .expect("registry row must exist for cleanup");
    let resp = ctx
        .post(
            &format!("/api/persistent-volumes/{}/cleanup", volume.id),
            &serde_json::json!({}),
        )
        .await;
    assert_eq!(resp.status(), 204, "cleanup must succeed");
    assert!(
        find_volume(ctx, host_path).await.is_none(),
        "cleanup must delete the registry row"
    );
    let docker = bollard::Docker::connect_with_local_defaults().unwrap();
    let _ = docker
        .remove_volume(
            &persistent_volume_name(host_path),
            None::<bollard::volume::RemoveVolumeOptions>,
        )
        .await;
    remove_host_dirs(host_path);
}

async fn create_plain_user(ctx: &TestContext, username: &str) -> String {
    ctx.login_admin().await;
    let resp = ctx
        .post("/api/users", &serde_json::json!({
            "username": username,
            "password": "pw123456",
        }))
        .await;
    assert_eq!(resp.status(), 200, "create user failed");
    resp.json::<serde_json::Value>()
        .await
        .unwrap()["user"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn create_group(ctx: &TestContext, name: &str, flags: serde_json::Value) -> String {
    ctx.login_admin().await;
    let mut group = flags.as_object().cloned().unwrap_or_default();
    group.insert("name".to_string(), serde_json::Value::String(name.to_string()));
    group.insert("max_instances".to_string(), serde_json::json!(5));
    let resp = ctx.post("/api/groups", &serde_json::Value::Object(group)).await;
    assert_eq!(resp.status(), 200, "create group failed");
    resp.json::<serde_json::Value>()
        .await
        .unwrap()["group"]["id"]
        .as_str()
        .unwrap()
        .to_string()
}

async fn assign_group(ctx: &TestContext, user_id: &str, group_ids: Vec<String>) {
    ctx.login_admin().await;
    let resp = ctx
        .put(&format!("/api/users/{}", user_id), &serde_json::json!({
            "group_ids": group_ids,
        }))
        .await;
    assert_eq!(resp.status(), 200, "assign group failed");
}

// ── Launch upserts the registry row keyed by the resolved host path ──

#[tokio::test]
async fn test_persistent_launch_upserts_registry_row() {
    let ctx = TestContext::new().await;
    let template_id = create_persistent_template(&ctx, "upsert").await;
    let tpl_name = template_name("upsert");
    let instance_id = launch_persistent(&ctx, &template_id, "use_persistent").await;

    let owner = admin_id(&ctx).await;
    let host_path = resolved_host_path(&persistent_root("upsert"), &tpl_name, &owner);

    let volume = find_volume(&ctx, &host_path)
        .await
        .expect("persistent launch must upsert a registry row");
    assert_eq!(volume.owner_id, Some(owner));
    assert_eq!(volume.status, VOLUME_STATUS_ACTIVE);

    // A referenced (active) volume must not appear in the orphaned view.
    let resp = ctx.get("/api/persistent-volumes").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(
        body["volumes"].as_array().unwrap().is_empty(),
        "active volume must not be listed as orphaned"
    );

    assert_eq!(
        ctx.delete(&format!("/api/instances/{}", instance_id)).await.status(),
        204
    );
    cleanup_volume(&ctx, &host_path).await;
}

// ── Deleting the last referencing instance flips the row to orphaned ──

#[tokio::test]
async fn test_delete_last_referencing_instance_flips_to_orphaned() {
    let ctx = TestContext::new().await;
    let template_id = create_persistent_template(&ctx, "orphan").await;
    let tpl_name = template_name("orphan");
    let instance_id = launch_persistent(&ctx, &template_id, "use_persistent").await;

    let owner = admin_id(&ctx).await;
    let host_path = resolved_host_path(&persistent_root("orphan"), &tpl_name, &owner);

    // Build a second live reference at the DB level (the API refuses two
    // persistent instances for the same template/owner), so the multi-reference
    // branch of the status sync is exercised before the last delete.
    let db = sea_orm::Database::connect(&common::pg_url(&ctx.db_name))
        .await
        .unwrap();
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let db_reference = instance_repo
        .launch(
            uuid::Uuid::parse_str(&template_id).unwrap(),
            owner,
            &tpl_name,
            true,
            Some(&host_path),
        )
        .await
        .unwrap();
    let am = workspace_instance::ActiveModel {
        id: Set(db_reference.id),
        status: Set("running".to_string()),
        ..Default::default()
    };
    workspace_instance::Entity::update_many()
        .set(am)
        .filter(workspace_instance::Column::Id.eq(db_reference.id))
        .exec(&db)
        .await
        .unwrap();

    // Removing the DB-level reference alone keeps the row `active`.
    workspace_instance::Entity::delete_by_id(db_reference.id)
        .exec(&db)
        .await
        .unwrap();
    PersistentVolumeRepository::new(&db)
        .sync_status_for_host_path(&host_path)
        .await
        .unwrap();
    assert_eq!(
        find_volume(&ctx, &host_path).await.unwrap().status,
        VOLUME_STATUS_ACTIVE,
        "another active reference must keep the row active"
    );
    drop(db);

    // Deleting the last referencing instance through the route flips the row.
    assert_eq!(
        ctx.delete(&format!("/api/instances/{}", instance_id)).await.status(),
        204
    );
    let volume = find_volume(&ctx, &host_path).await.unwrap();
    assert_eq!(volume.status, VOLUME_STATUS_ORPHANED);

    // The orphaned view reports the pinned JSON shape with the owner.
    let resp = ctx.get("/api/persistent-volumes").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let volumes = body["volumes"].as_array().unwrap();
    assert_eq!(volumes.len(), 1);
    let v = &volumes[0];
    assert_eq!(v["id"], volume.id.to_string());
    assert_eq!(v["host_path"], host_path);
    assert_eq!(v["owner_id"], owner.to_string());
    assert_eq!(v["owner_username"], "admin");
    assert_eq!(v["status"], VOLUME_STATUS_ORPHANED);
    assert!(v["created_at"].is_string());

    cleanup_volume(&ctx, &host_path).await;
}

// ── Deleting a user nulls the owner but keeps the row ──

#[tokio::test]
async fn test_deleting_user_nulls_owner_keeps_row() {
    let ctx = TestContext::new().await;
    let template_id = create_persistent_template(&ctx, "userdel").await;
    let tpl_name = template_name("userdel");

    // A plain user launches persistently — whitelist the template to them via
    // a group so the launch passes the default-deny pre-flight.
    let user_id = create_plain_user(&ctx, "pv_userdel").await;
    let group_id = create_group(
        &ctx,
        "pv_whitelist",
        serde_json::json!({
            "description": null,
            "can_create_template": false,
            "can_manage_users": false,
            "can_manage_group_instances": false,
            "can_manage_docker": false,
            "can_manage_registry": false,
            "template_ids": [template_id],
        }),
    )
    .await;
    assign_group(&ctx, &user_id, vec![group_id]).await;

    let resp = ctx.login_user("pv_userdel", "pw123456").await;
    assert_eq!(resp.status(), 200);
    let instance_id = launch_persistent(&ctx, &template_id, "use_persistent").await;

    let user_uuid = uuid::Uuid::parse_str(&user_id).unwrap();
    let host_path = resolved_host_path(&persistent_root("userdel"), &tpl_name, &user_uuid);
    let volume = find_volume(&ctx, &host_path).await.unwrap();
    assert_eq!(volume.owner_id, Some(user_uuid));
    assert_eq!(volume.status, VOLUME_STATUS_ACTIVE);

    let volume_name = persistent_volume_name(&host_path);
    let docker = bollard::Docker::connect_with_local_defaults().unwrap();

    // Delete the instance first (row becomes orphaned), then the user.
    assert_eq!(
        ctx.delete(&format!("/api/instances/{}", instance_id)).await.status(),
        204
    );
    ctx.login_admin().await;
    assert_eq!(ctx.delete(&format!("/api/users/{}", user_id)).await.status(), 204);

    let volume = find_volume(&ctx, &host_path)
        .await
        .expect("deleting the user must NOT delete the registry row");
    assert_eq!(volume.owner_id, None, "owner must be nulled, never the row deleted");
    assert_eq!(volume.status, VOLUME_STATUS_ORPHANED);

    assert!(
        docker.inspect_volume(&volume_name).await.is_ok(),
        "deleting a user must not remove the volume"
    );
    assert!(
        Path::new(&host_path).exists(),
        "deleting a user must not remove host data"
    );

    let resp = ctx.get("/api/persistent-volumes").await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let volumes = body["volumes"].as_array().unwrap();
    let v = volumes
        .iter()
        .find(|v| v["host_path"] == host_path)
        .expect("orphan with nulled owner must be listed");
    assert_eq!(v["owner_id"], serde_json::Value::Null);
    assert_eq!(v["owner_username"], serde_json::Value::Null);

    cleanup_volume(&ctx, &host_path).await;
}

// ── Re-launching an orphaned path reactivates the row; reset wipes ──

#[tokio::test]
async fn test_relaunch_orphaned_path_reactivates_and_reset_wipes() {
    let ctx = TestContext::new().await;
    let template_id = create_persistent_template(&ctx, "react").await;
    let tpl_name = template_name("react");
    let instance_id = launch_persistent(&ctx, &template_id, "use_persistent").await;

    let owner = admin_id(&ctx).await;
    let host_path = resolved_host_path(&persistent_root("react"), &tpl_name, &owner);
    let data_file = Path::new(&host_path).join("keepme.txt");

    std::fs::write(&data_file, "data").unwrap();
    assert_eq!(
        ctx.delete(&format!("/api/instances/{}", instance_id)).await.status(),
        204
    );
    assert_eq!(
        find_volume(&ctx, &host_path).await.unwrap().status,
        VOLUME_STATUS_ORPHANED
    );

    // A plain re-launch reuses and reactivates the row without wiping data.
    let instance_id2 = launch_persistent(&ctx, &template_id, "use_persistent").await;
    let volume = find_volume(&ctx, &host_path).await.unwrap();
    assert_eq!(volume.status, VOLUME_STATUS_ACTIVE, "re-launch must reactivate the row");
    assert_eq!(volume.owner_id, Some(owner));
    assert!(data_file.exists(), "a plain re-launch must keep preserved data");
    assert_eq!(
        ctx.delete(&format!("/api/instances/{}", instance_id2)).await.status(),
        204
    );

    // reset_persistent explicitly wipes host data, then re-prepares.
    std::fs::write(&data_file, "data").unwrap();
    let instance_id3 = launch_persistent(&ctx, &template_id, "reset_persistent").await;
    assert!(
        !data_file.exists(),
        "reset_persistent must wipe host data before re-preparing"
    );
    assert_eq!(
        find_volume(&ctx, &host_path).await.unwrap().status,
        VOLUME_STATUS_ACTIVE
    );

    assert_eq!(
        ctx.delete(&format!("/api/instances/{}", instance_id3)).await.status(),
        204
    );
    cleanup_volume(&ctx, &host_path).await;
}

// ── Orphaned list + cleanup gating: admins + can_manage_users only ──

#[tokio::test]
async fn test_orphaned_view_and_cleanup_gating() {
    let ctx = TestContext::new().await;

    // A group carrying ONLY can_manage_group_instances: its members must not
    // see or clean up orphaned volumes.
    let instances_group = create_group(
        &ctx,
        "pv_only_instances",
        serde_json::json!({ "can_manage_group_instances": true }),
    )
    .await;
    // A group carrying can_manage_users: its members may.
    let user_mgr_group = create_group(
        &ctx,
        "pv_user_managers",
        serde_json::json!({ "can_manage_users": true }),
    )
    .await;

    let _plain = create_plain_user(&ctx, "pv_plain").await;
    let inst_mgr = create_plain_user(&ctx, "pv_instmgr").await;
    let user_mgr = create_plain_user(&ctx, "pv_usermgr").await;
    assign_group(&ctx, &inst_mgr, vec![instances_group]).await;
    assign_group(&ctx, &user_mgr, vec![user_mgr_group]).await;

    // Orphaned volume A: persistent launch by admin, then delete the instance.
    let template_a = create_persistent_template(&ctx, "gate_a").await;
    let tpl_a_name = template_name("gate_a");
    let instance_a = launch_persistent(&ctx, &template_a, "use_persistent").await;
    let owner = admin_id(&ctx).await;
    let host_path_a = resolved_host_path(&persistent_root("gate_a"), &tpl_a_name, &owner);
    assert_eq!(
        ctx.delete(&format!("/api/instances/{}", instance_a)).await.status(),
        204
    );
    let volume_a = find_volume(&ctx, &host_path_a).await.unwrap();
    assert_eq!(volume_a.status, VOLUME_STATUS_ORPHANED);

    // A plain user is denied both the list and the cleanup.
    let resp = ctx.login_user("pv_plain", "pw123456").await;
    assert_eq!(resp.status(), 200);
    assert_eq!(ctx.get("/api/persistent-volumes").await.status(), 403);
    let resp = ctx
        .post(
            &format!("/api/persistent-volumes/{}/cleanup", volume_a.id),
            &serde_json::json!({}),
        )
        .await;
    assert_eq!(resp.status(), 403, "a plain user must not clean up volumes");

    // can_manage_group_instances alone grants neither.
    let resp = ctx.login_user("pv_instmgr", "pw123456").await;
    assert_eq!(resp.status(), 200);
    assert_eq!(
        ctx.get("/api/persistent-volumes").await.status(),
        403,
        "can_manage_group_instances alone must not grant the orphaned view"
    );
    let resp = ctx
        .post(
            &format!("/api/persistent-volumes/{}/cleanup", volume_a.id),
            &serde_json::json!({}),
        )
        .await;
    assert_eq!(resp.status(), 403);

    // A system admin sees the orphaned list and can clean up.
    let resp = ctx.login_admin().await;
    assert_eq!(resp.status(), 200);
    let resp = ctx.get("/api/persistent-volumes").await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(
        body["volumes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["host_path"] == host_path_a)
    );
    let resp = ctx
        .post(
            &format!("/api/persistent-volumes/{}/cleanup", volume_a.id),
            &serde_json::json!({}),
        )
        .await;
    assert_eq!(resp.status(), 204, "admin cleanup must succeed");
    assert!(find_volume(&ctx, &host_path_a).await.is_none());
    remove_host_dirs(&host_path_a);

    // A can_manage_users holder sees the orphaned list and can clean up.
    let template_b = create_persistent_template(&ctx, "gate_b").await;
    let tpl_b_name = template_name("gate_b");
    let instance_b = launch_persistent(&ctx, &template_b, "use_persistent").await;
    let host_path_b = resolved_host_path(&persistent_root("gate_b"), &tpl_b_name, &owner);
    assert_eq!(
        ctx.delete(&format!("/api/instances/{}", instance_b)).await.status(),
        204
    );
    let volume_b = find_volume(&ctx, &host_path_b).await.unwrap();

    let resp = ctx.login_user("pv_usermgr", "pw123456").await;
    assert_eq!(resp.status(), 200);
    let resp = ctx.get("/api/persistent-volumes").await;
    assert_eq!(resp.status(), 200, "can_manage_users holder must see the orphaned view");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(
        body["volumes"]
            .as_array()
            .unwrap()
            .iter()
            .any(|v| v["host_path"] == host_path_b)
    );
    let resp = ctx
        .post(
            &format!("/api/persistent-volumes/{}/cleanup", volume_b.id),
            &serde_json::json!({}),
        )
        .await;
    assert_eq!(resp.status(), 204, "can_manage_users holder cleanup must succeed");
    assert!(find_volume(&ctx, &host_path_b).await.is_none());
    remove_host_dirs(&host_path_b);
}

// ── Cleanup removes host data + volume + row; active is rejected ──

#[tokio::test]
async fn test_cleanup_removes_data_and_rejects_active() {
    let ctx = TestContext::new().await;
    let template_id = create_persistent_template(&ctx, "cleanup").await;
    let tpl_name = template_name("cleanup");
    let instance_id = launch_persistent(&ctx, &template_id, "use_persistent").await;

    let owner = admin_id(&ctx).await;
    let host_path = resolved_host_path(&persistent_root("cleanup"), &tpl_name, &owner);
    let volume_name = persistent_volume_name(&host_path);
    let docker = bollard::Docker::connect_with_local_defaults().unwrap();

    // Cleanup on an active (still-referenced) volume is rejected with 409.
    let volume = find_volume(&ctx, &host_path).await.unwrap();
    assert_eq!(volume.status, VOLUME_STATUS_ACTIVE);
    let resp = ctx
        .post(
            &format!("/api/persistent-volumes/{}/cleanup", volume.id),
            &serde_json::json!({}),
        )
        .await;
    assert_eq!(resp.status(), 409);
    assert!(
        find_volume(&ctx, &host_path).await.is_some(),
        "a rejected cleanup must leave the row in place"
    );
    assert!(
        docker.inspect_volume(&volume_name).await.is_ok(),
        "a rejected cleanup must keep the volume"
    );

    assert_eq!(
        ctx.delete(&format!("/api/instances/{}", instance_id)).await.status(),
        204
    );
    let volume = find_volume(&ctx, &host_path).await.unwrap();
    assert_eq!(volume.status, VOLUME_STATUS_ORPHANED);

    // Seed host data to prove the cleanup empties it.
    std::fs::write(Path::new(&host_path).join("doomed.txt"), "data").unwrap();
    assert!(docker.inspect_volume(&volume_name).await.is_ok());

    let resp = ctx
        .post(
            &format!("/api/persistent-volumes/{}/cleanup", volume.id),
            &serde_json::json!({}),
        )
        .await;
    assert_eq!(resp.status(), 204);
    assert!(
        docker.inspect_volume(&volume_name).await.is_err(),
        "cleanup must remove the Docker volume"
    );
    assert!(
        !Path::new(&host_path).join("doomed.txt").exists(),
        "cleanup must empty host data"
    );
    assert!(
        find_volume(&ctx, &host_path).await.is_none(),
        "cleanup must delete the registry row"
    );

    let _ = docker
        .remove_volume(&volume_name, None::<bollard::volume::RemoveVolumeOptions>)
        .await;
    remove_host_dirs(&host_path);
}

// ── No lifecycle path ever removes host data or the volume ──

#[tokio::test]
async fn test_no_lifecycle_path_removes_host_data() {
    let ctx = TestContext::new().await;
    let template_id = create_persistent_template(&ctx, "keep").await;
    let tpl_name = template_name("keep");
    let instance_id = launch_persistent(&ctx, &template_id, "use_persistent").await;

    let owner = admin_id(&ctx).await;
    let host_path = resolved_host_path(&persistent_root("keep"), &tpl_name, &owner);
    let volume_name = persistent_volume_name(&host_path);
    let docker = bollard::Docker::connect_with_local_defaults().unwrap();

    // stop / start: neither removes the volume or the host data.
    let resp = ctx
        .post(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({}))
        .await;
    assert_eq!(resp.status(), 200, "stop failed: {:?}", resp.text().await);
    assert!(docker.inspect_volume(&volume_name).await.is_ok(), "stop must keep the volume");
    assert!(Path::new(&host_path).exists(), "stop must keep host data");

    let resp = ctx
        .post(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({}))
        .await;
    assert_eq!(resp.status(), 200, "start failed: {:?}", resp.text().await);
    assert!(docker.inspect_volume(&volume_name).await.is_ok(), "start must keep the volume");

    // delete instance keeps volume + host data for reuse.
    assert_eq!(
        ctx.delete(&format!("/api/instances/{}", instance_id)).await.status(),
        204
    );
    assert!(
        docker.inspect_volume(&volume_name).await.is_ok(),
        "delete must keep the volume for reuse"
    );
    assert!(Path::new(&host_path).exists(), "delete must keep host data");
    assert_eq!(
        find_volume(&ctx, &host_path).await.unwrap().status,
        VOLUME_STATUS_ORPHANED
    );

    // Only the explicit cleanup endpoint removes it.
    cleanup_volume(&ctx, &host_path).await;
}

// ── Cleanup of an unknown volume id → 404 ──

#[tokio::test]
async fn test_cleanup_unknown_volume_404() {
    let ctx = TestContext::new().await;
    ctx.login_admin().await;

    let resp = ctx
        .post(
            "/api/persistent-volumes/00000000-0000-0000-0000-00000000dead/cleanup",
            &serde_json::json!({}),
        )
        .await;
    assert_eq!(resp.status(), 404);
}

#[tokio::test]
async fn test_context_helpers() {
    let ctx = TestContext::new().await;
    let _ = ctx.login_user("admin", "admin").await;
    let _ = ctx.put("/health", &serde_json::json!({})).await;
    let _ = ctx.login_token().await;
}
