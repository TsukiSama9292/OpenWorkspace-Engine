mod common;

use openworkspace_api::db::*;
use openworkspace_api::effective_context::TemplateVisibility;
use migration::MigratorTrait;
use sea_orm::ConnectionTrait;
use sea_orm::DatabaseConnection;

async fn setup_db() -> DatabaseConnection {
    setup_db_with_steps(None).await
}

async fn setup_db_with_steps(steps: Option<u32>) -> DatabaseConnection {
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
    migration::Migrator::up(&migrator_db, steps)
        .await
        .expect("failed to run migrations");
    drop(migrator_db);

    sea_orm::Database::connect(&db_url)
        .await
        .expect("failed to connect")
}

#[tokio::test]
async fn migrations_round_trip() {
    let db = setup_db().await;

    migration::Migrator::down(&db, None)
        .await
        .expect("failed to roll back migrations");
    migration::Migrator::up(&db, None)
        .await
        .expect("failed to re-apply migrations");
}

// ── Flat-RBAC migration tests ────────────────────────────────

async fn query_scalar<T>(db: &DatabaseConnection, sql: &str) -> T
where
    T: sea_orm::TryGetable,
{
    let result = db
        .query_one(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Postgres,
            sql.to_string(),
        ))
        .await
        .expect("raw query failed")
        .expect("expected a result row");
    result
        .try_get("", "value")
        .expect("failed to read scalar value")
}

async fn query_scalar_nullable<T>(db: &DatabaseConnection, sql: &str) -> Option<T>
where
    T: sea_orm::TryGetable,
{
    let result = db
        .query_one(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Postgres,
            sql.to_string(),
        ))
        .await
        .expect("raw query failed")
        .expect("expected a result row");
    result
        .try_get_nullable("", "value")
        .expect("failed to read scalar value")
}

async fn group_membership_count(db: &DatabaseConnection, group_name: &str, user_id: uuid::Uuid) -> i64 {
    query_scalar(
        db,
        &format!(
            "SELECT count(*) AS value FROM user_groups ug JOIN groups g ON g.id = ug.group_id \
             WHERE g.name = '{}' AND ug.user_id = '{}'",
            group_name, user_id
        ),
    )
    .await
}

async fn group_kind(db: &DatabaseConnection, group_name: &str) -> Option<String> {
    query_scalar_nullable(
        db,
        &format!(
            "SELECT kind AS value FROM groups WHERE name = '{}'",
            group_name
        ),
    )
    .await
}

async fn group_flag(db: &DatabaseConnection, group_name: &str, flag: &str) -> bool {
    query_scalar(
        db,
        &format!("SELECT {} AS value FROM groups WHERE name = '{}'", flag, group_name),
    )
    .await
}

async fn group_max_instances(db: &DatabaseConnection, group_name: &str) -> Option<i32> {
    query_scalar_nullable(
        db,
        &format!(
            "SELECT max_instances AS value FROM groups WHERE name = '{}'",
            group_name
        ),
    )
    .await
}

async fn group_id(db: &DatabaseConnection, group_name: &str) -> uuid::Uuid {
    query_scalar(
        db,
        &format!("SELECT id AS value FROM groups WHERE name = '{}'", group_name),
    )
    .await
}

async fn user_direct_max_instances(db: &DatabaseConnection, user_id: uuid::Uuid) -> Option<i32> {
    query_scalar_nullable(
        db,
        &format!(
            "SELECT direct_max_instances AS value FROM users WHERE id = '{}'",
            user_id
        ),
    )
    .await
}

#[tokio::test]
async fn flat_rbac_migration_creates_tables_and_seeds_system_groups() {
    let db = setup_db().await;

    for table in &[
        "groups",
        "user_groups",
        "group_templates",
        "persistent_volumes",
    ] {
        let exists: bool = query_scalar(
            &db,
            &format!("SELECT to_regclass('{}') IS NOT NULL AS value", table),
        )
        .await;
        assert!(exists, "expected table {} to exist after migrations", table);
    }

    // The per-user whitelist is the contract drop of migration 000020.
    let personal_whitelist: bool = query_scalar(
        &db,
        "SELECT to_regclass('user_templates') IS NOT NULL AS value",
    )
    .await;
    assert!(!personal_whitelist, "user_templates must be dropped");

    // The admin boolean is the contract drop of migration 000020.
    let user_columns: i64 = query_scalar(
        &db,
        "SELECT count(*) AS value FROM information_schema.columns \
         WHERE table_name = 'users' AND column_name IN ('is_system_admin', 'direct_max_instances')",
    )
    .await;
    assert_eq!(user_columns, 1, "only direct_max_instances must remain");

    // The legacy Managers group is renamed to Manager.
    let managers: i64 =
        query_scalar(&db, "SELECT count(*) AS value FROM groups WHERE name = 'Managers'").await;
    assert_eq!(managers, 0, "Managers must be renamed to Manager");

    // Admin group: kind='admin', all five flags TRUE, unlimited (NULL) ceiling.
    let admin_count: i64 =
        query_scalar(&db, "SELECT count(*) AS value FROM groups WHERE name = 'Admin'").await;
    assert_eq!(admin_count, 1, "expected the Admin group to be seeded");
    assert_eq!(group_kind(&db, "Admin").await.as_deref(), Some("admin"));
    for flag in &[
        "can_create_template",
        "can_manage_users",
        "can_manage_group_instances",
        "can_manage_docker",
        "can_manage_registry",
    ] {
        assert!(group_flag(&db, "Admin", flag).await, "Admin {} = TRUE", flag);
    }
    assert_eq!(group_max_instances(&db, "Admin").await, None, "Admin is unlimited");

    // Manager group: kind='manager', all five flags TRUE, ceiling 2.
    let manager_count: i64 =
        query_scalar(&db, "SELECT count(*) AS value FROM groups WHERE name = 'Manager'").await;
    assert_eq!(manager_count, 1, "expected the Manager group to be seeded");
    assert_eq!(group_kind(&db, "Manager").await.as_deref(), Some("manager"));
    for flag in &[
        "can_create_template",
        "can_manage_users",
        "can_manage_group_instances",
        "can_manage_docker",
        "can_manage_registry",
    ] {
        assert!(group_flag(&db, "Manager", flag).await, "Manager {} = TRUE", flag);
    }
    assert_eq!(group_max_instances(&db, "Manager").await, Some(2));

    // User group: kind='user', all five flags FALSE, ceiling 1.
    let user_count: i64 =
        query_scalar(&db, "SELECT count(*) AS value FROM groups WHERE name = 'User'").await;
    assert_eq!(user_count, 1, "expected the User group to be seeded");
    assert_eq!(group_kind(&db, "User").await.as_deref(), Some("user"));
    for flag in &[
        "can_create_template",
        "can_manage_users",
        "can_manage_group_instances",
        "can_manage_docker",
        "can_manage_registry",
    ] {
        assert!(!group_flag(&db, "User", flag).await, "User {} = FALSE", flag);
    }
    assert_eq!(group_max_instances(&db, "User").await, Some(1));

    db.execute(sea_orm::Statement::from_string(
        sea_orm::DatabaseBackend::Postgres,
        "INSERT INTO groups (name) VALUES ('defaults')".to_string(),
    ))
    .await
    .unwrap();

    let default_flag: bool = query_scalar(
        &db,
        "SELECT can_create_template AS value FROM groups WHERE name = 'defaults'",
    )
    .await;
    assert!(!default_flag, "a fresh custom group should have all flags FALSE");
    assert_eq!(group_kind(&db, "defaults").await, None, "custom groups have no kind");
    let default_ceiling: i32 =
        query_scalar(&db, "SELECT max_instances AS value FROM groups WHERE name = 'defaults'")
            .await;
    assert_eq!(default_ceiling, 2);
}

#[tokio::test]
async fn flat_rbac_migration_moves_system_admins_and_backfills_admin_whitelist() {
    // Migrations 1..19 land the legacy flat-RBAC state: the `Managers` group,
    // `users.is_system_admin`, and the `user_templates` table all exist.
    let db = setup_db_with_steps(Some(19)).await;

    // Seed the pre-000020 rows a real post-000019 deployment would have.
    let admin_id = uuid::Uuid::new_v4();
    let mgr1_id = uuid::Uuid::new_v4();
    let mgr2_id = uuid::Uuid::new_v4();
    let plain_id = uuid::Uuid::new_v4();
    for (id, username) in [
        (admin_id, "admin1"),
        (mgr1_id, "mgr1"),
        (mgr2_id, "mgr2"),
        (plain_id, "plain"),
    ] {
        db.execute(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Postgres,
            format!(
                "INSERT INTO users (id, username, password_hash) VALUES ('{}', '{}', 'hash')",
                id, username
            ),
        ))
        .await
        .unwrap();
    }

    db.execute(sea_orm::Statement::from_string(
        sea_orm::DatabaseBackend::Postgres,
        format!(
            "UPDATE users SET is_system_admin = TRUE WHERE id = '{}'",
            admin_id
        ),
    ))
    .await
    .unwrap();

    let managers_id = group_id(&db, "Managers").await;
    for id in [mgr1_id, mgr2_id] {
        db.execute(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Postgres,
            format!(
                "INSERT INTO user_groups (user_id, group_id) VALUES ('{}', '{}')",
                id, managers_id
            ),
        ))
        .await
        .unwrap();
    }

    for (id, limit) in [(admin_id, 9), (mgr1_id, 5), (plain_id, 3)] {
        db.execute(sea_orm::Statement::from_string(
            sea_orm::DatabaseBackend::Postgres,
            format!(
                "UPDATE users SET direct_max_instances = {} WHERE id = '{}'",
                limit, id
            ),
        ))
        .await
        .unwrap();
    }

    // A template exists before the migration so the Admin backfill has
    // something to grant.
    let template_id = uuid::Uuid::new_v4();
    db.execute(sea_orm::Statement::from_string(
        sea_orm::DatabaseBackend::Postgres,
        format!(
            "INSERT INTO workspace_templates (id, name, owner_id) VALUES ('{}', 'pre', '{}')",
            template_id, admin_id
        ),
    ))
    .await
    .unwrap();

    migration::Migrator::up(&db, Some(1))
        .await
        .expect("failed to apply the system-groups migration");

    // Contract drop: personal whitelist table and admin boolean are gone.
    let personal_whitelist: bool = query_scalar(
        &db,
        "SELECT to_regclass('user_templates') IS NOT NULL AS value",
    )
    .await;
    assert!(!personal_whitelist, "user_templates must be dropped");
    let admin_columns: i64 = query_scalar(
        &db,
        "SELECT count(*) AS value FROM information_schema.columns \
         WHERE table_name = 'users' AND column_name = 'is_system_admin'",
    )
    .await;
    assert_eq!(admin_columns, 0, "is_system_admin must be dropped");

    // Former system admins move into the Admin group and out of Manager.
    assert_eq!(group_membership_count(&db, "Admin", admin_id).await, 1);
    assert_eq!(group_membership_count(&db, "Manager", admin_id).await, 0);

    // Managers stay in the renamed Manager group; nobody else lands in it.
    assert_eq!(group_membership_count(&db, "Manager", mgr1_id).await, 1);
    assert_eq!(group_membership_count(&db, "Manager", mgr2_id).await, 1);
    assert_eq!(group_membership_count(&db, "Admin", mgr1_id).await, 0);
    assert_eq!(group_membership_count(&db, "Admin", plain_id).await, 0);

    // The Admin group's whitelist is backfilled onto every existing template.
    let backfilled: i64 = query_scalar(
        &db,
        &format!(
            "SELECT count(*) AS value FROM group_templates gt \
             JOIN groups g ON g.id = gt.group_id \
             WHERE g.kind = 'admin' AND gt.template_id = '{}'",
            template_id
        ),
    )
    .await;
    assert_eq!(backfilled, 1, "Admin must be whitelisted on pre-existing templates");

    // Personal ceilings survive the expand→contract.
    assert_eq!(user_direct_max_instances(&db, admin_id).await, Some(9));
    assert_eq!(user_direct_max_instances(&db, mgr1_id).await, Some(5));
    assert_eq!(user_direct_max_instances(&db, plain_id).await, Some(3));

    // The seeded system groups carry the spec'd kinds and ceilings.
    assert_eq!(group_kind(&db, "Admin").await.as_deref(), Some("admin"));
    assert_eq!(group_kind(&db, "Manager").await.as_deref(), Some("manager"));
    assert_eq!(group_kind(&db, "User").await.as_deref(), Some("user"));
    assert_eq!(group_max_instances(&db, "Admin").await, None);
    assert_eq!(group_max_instances(&db, "Manager").await, Some(2));
    assert_eq!(group_max_instances(&db, "User").await, Some(1));
}

#[tokio::test]
async fn flat_rbac_migration_tolerates_custom_groups_with_system_names() {
    // Spec Decision 9 keeps `Admin`/`Manager`/`User` legal as custom-group
    // names; migration 000020 must not crash on them (Standards finding: the
    // seeding previously collided on the unique name constraint).
    let db = setup_db_with_steps(Some(19)).await;

    // A pre-000020 deployment with custom groups named exactly like the
    // system groups that 000020 is about to seed.
    db.execute(sea_orm::Statement::from_string(
        sea_orm::DatabaseBackend::Postgres,
        "INSERT INTO groups (name) VALUES ('Admin'), ('User')".to_string(),
    ))
    .await
    .unwrap();

    migration::Migrator::up(&db, Some(1))
        .await
        .expect("migration 000020 must tolerate custom groups holding system names");

    // The system groups are seeded with the canonical names and kinds.
    assert_eq!(group_kind(&db, "Admin").await.as_deref(), Some("admin"));
    assert_eq!(group_kind(&db, "User").await.as_deref(), Some("user"));
    assert_eq!(group_kind(&db, "Manager").await.as_deref(), Some("manager"));

    // The colliding custom groups survive, renamed out of the way.
    let renamed_admin: i64 = query_scalar(
        &db,
        "SELECT count(*) AS value FROM groups WHERE kind IS NULL AND name LIKE 'Admin (custom %'",
    )
    .await;
    assert_eq!(renamed_admin, 1, "custom Admin group must survive renamed");
    let renamed_user: i64 = query_scalar(
        &db,
        "SELECT count(*) AS value FROM groups WHERE kind IS NULL AND name LIKE 'User (custom %'",
    )
    .await;
    assert_eq!(renamed_user, 1, "custom User group must survive renamed");
}

// ── UserRepository tests ──────────────────────────────────────

#[tokio::test]
async fn user_seed_admin_creates_admin() {
    let db = setup_db().await;
    let repo = UserRepository::new(&db);

    repo.seed_admin("testpass").await.unwrap();

    let user = repo.find_by_username("admin").await.unwrap();
    assert!(user.is_some());
    let u = user.unwrap();
    assert_eq!(u.username, "admin");
    assert!(!u.id.is_nil());
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

    let id = repo.create("alice", "hash123").await.unwrap();

    let found = repo.find_by_id(id).await.unwrap();
    assert!(found.is_some());
    let u = found.unwrap();
    assert_eq!(u.id, id);
    assert_eq!(u.username, "alice");
    assert_eq!(u.password_hash, "hash123");
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

    repo.create("user1", "h1").await.unwrap();
    repo.create("user2", "h2").await.unwrap();

    let users = repo.list_all().await.unwrap();
    assert_eq!(users.len(), 2);
    let names: Vec<&str> = users.iter().map(|u| u.username.as_str()).collect();
    assert!(names.contains(&"user1"));
    assert!(names.contains(&"user2"));
}

#[tokio::test]
async fn user_delete() {
    let db = setup_db().await;
    let repo = UserRepository::new(&db);

    let id = repo.create("deleteme", "h").await.unwrap();
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

// ── WorkspaceTemplateRepository tests ───────────────────────────

#[tokio::test]
async fn config_create_and_find() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create(
            "my-config",
            Some("A test config"),
            admin.id,
            "ubuntu:22.04",
            4,
            8_589_934_592,
            1,
            None,
            "kasmvnc",
            "docker",
            &serde_json::json!({"cmd": ["sleep", "infinity"]}),
            &serde_json::json!({}),
            &serde_json::json!({"bind": "/data"}),
            Some("/host/data"),
            Some(3600),
            "stop",
            0,
            0,
            None,
            "pause", false)
        .await
        .unwrap();

    assert_eq!(config.name, "my-config");
    assert_eq!(config.image, "ubuntu:22.04");
    assert_eq!(config.cores, 4);
    assert_eq!(config.memory, 8_589_934_592);
    assert_eq!(config.gpu_count, 1);
    assert_eq!(config.description, Some("A test config".to_string()));
    assert_eq!(config.persistent_storage_path, Some("/host/data".to_string()));
    assert_eq!(config.max_run_seconds, Some(3600));
    assert_eq!(config.timeout_action, "stop");

    let found = template_repo.find_by_id(config.id).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "my-config");
}

#[tokio::test]
async fn config_list_by_owner() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    template_repo
        .create("cfg1", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();
    template_repo
        .create("cfg2", None, admin.id, "img:2", 2, 2048, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();

    let configs = template_repo.list_by_owner(admin.id).await.unwrap();
    assert_eq!(configs.len(), 2);
}

#[tokio::test]
async fn config_list_all() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    template_repo
        .create("cfg1", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();

    let configs = template_repo.list_all().await.unwrap();
    assert_eq!(configs.len(), 1);
}

#[tokio::test]
async fn config_update() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("old-name", None, admin.id, "old:img", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();

    let updated = template_repo
        .update(
            config.id,
            "new-name",
            Some("updated desc"),
            "new:img",
            8,
            16_777_216,
            2,
            Some("my-registry"),
            "kasmvnc",
            "docker",
            &serde_json::json!({"key": "val"}),
            &serde_json::json!({"exec": true}),
            &serde_json::json!({"vol": "/mnt"}),
            Some("/new/path"),
            Some(7200),
            "pause",
            0,
            0,
            None,
            "pause", false)
        .await
        .unwrap();
    assert!(updated);

    let found = template_repo.find_by_id(config.id).await.unwrap().unwrap();
    assert_eq!(found.name, "new-name");
    assert_eq!(found.image, "new:img");
    assert_eq!(found.cores, 8);
    assert_eq!(found.gpu_count, 2);
    assert_eq!(found.docker_registry, Some("my-registry".to_string()));
    assert_eq!(found.description, Some("updated desc".to_string()));
    assert_eq!(found.container_runtime, "docker");
    assert_eq!(found.persistent_storage_path, Some("/new/path".to_string()));
    assert_eq!(found.max_run_seconds, Some(7200));
    assert_eq!(found.timeout_action, "pause");
}

#[tokio::test]
async fn template_create_defaults_to_private() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("cfg", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();

    assert_eq!(config.visibility, TemplateVisibility::Private);
}

#[tokio::test]
async fn template_set_visibility_persists() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("cfg", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();

    assert!(template_repo
        .set_visibility(config.id, TemplateVisibility::Public)
        .await
        .unwrap());
    let found = template_repo.find_by_id(config.id).await.unwrap().unwrap();
    assert_eq!(found.visibility, TemplateVisibility::Public);

    assert!(template_repo
        .set_visibility(config.id, TemplateVisibility::Hidden)
        .await
        .unwrap());
    let found = template_repo.find_by_id(config.id).await.unwrap().unwrap();
    assert_eq!(found.visibility, TemplateVisibility::Hidden);

    assert!(template_repo
        .set_visibility(config.id, TemplateVisibility::Private)
        .await
        .unwrap());
    let found = template_repo.find_by_id(config.id).await.unwrap().unwrap();
    assert_eq!(found.visibility, TemplateVisibility::Private);
}

#[tokio::test]
async fn template_update_preserves_visibility() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("cfg", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();
    assert!(template_repo
        .set_visibility(config.id, TemplateVisibility::Public)
        .await
        .unwrap());

    let updated = template_repo
        .update(
            config.id,
            "new-name",
            None,
            "new:img",
            2,
            1024,
            0,
            None,
            "kasmvnc",
            "docker",
            &serde_json::json!({}),
            &serde_json::json!({}),
            &serde_json::json!({}),
            None,
            None,
            "remove",
            0,
            0,
            None,
            "pause",
            false,
        )
        .await
        .unwrap();
    assert!(updated);

    let found = template_repo.find_by_id(config.id).await.unwrap().unwrap();
    assert_eq!(found.name, "new-name");
    assert_eq!(found.visibility, TemplateVisibility::Public);
}

#[tokio::test]
async fn template_visibility_migration_defaults_existing_rows_to_private() {
    // A template created before the visibility column existed must land at
    // `private` once migration 21 runs — the upgrade changes no authorization.
    let db = setup_db_with_steps(Some(20)).await;
    let user_repo = UserRepository::new(&db);
    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let id = uuid::Uuid::new_v4();
    db.execute(sea_orm::Statement::from_string(
        sea_orm::DatabaseBackend::Postgres,
        format!(
            "INSERT INTO workspace_templates (id, name, owner_id) VALUES ('{}', 'legacy', '{}')",
            id, admin.id
        ),
    ))
    .await
    .expect("failed to insert pre-visibility template");

    migration::Migrator::up(&db, Some(21))
        .await
        .expect("failed to apply visibility migration");

    let visibility: String =
        query_scalar(&db, &format!("SELECT visibility AS value FROM workspace_templates WHERE id = '{}'", id))
            .await;
    assert_eq!(visibility, "private");
}

#[tokio::test]
async fn config_delete() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("del", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();

    let deleted = template_repo.delete(config.id).await.unwrap();
    assert!(deleted);
    assert!(template_repo.find_by_id(config.id).await.unwrap().is_none());
}

#[tokio::test]
async fn config_delete_nonexistent() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);

    let deleted = template_repo.delete(uuid::Uuid::new_v4()).await.unwrap();
    assert!(!deleted);
}

#[tokio::test]
async fn config_count_instances() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("counted", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();

    assert_eq!(template_repo.count_instances(config.id).await.unwrap(), 0);

    instance_repo.launch(config.id, admin.id, "counted", false, None).await.unwrap();
    instance_repo.launch(config.id, admin.id, "counted", false, None).await.unwrap();

    assert_eq!(template_repo.count_instances(config.id).await.unwrap(), 2);
}

#[tokio::test]
async fn config_create_with_container_runtime() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("runsc-cfg", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "runsc", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();
    assert_eq!(config.container_runtime, "runsc");

    let found = template_repo.find_by_id(config.id).await.unwrap().unwrap();
    assert_eq!(found.container_runtime, "runsc");
}

#[tokio::test]
async fn config_update_container_runtime() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("runtime-up", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();
    assert_eq!(config.container_runtime, "docker");

    let updated = template_repo
        .update(config.id, "runtime-up", None, "img:1", 1, 1024, 0, None, "kasmvnc", "runsc", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();
    assert!(updated);

    let found = template_repo.find_by_id(config.id).await.unwrap().unwrap();
    assert_eq!(found.container_runtime, "runsc");
}

#[tokio::test]
async fn config_default_docker_in_instance_false() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("dini-default", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();
    assert!(!config.docker_in_instance);

    let found = template_repo.find_by_id(config.id).await.unwrap().unwrap();
    assert!(!found.docker_in_instance);
}

#[tokio::test]
async fn config_create_with_docker_in_instance() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("dini-true", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "runsc", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", true)
        .await
        .unwrap();
    assert!(config.docker_in_instance);

    let found = template_repo.find_by_id(config.id).await.unwrap().unwrap();
    assert!(found.docker_in_instance);
}

#[tokio::test]
async fn config_update_docker_in_instance() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("dini-up", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();
    assert!(!config.docker_in_instance);

    let updated = template_repo
        .update(config.id, "dini-up", None, "img:1", 1, 1024, 0, None, "kasmvnc", "runsc", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", true)
        .await
        .unwrap();
    assert!(updated);

    let found = template_repo.find_by_id(config.id).await.unwrap().unwrap();
    assert!(found.docker_in_instance);
}

#[tokio::test]
async fn config_create_and_update_network_bandwidth() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("bw", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 100, 50, None, "pause", false)
        .await
        .unwrap();
    assert_eq!(config.network_bandwidth_up_mbps, 100);
    assert_eq!(config.network_bandwidth_down_mbps, 50);

    let found = template_repo.find_by_id(config.id).await.unwrap().unwrap();
    assert_eq!(found.network_bandwidth_up_mbps, 100);
    assert_eq!(found.network_bandwidth_down_mbps, 50);

    let updated = template_repo
        .update(config.id, "bw", None, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();
    assert!(updated);

    let found = template_repo.find_by_id(config.id).await.unwrap().unwrap();
    assert_eq!(found.network_bandwidth_up_mbps, 0);
    assert_eq!(found.network_bandwidth_down_mbps, 0);
}

// ── WorkspaceInstanceRepository tests ─────────────────────────

#[tokio::test]
async fn instance_launch_and_find() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("inst-cfg", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();

    let inst = instance_repo
        .launch(config.id, admin.id, "inst-cfg", true, Some("/host/data"))
        .await
        .unwrap();

    assert_eq!(inst.template_id, config.id);
    assert_eq!(inst.owner_id, admin.id);
    assert_eq!(inst.status, "stopped");
    assert!(inst.container_id.is_none());
    assert!(inst.mount_persistent);
    assert_eq!(inst.resolved_volume_host_path, Some("/host/data".to_string()));
    assert!(!inst.access_token.is_empty());
    assert_eq!(inst.name, "inst-cfg-1");
    assert_eq!(inst.instance_number, 1);

    let found = instance_repo.find_by_id(inst.id).await.unwrap();
    assert!(found.is_some());
}

#[tokio::test]
async fn instance_launch_auto_increments_number() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("multi", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();

    let i1 = instance_repo.launch(config.id, admin.id, "multi", false, None).await.unwrap();
    let i2 = instance_repo.launch(config.id, admin.id, "multi", false, None).await.unwrap();
    let i3 = instance_repo.launch(config.id, admin.id, "multi", false, None).await.unwrap();

    assert_eq!(i1.instance_number, 1);
    assert_eq!(i2.instance_number, 2);
    assert_eq!(i3.instance_number, 3);
    assert_eq!(i1.name, "multi-1");
    assert_eq!(i2.name, "multi-2");
    assert_eq!(i3.name, "multi-3");
}

#[tokio::test]
async fn instance_find_by_access_token() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("vnc-cfg", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();

    let inst = instance_repo.launch(config.id, admin.id, "vnc-cfg", false, None).await.unwrap();

    let found = instance_repo.find_by_access_token(&inst.access_token).await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, inst.id);

    let missing = instance_repo.find_by_access_token("nonexistent-token").await.unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn instance_list_by_owner() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    user_repo.create("bob", "hash").await.unwrap();
    let bob = user_repo.find_by_username("bob").await.unwrap().unwrap();

    let config = template_repo
        .create("list-cfg", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();

    instance_repo.launch(config.id, admin.id, "list-cfg", false, None).await.unwrap();
    instance_repo.launch(config.id, bob.id, "list-cfg", false, None).await.unwrap();

    let admin_instances = instance_repo.list_by_owner(admin.id).await.unwrap();
    assert_eq!(admin_instances.len(), 1);

    let bob_instances = instance_repo.list_by_owner(bob.id).await.unwrap();
    assert_eq!(bob_instances.len(), 1);
}

#[tokio::test]
async fn instance_list_all() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("all-cfg", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();

    instance_repo.launch(config.id, admin.id, "all-cfg", false, None).await.unwrap();
    instance_repo.launch(config.id, admin.id, "all-cfg", false, None).await.unwrap();

    let all = instance_repo.list_all().await.unwrap();
    assert_eq!(all.len(), 2);
}

#[tokio::test]
async fn instance_update_status() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("status-cfg", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();

    let inst = instance_repo.launch(config.id, admin.id, "status-cfg", false, None).await.unwrap();
    assert_eq!(inst.status, "stopped");

    let updated = instance_repo.update_status(inst.id, "running").await.unwrap();
    assert!(updated);

    let found = instance_repo.find_by_id(inst.id).await.unwrap().unwrap();
    assert_eq!(found.status, "running");
}

#[tokio::test]
async fn instance_update_container_id() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("cid-cfg", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();

    let inst = instance_repo.launch(config.id, admin.id, "cid-cfg", false, None).await.unwrap();
    assert!(inst.container_id.is_none());

    instance_repo.update_container_id(inst.id, "abc123def456").await.unwrap();

    let found = instance_repo.find_by_id(inst.id).await.unwrap().unwrap();
    assert_eq!(found.container_id, Some("abc123def456".to_string()));
}

#[tokio::test]
async fn instance_delete() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("del-cfg", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();

    let inst = instance_repo.launch(config.id, admin.id, "del-cfg", false, None).await.unwrap();

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

// ── Instance list_by_template ─────────────────────────────────

#[tokio::test]
async fn instance_list_by_template() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let cfg1 = template_repo
        .create("lbc1", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();
    let cfg2 = template_repo
        .create("lbc2", None, admin.id, "img:2", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();

    instance_repo.launch(cfg1.id, admin.id, "lbc1", false, None).await.unwrap();
    instance_repo.launch(cfg1.id, admin.id, "lbc1", false, None).await.unwrap();
    instance_repo.launch(cfg2.id, admin.id, "lbc2", false, None).await.unwrap();

    let list1 = instance_repo.list_by_template(cfg1.id).await.unwrap();
    assert_eq!(list1.len(), 2);
    for inst in &list1 {
        assert_eq!(inst.template_id, cfg1.id);
    }

    let list2 = instance_repo.list_by_template(cfg2.id).await.unwrap();
    assert_eq!(list2.len(), 1);
    assert_eq!(list2[0].template_id, cfg2.id);
}

#[tokio::test]
async fn instance_list_by_template_empty() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let cfg = template_repo
        .create("lbc-empty", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();

    let list = instance_repo.list_by_template(cfg.id).await.unwrap();
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

    let model = workspace_template::Model {
        id,
        name: "test-cfg".to_string(),
        description: Some("desc".to_string()),
        owner_id,
        image: "img:1".to_string(),
        cores: 4,
        memory: 8192,
        gpu_count: 2,
        docker_registry: Some("reg".to_string()),
        remote_type: "kasmvnc".to_string(),
        container_runtime: "docker".to_string(),
        run_config: serde_json::json!({"key": "val"}),
        exec_config: serde_json::json!({"exec": true}),
        volume_mappings: serde_json::json!({"/host": "/container"}),
        persistent_storage_path: Some("/persist".to_string()),
        max_run_seconds: Some(5400),
        timeout_action: "pause".to_string(),
        network_bandwidth_up_mbps: 100,
        network_bandwidth_down_mbps: 50,
        keep_time_seconds: Some(1800),
        keep_time_action: "pause".to_string(),
        docker_in_instance: false,
        visibility: "public".to_string(),
        created_at: now,
        updated_at: now,
    };

    let config: WorkspaceTemplate = model.into();
    assert_eq!(config.id, id);
    assert_eq!(config.name, "test-cfg");
    assert_eq!(config.description, Some("desc".to_string()));
    assert_eq!(config.owner_id, owner_id);
    assert_eq!(config.image, "img:1");
    assert_eq!(config.cores, 4);
    assert_eq!(config.memory, 8192);
    assert_eq!(config.gpu_count, 2);
    assert_eq!(config.docker_registry, Some("reg".to_string()));
    assert_eq!(config.remote_type, "kasmvnc");
    assert_eq!(config.container_runtime, "docker");
    assert_eq!(config.run_config, serde_json::json!({"key": "val"}));
    assert_eq!(config.exec_config, serde_json::json!({"exec": true}));
    assert_eq!(config.volume_mappings, serde_json::json!({"/host": "/container"}));
    assert_eq!(config.persistent_storage_path, Some("/persist".to_string()));
    assert_eq!(config.max_run_seconds, Some(5400));
    assert_eq!(config.timeout_action, "pause");
    assert_eq!(config.network_bandwidth_up_mbps, 100);
    assert_eq!(config.network_bandwidth_down_mbps, 50);
    assert_eq!(config.keep_time_seconds, Some(1800));
    assert_eq!(config.keep_time_action, "pause");
    assert_eq!(config.visibility, TemplateVisibility::Public);
}

#[test]
fn config_model_from_null_optionals() {
    let model = workspace_template::Model {
        id: uuid::Uuid::new_v4(),
        name: "minimal".to_string(),
        description: None,
        owner_id: uuid::Uuid::new_v4(),
        image: "img:1".to_string(),
        cores: 1,
        memory: 1024,
        gpu_count: 0,
        docker_registry: None,
        remote_type: "kasmvnc".to_string(),
        container_runtime: "docker".to_string(),
        run_config: serde_json::json!({}),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_storage_path: None,
        max_run_seconds: None,
        timeout_action: "remove".to_string(),
        network_bandwidth_up_mbps: 0,
        network_bandwidth_down_mbps: 0,
        keep_time_seconds: None,
        keep_time_action: "remove".to_string(),
        docker_in_instance: false,
        visibility: "private".to_string(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let config: WorkspaceTemplate = model.into();
    assert!(config.description.is_none());
    assert!(config.docker_registry.is_none());
    assert!(config.persistent_storage_path.is_none());
    assert!(config.max_run_seconds.is_none());
    assert_eq!(config.timeout_action, "remove");
    assert!(config.keep_time_seconds.is_none());
    assert_eq!(config.keep_time_action, "remove");
    assert_eq!(config.visibility, TemplateVisibility::Private);
}

#[test]
fn config_model_from_container_runtime_runsc() {
    let model = workspace_template::Model {
        id: uuid::Uuid::new_v4(),
        name: "runsc-test".to_string(),
        description: None,
        owner_id: uuid::Uuid::new_v4(),
        image: "img:1".to_string(),
        cores: 1,
        memory: 1024,
        gpu_count: 0,
        docker_registry: None,
        remote_type: "kasmvnc".to_string(),
        container_runtime: "runsc".to_string(),
        run_config: serde_json::json!({}),
        exec_config: serde_json::json!({}),
        volume_mappings: serde_json::json!({}),
        persistent_storage_path: None,
        max_run_seconds: Some(120),
        timeout_action: "stop".to_string(),
        network_bandwidth_up_mbps: 0,
        network_bandwidth_down_mbps: 0,
        keep_time_seconds: None,
        keep_time_action: "remove".to_string(),
        docker_in_instance: false,
        visibility: "hidden".to_string(),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let config: WorkspaceTemplate = model.into();
    assert_eq!(config.container_runtime, "runsc");
    assert_eq!(config.max_run_seconds, Some(120));
    assert_eq!(config.timeout_action, "stop");
    assert!(config.keep_time_seconds.is_none());
    assert_eq!(config.keep_time_action, "remove");
    assert_eq!(config.visibility, TemplateVisibility::Hidden);
}

#[test]
fn instance_model_from_converts_all_fields() {
    let id = uuid::Uuid::new_v4();
    let template_id = uuid::Uuid::new_v4();
    let owner_id = uuid::Uuid::new_v4();
    let now = chrono::Utc::now();

    let model = workspace_instance::Model {
        id,
        template_id,
        name: "inst-1".to_string(),
        instance_number: 1,
        owner_id,
        container_id: Some("abc123".to_string()),
        host_port: Some(10000),
        status: "running".to_string(),
        access_token: "vtok".to_string(),
        access_password: "secretpw".to_string(),
        mount_persistent: true,
        resolved_volume_host_path: Some("/host/path".to_string()),
        started_at: Some(now),
        last_seen_at: Some(now),
        created_at: now,
        updated_at: now,
    };

    let inst: WorkspaceInstance = model.into();
    assert_eq!(inst.id, id);
    assert_eq!(inst.template_id, template_id);
    assert_eq!(inst.name, "inst-1");
    assert_eq!(inst.instance_number, 1);
    assert_eq!(inst.owner_id, owner_id);
    assert_eq!(inst.container_id, Some("abc123".to_string()));
    assert_eq!(inst.status, "running");
    assert_eq!(inst.access_token, "vtok");
    assert!(inst.mount_persistent);
    assert_eq!(inst.resolved_volume_host_path, Some("/host/path".to_string()));
    assert_eq!(inst.started_at, Some(now));
    assert_eq!(inst.last_seen_at, Some(now));
}

#[test]
fn instance_model_from_none_optionals() {
    let model = workspace_instance::Model {
        id: uuid::Uuid::new_v4(),
        template_id: uuid::Uuid::new_v4(),
        name: "inst-none".to_string(),
        instance_number: 1,
        owner_id: uuid::Uuid::new_v4(),
        container_id: None,
        host_port: None,
        status: "stopped".to_string(),
        access_token: "tok".to_string(),
        access_password: "pw".to_string(),
        mount_persistent: false,
        resolved_volume_host_path: None,
        started_at: None,
        last_seen_at: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };

    let inst: WorkspaceInstance = model.into();
    assert!(inst.container_id.is_none());
    assert!(inst.resolved_volume_host_path.is_none());
    assert!(inst.started_at.is_none());
    assert!(inst.last_seen_at.is_none());
    assert!(!inst.mount_persistent);
}

#[tokio::test]
async fn instance_update_container_id_success() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("cid-success", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();

    let inst = instance_repo.launch(config.id, admin.id, "cid-success", false, None).await.unwrap();

    let updated = instance_repo.update_container_id(inst.id, "new_container_999").await.unwrap();
    assert!(updated);

    let found = instance_repo.find_by_id(inst.id).await.unwrap().unwrap();
    assert_eq!(found.container_id, Some("new_container_999".to_string()));
}

#[tokio::test]
async fn instance_update_status_success() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("status-success", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();

    let inst = instance_repo.launch(config.id, admin.id, "status-success", false, None).await.unwrap();
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

#[tokio::test]
async fn instance_update_started_at_success() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("started-at-cfg", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();

    let inst = instance_repo.launch(config.id, admin.id, "started-at-cfg", false, None).await.unwrap();
    assert!(inst.started_at.is_none());

    let now = chrono::Utc::now();
    let updated = instance_repo.update_started_at(inst.id, Some(now)).await.unwrap();
    assert!(updated);

    let found = instance_repo.find_by_id(inst.id).await.unwrap().unwrap();
    assert!(found.started_at.is_some());
    assert!((found.started_at.unwrap() - now).num_microseconds().unwrap_or(0).abs() < 1000);

    let updated = instance_repo.update_started_at(inst.id, None).await.unwrap();
    assert!(updated);

    let found = instance_repo.find_by_id(inst.id).await.unwrap().unwrap();
    assert!(found.started_at.is_none());
}

#[tokio::test]
async fn instance_list_running_with_started_at() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("lrsa-cfg", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();

    let a = instance_repo.launch(config.id, admin.id, "lrsa", false, None).await.unwrap();
    let b = instance_repo.launch(config.id, admin.id, "lrsa", false, None).await.unwrap();
    let c = instance_repo.launch(config.id, admin.id, "lrsa", false, None).await.unwrap();
    let d = instance_repo.launch(config.id, admin.id, "lrsa", false, None).await.unwrap();

    instance_repo.update_status(a.id, "running").await.unwrap();
    instance_repo.update_status(b.id, "running").await.unwrap();
    instance_repo.update_status(c.id, "running").await.unwrap();
    instance_repo.update_status(d.id, "stopped").await.unwrap();

    let now = chrono::Utc::now();
    instance_repo.update_started_at(a.id, Some(now)).await.unwrap();
    instance_repo.update_started_at(b.id, Some(now)).await.unwrap();
    instance_repo.update_started_at(d.id, Some(now)).await.unwrap();

    let running = instance_repo.list_running_with_started_at().await.unwrap();
    let mut ids: Vec<uuid::Uuid> = running.iter().map(|i| i.id).collect();
    ids.sort();

    let mut expected = vec![a.id, b.id];
    expected.sort();
    assert_eq!(ids, expected);
}

#[tokio::test]
async fn instance_host_port_commit_list_and_clear() {
    let db = setup_db().await;
    let template_repo = WorkspaceTemplateRepository::new(&db);
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    let user_repo = UserRepository::new(&db);

    user_repo.seed_admin("pass").await.unwrap();
    let admin = user_repo.find_by_username("admin").await.unwrap().unwrap();

    let config = template_repo
        .create("hp-cfg", None, admin.id, "img:1", 1, 1024, 0, None, "kasmvnc", "docker", &serde_json::json!({}), &serde_json::json!({}), &serde_json::json!({}), None, None, "remove", 0, 0, None, "pause", false)
        .await
        .unwrap();

    let a = instance_repo.launch(config.id, admin.id, "hp", false, None).await.unwrap();
    let b = instance_repo.launch(config.id, admin.id, "hp", false, None).await.unwrap();

    assert!(instance_repo.list_host_ports().await.unwrap().is_empty());

    assert!(instance_repo.update_host_port(a.id, Some(10000)).await.unwrap());
    let found = instance_repo.find_by_id(a.id).await.unwrap().unwrap();
    assert_eq!(found.host_port, Some(10000));
    assert_eq!(instance_repo.list_host_ports().await.unwrap(), vec![10000]);

    // UNIQUE index on host_port is the concurrency arbiter: a second instance
    // cannot claim the same port.
    assert!(instance_repo.update_host_port(b.id, Some(10000)).await.is_err());
    assert_eq!(instance_repo.list_host_ports().await.unwrap(), vec![10000]);

    assert!(instance_repo.update_host_port(a.id, None).await.unwrap());
    assert!(instance_repo.list_host_ports().await.unwrap().is_empty());

    assert!(instance_repo.update_host_port(b.id, Some(10001)).await.unwrap());
    assert_eq!(instance_repo.list_host_ports().await.unwrap(), vec![10001]);
}

