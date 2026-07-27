mod common;

use openworkspace_api::db::*;
use sea_orm::DatabaseConnection;
use migration::MigratorTrait;

async fn setup_db() -> DatabaseConnection {
    common::ensure_pg().await;

    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let db_name = format!("db_test_{}_{:04}", std::process::id(), counter);

    let base_url = common::pg_base_url();
    let (client, conn) = tokio_postgres::connect(&base_url, tokio_postgres::NoTls)
        .await
        .expect("failed to connect");
    tokio::spawn(conn);

    let _ = client
        .execute(
            &format!(
                "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{}'",
                db_name
            )[..],
            &[],
        )
        .await;
    let _ = client
        .execute(&format!("DROP DATABASE IF EXISTS \"{}\"", db_name)[..], &[])
        .await;

    client
        .execute(&format!("CREATE DATABASE \"{}\"", db_name)[..], &[])
        .await
        .expect("failed to create test database");

    let db_url = common::pg_url(&db_name);

    let migrator_db = sea_orm::Database::connect(&db_url)
        .await
        .expect("failed to connect for migrations");
    migration::Migrator::up(&migrator_db, None)
        .await
        .expect("failed to run migrations");
    drop(migrator_db);

    sea_orm::Database::connect(&db_url)
        .await
        .expect("failed to connect")
}

// ── UserRepository tests ──────────────────────────────────────

#[tokio::test]
async fn user_seed_admin_creates_admin() {
    let db = setup_db().await;
    let repo = UserRepository::new(&db);

    repo.seed_admin("testpass").await.unwrap();

    let user = repo.find_by_username("admin").await.unwrap();
    assert!(user.is_some());
    let (id, username, _hash, role) = user.unwrap();
    assert_eq!(username, "admin");
    assert_eq!(role, "admin");
    assert!(!id.is_nil());
}

#[tokio::test]
async fn user_seed_admin_idempotent() {
    let db = setup_db().await;
    let repo = UserRepository::new(&db);

    repo.seed_admin("pass1").await.unwrap();
    repo.seed_admin("pass2").await.unwrap();

    let user = repo.find_by_username("admin").await.unwrap();
    assert!(user.is_some());
}

#[tokio::test]
async fn user_create_and_find_by_id() {
    let db = setup_db().await;
    let repo = UserRepository::new(&db);

    let id = repo.create("alice", "hash123", "user").await.unwrap();

    let found = repo.find_by_id(id).await.unwrap();
    assert!(found.is_some());
    let (found_id, username, password_hash, role, _created_at) = found.unwrap();
    assert_eq!(found_id, id);
    assert_eq!(username, "alice");
    assert_eq!(password_hash, "hash123");
    assert_eq!(role, "user");
}

#[tokio::test]
async fn user_find_by_username_not_found() {
    let db = setup_db().await;
    let repo = UserRepository::new(&db);

    let result = repo.find_by_username("nonexistent").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn user_list_all() {
    let db = setup_db().await;
    let repo = UserRepository::new(&db);

    repo.create("user1", "h1", "user").await.unwrap();
    repo.create("user2", "h2", "admin").await.unwrap();

    let users = repo.list_all().await.unwrap();
    assert_eq!(users.len(), 2);
    let names: Vec<&str> = users.iter().map(|u| u.1.as_str()).collect();
    assert!(names.contains(&"user1"));
    assert!(names.contains(&"user2"));
}

#[tokio::test]
async fn user_delete() {
    let db = setup_db().await;
    let repo = UserRepository::new(&db);

    let id = repo.create("deleteme", "h", "user").await.unwrap();
    let deleted = repo.delete(id).await.unwrap();
    assert!(deleted);

    let found = repo.find_by_id(id).await.unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn user_delete_nonexistent() {
    let db = setup_db().await;
    let repo = UserRepository::new(&db);

    let uuid = uuid::Uuid::new_v4();
    let deleted = repo.delete(uuid).await.unwrap();
    assert!(!deleted);
}

// ── WorkspaceConfigRepository tests ───────────────────────────

#[tokio::test]
async fn config_create_and_find() {
    let db = setup_db().await;
    let config_repo = WorkspaceConfigRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = config_repo
        .create(
            "my-config",
            Some("A test config"),
            admin.0,
            "ubuntu:22.04",
            4,
            8_589_934_592,
            1,
            None,
            &serde_json::json!({"cmd": ["sleep", "infinity"]}),
            &serde_json::json!({}),
            &serde_json::json!({"bind": "/data"}),
            Some("/host/data"),
        )
        .await
        .unwrap();

    assert_eq!(config.name, "my-config");
    assert_eq!(config.image, "ubuntu:22.04");
    assert_eq!(config.cores, 4);
    assert_eq!(config.memory, 8_589_934_592);
    assert_eq!(config.gpu_count, 1);
    assert_eq!(config.description, Some("A test config".to_string()));
    assert_eq!(config.persistent_storage_path, Some("/host/data".to_string()));

    let found = config_repo.find_by_id(config.id).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "my-config");
}

#[tokio::test]
async fn config_list_by_owner() {
    let db = setup_db().await;
    let config_repo = WorkspaceConfigRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    config_repo
        .create("cfg1", None, admin.0, "img:1", 1, 1024, 0, None, &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None)
        .await
        .unwrap();
    config_repo
        .create("cfg2", None, admin.0, "img:2", 2, 2048, 0, None, &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None)
        .await
        .unwrap();

    let configs = config_repo.list_by_owner(admin.0).await.unwrap();
    assert_eq!(configs.len(), 2);
}

#[tokio::test]
async fn config_list_all() {
    let db = setup_db().await;
    let config_repo = WorkspaceConfigRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    config_repo
        .create("cfg1", None, admin.0, "img:1", 1, 1024, 0, None, &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None)
        .await
        .unwrap();

    let configs = config_repo.list_all().await.unwrap();
    assert_eq!(configs.len(), 1);
}

#[tokio::test]
async fn config_update() {
    let db = setup_db().await;
    let config_repo = WorkspaceConfigRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = config_repo
        .create("old-name", None, admin.0, "old:img", 1, 1024, 0, None, &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None)
        .await
        .unwrap();

    let updated = config_repo
        .update(
            config.id,
            "new-name",
            Some("updated desc"),
            "new:img",
            8,
            16_777_216,
            2,
            Some("my-registry"),
            &serde_json::json!({"key": "val"}),
            &serde_json::json!({"exec": true}),
            &serde_json::json!({"vol": "/mnt"}),
            Some("/new/path"),
        )
        .await
        .unwrap();
    assert!(updated);

    let found = config_repo.find_by_id(config.id).await.unwrap().unwrap();
    assert_eq!(found.name, "new-name");
    assert_eq!(found.image, "new:img");
    assert_eq!(found.cores, 8);
    assert_eq!(found.gpu_count, 2);
    assert_eq!(found.docker_registry, Some("my-registry".to_string()));
    assert_eq!(found.description, Some("updated desc".to_string()));
    assert_eq!(found.persistent_storage_path, Some("/new/path".to_string()));
}

#[tokio::test]
async fn config_delete() {
    let db = setup_db().await;
    let config_repo = WorkspaceConfigRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = config_repo
        .create("del", None, admin.0, "img:1", 1, 1024, 0, None, &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None)
        .await
        .unwrap();

    let deleted = config_repo.delete(config.id).await.unwrap();
    assert!(deleted);
    assert!(config_repo.find_by_id(config.id).await.unwrap().is_none());
}

#[tokio::test]
async fn config_delete_nonexistent() {
    let db = setup_db().await;
    let config_repo = WorkspaceConfigRepository::new(&db);

    let deleted = config_repo.delete(uuid::Uuid::new_v4()).await.unwrap();
    assert!(!deleted);
}

#[tokio::test]
async fn config_count_instances() {
    let db = setup_db().await;
    let config_repo = WorkspaceConfigRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = config_repo
        .create("counted", None, admin.0, "img:1", 1, 1024, 0, None, &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None)
        .await
        .unwrap();

    assert_eq!(config_repo.count_instances(config.id).await.unwrap(), 0);

    instance_repo.launch(config.id, admin.0, "counted", false, None).await.unwrap();
    instance_repo.launch(config.id, admin.0, "counted", false, None).await.unwrap();

    assert_eq!(config_repo.count_instances(config.id).await.unwrap(), 2);
}

// ── WorkspaceInstanceRepository tests ─────────────────────────

#[tokio::test]
async fn instance_launch_and_find() {
    let db = setup_db().await;
    let config_repo = WorkspaceConfigRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = config_repo
        .create("inst-cfg", None, admin.0, "img:1", 1, 1024, 0, None, &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None)
        .await
        .unwrap();

    let inst = instance_repo
        .launch(config.id, admin.0, "inst-cfg", true, Some("/host/data"))
        .await
        .unwrap();

    assert_eq!(inst.config_id, config.id);
    assert_eq!(inst.owner_id, admin.0);
    assert_eq!(inst.status, "stopped");
    assert!(inst.container_id.is_none());
    assert!(inst.mount_persistent);
    assert_eq!(inst.resolved_volume_host_path, Some("/host/data".to_string()));
    assert!(!inst.vnc_token.is_empty());
    assert_eq!(inst.name, "inst-cfg-1");
    assert_eq!(inst.instance_number, 1);

    let found = instance_repo.find_by_id(inst.id).await.unwrap();
    assert!(found.is_some());
}

#[tokio::test]
async fn instance_launch_auto_increments_number() {
    let db = setup_db().await;
    let config_repo = WorkspaceConfigRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = config_repo
        .create("multi", None, admin.0, "img:1", 1, 1024, 0, None, &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None)
        .await
        .unwrap();

    let i1 = instance_repo.launch(config.id, admin.0, "multi", false, None).await.unwrap();
    let i2 = instance_repo.launch(config.id, admin.0, "multi", false, None).await.unwrap();
    let i3 = instance_repo.launch(config.id, admin.0, "multi", false, None).await.unwrap();

    assert_eq!(i1.instance_number, 1);
    assert_eq!(i2.instance_number, 2);
    assert_eq!(i3.instance_number, 3);
    assert_eq!(i1.name, "multi-1");
    assert_eq!(i2.name, "multi-2");
    assert_eq!(i3.name, "multi-3");
}

#[tokio::test]
async fn instance_find_by_vnc_token() {
    let db = setup_db().await;
    let config_repo = WorkspaceConfigRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = config_repo
        .create("vnc-cfg", None, admin.0, "img:1", 1, 1024, 0, None, &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None)
        .await
        .unwrap();

    let inst = instance_repo.launch(config.id, admin.0, "vnc-cfg", false, None).await.unwrap();

    let found = instance_repo.find_by_vnc_token(&inst.vnc_token).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, inst.id);

    let missing = instance_repo.find_by_vnc_token("nonexistent-token").await.unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn instance_list_by_owner() {
    let db = setup_db().await;
    let config_repo = WorkspaceConfigRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    user_repo.create("bob", "hash", "user").await.unwrap();
    let bob = user_repo.find_by_username("bob").await.unwrap().unwrap();

    let config = config_repo
        .create("list-cfg", None, admin.0, "img:1", 1, 1024, 0, None, &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None)
        .await
        .unwrap();

    instance_repo.launch(config.id, admin.0, "list-cfg", false, None).await.unwrap();
    instance_repo.launch(config.id, bob.0, "list-cfg", false, None).await.unwrap();

    let admin_instances = instance_repo.list_by_owner(admin.0).await.unwrap();
    assert_eq!(admin_instances.len(), 1);

    let bob_instances = instance_repo.list_by_owner(bob.0).await.unwrap();
    assert_eq!(bob_instances.len(), 1);
}

#[tokio::test]
async fn instance_list_all() {
    let db = setup_db().await;
    let config_repo = WorkspaceConfigRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = config_repo
        .create("all-cfg", None, admin.0, "img:1", 1, 1024, 0, None, &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None)
        .await
        .unwrap();

    instance_repo.launch(config.id, admin.0, "all-cfg", false, None).await.unwrap();
    instance_repo.launch(config.id, admin.0, "all-cfg", false, None).await.unwrap();

    let all = instance_repo.list_all().await.unwrap();
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn instance_update_status() {
    let db = setup_db().await;
    let config_repo = WorkspaceConfigRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = config_repo
        .create("status-cfg", None, admin.0, "img:1", 1, 1024, 0, None, &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None)
        .await
        .unwrap();

    let inst = instance_repo.launch(config.id, admin.0, "status-cfg", false, None).await.unwrap();
    assert_eq!(inst.status, "stopped");

    let updated = instance_repo.update_status(inst.id, "running").await.unwrap();
    assert!(updated);

    let found = instance_repo.find_by_id(inst.id).await.unwrap().unwrap();
    assert_eq!(found.status, "running");
}

#[tokio::test]
async fn instance_update_container_id() {
    let db = setup_db().await;
    let config_repo = WorkspaceConfigRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = config_repo
        .create("cid-cfg", None, admin.0, "img:1", 1, 1024, 0, None, &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None)
        .await
        .unwrap();

    let inst = instance_repo.launch(config.id, admin.0, "cid-cfg", false, None).await.unwrap();
    assert!(inst.container_id.is_none());

    instance_repo.update_container_id(inst.id, "abc123def456").await.unwrap();

    let found = instance_repo.find_by_id(inst.id).await.unwrap().unwrap();
    assert_eq!(found.container_id, Some("abc123def456".to_string()));
}

#[tokio::test]
async fn instance_delete() {
    let db = setup_db().await;
    let config_repo = WorkspaceConfigRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = config_repo
        .create("del-cfg", None, admin.0, "img:1", 1, 1024, 0, None, &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None)
        .await
        .unwrap();

    let inst = instance_repo.launch(config.id, admin.0, "del-cfg", false, None).await.unwrap();

    let deleted = instance_repo.delete(inst.id).await.unwrap();
    assert!(deleted);
    assert!(instance_repo.find_by_id(inst.id).await.unwrap().is_none());
}

#[tokio::test]
async fn instance_delete_nonexistent() {
    let db = setup_db().await;
    let instance_repo = WorkspaceInstanceRepository::new(&db);

    let deleted = instance_repo.delete(uuid::Uuid::new_v4()).await.unwrap();
    assert!(!deleted);
}

#[tokio::test]
async fn instance_update_status_nonexistent() {
    let db = setup_db().await;
    let instance_repo = WorkspaceInstanceRepository::new(&db);

    let updated = instance_repo.update_status(uuid::Uuid::new_v4(), "running").await.unwrap();
    assert!(!updated);
}

// ── RegistryRepository tests ──────────────────────────────────

#[tokio::test]
async fn registry_get_set_url() {
    let db = setup_db().await;
    let repo = RegistryRepository::new(&db);

    let url = repo.get_url().await.unwrap();
    assert!(url.is_none());

    repo.set_url("https://example.com/registry.json").await.unwrap();

    let url = repo.get_url().await.unwrap();
    assert_eq!(url, Some("https://example.com/registry.json".to_string()));
}

#[tokio::test]
async fn registry_set_url_upsert() {
    let db = setup_db().await;
    let repo = RegistryRepository::new(&db);

    repo.set_url("https://first.com").await.unwrap();
    repo.set_url("https://second.com").await.unwrap();

    let url = repo.get_url().await.unwrap();
    assert_eq!(url, Some("https://second.com".to_string()));
}

#[tokio::test]
async fn registry_get_set_cached() {
    let db = setup_db().await;
    let repo = RegistryRepository::new(&db);

    let cached = repo.get_cached().await.unwrap();
    assert!(cached.is_none());

    let data = serde_json::json!({
        "workspaces": [
            {"name": "ubuntu", "image": "ubuntu:22.04"},
            {"name": "debian", "image": "debian:12"}
        ]
    });
    repo.set_cached(&data).await.unwrap();

    let cached = repo.get_cached().await.unwrap();
    assert!(cached.is_some());
    let cached = cached.unwrap();
    assert_eq!(cached["workspaces"][0]["name"], "ubuntu");
    assert_eq!(cached["workspaces"][1]["name"], "debian");
}

#[tokio::test]
async fn registry_cached_upsert() {
    let db = setup_db().await;
    let repo = RegistryRepository::new(&db);

    repo.set_cached(&serde_json::json!({"v": 1})).await.unwrap();
    repo.set_cached(&serde_json::json!({"v": 2})).await.unwrap();

    let cached = repo.get_cached().await.unwrap().unwrap();
    assert_eq!(cached["v"], 2);
}

// ── Instance list_by_config ─────────────────────────────────

#[tokio::test]
async fn instance_list_by_config() {
    let db = setup_db().await;
    let config_repo = WorkspaceConfigRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let cfg1 = config_repo
        .create("lbc1", None, admin.0, "img:1", 1, 1024, 0, None, &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None)
        .await
        .unwrap();
    let cfg2 = config_repo
        .create("lbc2", None, admin.0, "img:2", 1, 1024, 0, None, &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None)
        .await
        .unwrap();

    instance_repo.launch(cfg1.id, admin.0, "lbc1", false, None).await.unwrap();
    instance_repo.launch(cfg1.id, admin.0, "lbc1", false, None).await.unwrap();
    instance_repo.launch(cfg2.id, admin.0, "lbc2", false, None).await.unwrap();

    let list1 = instance_repo.list_by_config(cfg1.id).await.unwrap();
    assert_eq!(list1.len(), 2);
    for inst in &list1 {
        assert_eq!(inst.config_id, cfg1.id);
    }

    let list2 = instance_repo.list_by_config(cfg2.id).await.unwrap();
    assert_eq!(list2.len(), 1);
    assert_eq!(list2[0].config_id, cfg2.id);
}

#[tokio::test]
async fn instance_list_by_config_empty() {
    let db = setup_db().await;
    let config_repo = WorkspaceConfigRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let cfg = config_repo
        .create("lbc-empty", None, admin.0, "img:1", 1, 1024, 0, None, &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None)
        .await
        .unwrap();

    let list = instance_repo.list_by_config(cfg.id).await.unwrap();
    assert!(list.is_empty());
}

#[tokio::test]
async fn instance_update_container_id_nonexistent() {
    let db = setup_db().await;
    let instance_repo = WorkspaceInstanceRepository::new(&db);

    let updated = instance_repo.update_container_id(uuid::Uuid::new_v4(), "abc123").await.unwrap();
    assert!(!updated);
}

// ── From impl tests ─────────────────────────────────────────

#[test]
fn config_model_from_converts_all_fields() {
    let id = uuid::Uuid::new_v4();
    let owner_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();

    let model = workspace_config::Model {
        id,
        name: "test-cfg".to_string(),
        description: Some("desc".to_string()),
        owner_id,
        image: "img:1".to_string(),
        cores: 4,
        memory: 8192,
        gpu_count: 2,
        docker_registry: Some("reg".to_string()),
        run_config: serde_json::json!({"key": "val"}),
        exec_config: serde_json::json!({"exec": true}),
        volume_mappings: serde_json::json!({"/host": "/container"}),
        persistent_storage_path: Some("/persist".to_string()),
        created_at: now,
        updated_at: now,
    };

    let config: WorkspaceConfig = model.into();
    assert_eq!(config.id, id);
    assert_eq!(config.name, "test-cfg");
    assert_eq!(config.description, Some("desc".to_string()));
    assert_eq!(config.owner_id, owner_id);
    assert_eq!(config.image, "img:1");
    assert_eq!(config.cores, 4);
    assert_eq!(config.memory, 8192);
    assert_eq!(config.gpu_count, 2);
    assert_eq!(config.docker_registry, Some("reg".to_string()));
    assert_eq!(config.run_config, serde_json::json!({"key": "val"}));
    assert_eq!(config.exec_config, serde_json::json!({"exec": true}));
    assert_eq!(config.volume_mappings, serde_json::json!({"/host": "/container"}));
    assert_eq!(config.persistent_storage_path, Some("/persist".to_string()));
}

#[test]
fn config_model_from_null_optionals() {
    let model = workspace_config::Model {
        id: uuid::Uuid::new_v4(),
        name: "minimal".to_string(),
        description: None,
        owner_id: uuid::Uuid::new_v4(),
        image: "img:1".to_string(),
        cores: 1,
        memory: 1024,
        gpu_count: 0,
        docker_registry: None,
        run_config: serde_json::json!({}),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_storage_path: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let config: WorkspaceConfig = model.into();
    assert!(config.description.is_none());
    assert!(config.docker_registry.is_none());
    assert!(config.persistent_storage_path.is_none());
}

#[test]
fn instance_model_from_converts_all_fields() {
    let id = uuid::Uuid::new_v4();
    let config_id = uuid::Uuid::new_v4();
    let owner_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();

    let model = workspace_instance::Model {
        id,
        config_id,
        name: "inst-1".to_string(),
        instance_number: 1,
        owner_id,
        container_id: Some("abc123".to_string()),
        status: "running".to_string(),
        vnc_token: "vtok".to_string(),
        vnc_password: "secretpw".to_string(),
        mount_persistent: true,
        resolved_volume_host_path: Some("/host/path".to_string()),
        created_at: now,
        updated_at: now,
    };

    let inst: WorkspaceInstance = model.into();
    assert_eq!(inst.id, id);
    assert_eq!(inst.config_id, config_id);
    assert_eq!(inst.name, "inst-1");
    assert_eq!(inst.instance_number, 1);
    assert_eq!(inst.owner_id, owner_id);
    assert_eq!(inst.container_id, Some("abc123".to_string()));
    assert_eq!(inst.status, "running");
    assert_eq!(inst.vnc_token, "vtok");
    assert!(inst.mount_persistent);
    assert_eq!(inst.resolved_volume_host_path, Some("/host/path".to_string()));
}

#[test]
fn instance_model_from_none_optionals() {
    let model = workspace_instance::Model {
        id: uuid::Uuid::new_v4(),
        config_id: uuid::Uuid::new_v4(),
        name: "inst-none".to_string(),
        instance_number: 1,
        owner_id: uuid::Uuid::new_v4(),
        container_id: None,
        status: "stopped".to_string(),
        vnc_token: "tok".to_string(),
        vnc_password: "pw".to_string(),
        mount_persistent: false,
        resolved_volume_host_path: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let inst: WorkspaceInstance = model.into();
    assert!(inst.container_id.is_none());
    assert!(inst.resolved_volume_host_path.is_none());
    assert!(!inst.mount_persistent);
}

#[tokio::test]
async fn instance_update_container_id_success() {
    let db = setup_db().await;
    let config_repo = WorkspaceConfigRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = config_repo
        .create("cid-success", None, admin.0, "img:1", 1, 1024, 0, None, &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None)
        .await
        .unwrap();

    let inst = instance_repo.launch(config.id, admin.0, "cid-success", false, None).await.unwrap();

    let updated = instance_repo.update_container_id(inst.id, "new_container_999").await.unwrap();
    assert!(updated);

    let found = instance_repo.find_by_id(inst.id).await.unwrap().unwrap();
    assert_eq!(found.container_id, Some("new_container_999".to_string()));
}

#[tokio::test]
async fn instance_update_status_success() {
    let db = setup_db().await;
    let config_repo = WorkspaceConfigRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = config_repo
        .create("status-success", None, admin.0, "img:1", 1, 1024, 0, None, &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None)
        .await
        .unwrap();

    let inst = instance_repo.launch(config.id, admin.0, "status-success", false, None).await.unwrap();
    assert_eq!(inst.status, "stopped");

    let updated = instance_repo.update_status(inst.id, "running").await.unwrap();
    assert!(updated);

    let found = instance_repo.find_by_id(inst.id).await.unwrap().unwrap();
    assert_eq!(found.status, "running");

    let updated = instance_repo.update_status(inst.id, "paused").await.unwrap();
    assert!(updated);

    let found = instance_repo.find_by_id(inst.id).await.unwrap().unwrap();
    assert_eq!(found.status, "paused");
}
