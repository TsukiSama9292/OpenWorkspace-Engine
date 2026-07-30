mod common;

use std::sync::atomic::{AtomicU32, Ordering};

use common::ensure_pg;
use migration::MigratorTrait;
use openworkspace_api::db::{WorkspaceInstanceRepository, WorkspaceTemplateRepository, UserRepository};
use openworkspace_api::docker::MockDockerService;
use openworkspace_api::health_worker;
use openworkspace_api::vnc_cache::VncCache;
use sea_orm::ConnectionTrait;
use sea_orm::DatabaseConnection;

static COUNTER: AtomicU32 = AtomicU32::new(0);

struct WorkerTestContext {
    db: DatabaseConnection,
    admin_id: uuid::Uuid,
    db_name: String,
}

impl WorkerTestContext {
    async fn new() -> Self {
        ensure_pg().await;
        let counter = COUNTER.fetch_add(1, Ordering::Relaxed);
        let db_name = format!("worker_test_{}_{:04}", std::process::id(), counter);
        let base_url = common::pg_base_url();

        let (pg_client, conn) = 'connect: {
            for attempt in 0..20 {
                match tokio_postgres::connect(&base_url, tokio_postgres::NoTls).await {
                    Ok(c) => break 'connect c,
                    Err(e) => {
                        if attempt == 19 {
                            panic!("failed to connect after retries: {}", e);
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(250 * (attempt + 1))).await;
                    }
                }
            }
            unreachable!()
        };
        tokio::spawn(async move { let _ = conn.await; });
        pg_client
            .execute(&format!("DROP DATABASE IF EXISTS \"{}\"", db_name)[..], &[])
            .await
            .unwrap();
        pg_client
            .execute(&format!("CREATE DATABASE \"{}\"", db_name)[..], &[])
            .await
            .unwrap();
        let db_url = common::pg_url(&db_name);

        let migrator_db = sea_orm::Database::connect(&db_url).await.unwrap();
        migration::Migrator::up(&migrator_db, None).await.unwrap();
        drop(migrator_db);

        let db = sea_orm::Database::connect(&db_url).await.unwrap();

        UserRepository::new(&db)
            .seed_admin("admin")
            .await
            .unwrap();

        let admin = UserRepository::new(&db)
            .find_by_username("admin")
            .await
            .unwrap()
            .unwrap();
        let admin_id = admin.0;

        WorkerTestContext { db, admin_id, db_name }
    }

    async fn create_template(&self, name: &str, remote_type: &str) -> uuid::Uuid {
        let repo = WorkspaceTemplateRepository::new(&self.db);
        let template = repo
            .create(
                name,
                None,
                self.admin_id,
                "test-image",
                1,
                1024,
                0,
                None,
                remote_type,
                "docker",
                &serde_json::json!({}),
                &serde_json::json!({}),
                &serde_json::json!({}),
                None,
            )
            .await
            .unwrap();
        template.id
    }

    async fn create_starting_instance(&self, template_id: uuid::Uuid) -> uuid::Uuid {
        let repo = WorkspaceInstanceRepository::new(&self.db);
        let instance = repo
            .launch(template_id, self.admin_id, "test-instance", false, None)
            .await
            .unwrap();
        repo.update_container_id(instance.id, "test-container-id")
            .await
            .unwrap();
        repo.update_status(instance.id, "starting").await.unwrap();
        instance.id
    }
}

impl Drop for WorkerTestContext {
    fn drop(&mut self) {
        let url = common::pg_base_url();
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
                        .execute(
                            &format!("DROP DATABASE IF EXISTS \"{}\"", db_name)[..],
                            &[],
                        )
                        .await;
                }
            });
        });
    }
}

// ── Verify that a starting instance with probe failure stays starting (no timeout) ──

#[tokio::test]
async fn test_probe_failure_stays_starting() {
    let ctx = WorkerTestContext::new().await;
    let template_id = ctx.create_template("probe-fail", "kasmvnc").await;
    let instance_id = ctx.create_starting_instance(template_id).await;

    let mut mock_docker = MockDockerService::new();
    mock_docker
        .expect_network_name()
        .return_const("ow-test".to_string());
    mock_docker
        .expect_get_container_ip()
        .returning(|_, _| Box::pin(async { Ok("10.0.0.1".to_string()) }));

    let vnc_cache = VncCache::new();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(1))
        .build()
        .unwrap();

    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);

    let _ = health_worker::check_instances(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        &client,
    )
    .await;

    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    assert_eq!(instance.status, "starting", "should remain starting when probe fails and not timed out");
}

// ── Verify that a starting instance with probe failure + timeout becomes error ──

#[tokio::test]
async fn test_probe_timeout_sets_error() {
    let ctx = WorkerTestContext::new().await;
    let template_id = ctx.create_template("probe-timeout", "kasmvnc").await;
    let instance_id = ctx.create_starting_instance(template_id).await;

    // Set updated_at to 130 seconds ago to trigger timeout
    let id_str = instance_id.to_string();
    let raw_sql = format!(
        "UPDATE workspace_instance SET updated_at = NOW() - INTERVAL '130 seconds' WHERE id = '{}'",
        id_str
    );
    ctx.db.execute(sea_orm::Statement::from_string(
        sea_orm::DatabaseBackend::Postgres,
        raw_sql,
    )).await.unwrap();

    let mut mock_docker = MockDockerService::new();
    mock_docker
        .expect_network_name()
        .return_const("ow-test".to_string());
    mock_docker
        .expect_get_container_ip()
        .returning(|_, _| Box::pin(async { Ok("10.0.0.1".to_string()) }));

    let vnc_cache = VncCache::new();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(1))
        .build()
        .unwrap();

    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);

    let _ = health_worker::check_instances(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        &client,
    )
    .await;

    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    assert_eq!(instance.status, "error", "should become error after timeout");
}
