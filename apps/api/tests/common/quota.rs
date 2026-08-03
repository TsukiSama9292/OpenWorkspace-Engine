//! Shared DB-only harness for the quota integration suites: a throwaway
//! migrated database (`TestDb`) plus the row-insertion helpers both the
//! quota-activation and lifecycle-accounting binaries use.
//!
//! This module is deliberately NOT declared from `common/mod.rs`. It is pulled
//! in only by the two binaries that use it via
//! `#[path = "common/quota.rs"] mod quota;`, so it is not compiled into the
//! other integration binaries where its helpers would be dead code. Every item
//! here is exercised by both consuming binaries.

use std::sync::atomic::{AtomicU32, Ordering};

use migration::{Migrator, MigratorTrait};
use openworkspace_api::auth::Role;
use openworkspace_api::db::{user, WorkspaceInstanceRepository, WorkspaceTemplateRepository};
use openworkspace_api::quota::{AllocationMode, QuotaOverride};
use openworkspace_api::quota_activation::{ActivationKind, ActivationRequest, LaunchPayload};
use sea_orm::{Database, EntityTrait, Set};
use serde_json::json;
use uuid::Uuid;

pub const GIB: i64 = 1024 * 1024 * 1024;

static DB_COUNTER: AtomicU32 = AtomicU32::new(0);

/// A throwaway database with the schema migrated, dropped on `Drop`. This is
/// the DB-only slice of `TestContext` (no HTTP server, no Docker client).
pub struct TestDb {
    pub db: sea_orm::DatabaseConnection,
    db_name: String,
}

impl TestDb {
    pub async fn new() -> Self {
        crate::common::ensure_pg().await;

        let counter = DB_COUNTER.fetch_add(1, Ordering::Relaxed);
        let db_name = format!("quota_harness_{}_{:04}", std::process::id(), counter);
        let base_url = crate::common::pg_base_url();

        let (cleanup_client, cleanup_conn) =
            tokio_postgres::connect(&base_url, tokio_postgres::NoTls)
                .await
                .expect("failed to connect to postgres");
        tokio::spawn(async move {
            let _ = cleanup_conn.await;
        });
        let _ = cleanup_client
            .execute(
                &format!(
                    "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{}'",
                    db_name
                )[..],
                &[],
            )
            .await;
        let _ = cleanup_client
            .execute(&format!("DROP DATABASE IF EXISTS \"{}\"", db_name)[..], &[])
            .await;
        cleanup_client
            .execute(&format!("CREATE DATABASE \"{}\"", db_name)[..], &[])
            .await
            .expect("failed to create test database");

        let db_url = crate::common::pg_url(&db_name);
        let migrator_db = Database::connect(&db_url)
            .await
            .expect("failed to connect for migrations");
        Migrator::up(&migrator_db, None)
            .await
            .expect("failed to run migrations");
        drop(migrator_db);

        let db = Database::connect(&db_url)
            .await
            .expect("failed to connect");
        Self { db, db_name }
    }
}

impl Drop for TestDb {
    fn drop(&mut self) {
        let url = crate::common::pg_base_url();
        let db_name = self.db_name.clone();
        let _ = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                if let Ok((client, conn)) =
                    tokio_postgres::connect(&url, tokio_postgres::NoTls).await
                {
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
                }
            });
        });
    }
}

pub fn allocation_mode_str(mode: AllocationMode) -> &'static str {
    match mode {
        AllocationMode::Dedicated => "dedicated",
        AllocationMode::Shared => "shared",
    }
}

pub async fn insert_user(
    db: &sea_orm::DatabaseConnection,
    username: &str,
    role: Role,
    overrides: QuotaOverride,
) -> Uuid {
    let id = Uuid::new_v4();
    user::Entity::insert(user::ActiveModel {
        id: Set(id),
        username: Set(username.to_string()),
        password_hash: Set("test-hash".to_string()),
        role: Set(role.as_str().to_string()),
        instance_limit: Set(overrides.instance_limit),
        max_cpu_cores: Set(overrides.max_cpu_cores),
        max_ram_bytes: Set(overrides.max_ram_bytes),
        ..Default::default()
    })
    .exec(db)
    .await
    .expect("failed to insert user");
    id
}

pub async fn insert_template(
    db: &sea_orm::DatabaseConnection,
    owner_id: Uuid,
    cores: i32,
    memory: i64,
    allocation_mode: AllocationMode,
) -> openworkspace_api::db::WorkspaceTemplate {
    WorkspaceTemplateRepository::new(db)
        .create_with_allocation_mode(
            allocation_mode_str(allocation_mode),
            "test-template",
            None,
            owner_id,
            "busybox:1",
            cores,
            memory,
            0,
            None,
            "vnc",
            "docker",
            &json!({}),
            &json!({}),
            &json!({}),
            None,
            None,
            "stop",
            0,
            0,
            None,
            "stop",
            false,
        )
        .await
        .expect("failed to insert template")
}

/// Create a workspace instance row directly through the repository and set its
/// status, bypassing the (Docker-backed) activation route.
pub async fn insert_instance(
    db: &sea_orm::DatabaseConnection,
    template_id: Uuid,
    owner_id: Uuid,
    template_name: &str,
    status: &str,
) -> Uuid {
    let repo = WorkspaceInstanceRepository::new(db);
    let instance = repo
        .launch(template_id, owner_id, template_name, false, None)
        .await
        .expect("failed to insert instance");
    repo.update_status(instance.id, status)
        .await
        .expect("failed to set instance status");
    instance.id
}

pub fn launch_request<'a>(
    template: &'a openworkspace_api::db::WorkspaceTemplate,
    user_id: Uuid,
    role: Role,
    overrides: QuotaOverride,
) -> ActivationRequest<'a> {
    ActivationRequest {
        kind: ActivationKind::Launch(LaunchPayload {
            mount_persistent: false,
            resolved_volume_host_path: None,
        }),
        template,
        user_id,
        role,
        user_overrides: overrides,
    }
}
