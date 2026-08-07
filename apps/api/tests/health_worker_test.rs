mod common;

use std::sync::atomic::{AtomicU32, Ordering};

use common::ensure_pg;
use migration::MigratorTrait;
use openworkspace_api::db::{
    PersistentVolumeRepository, UserRepository, WorkspaceInstanceRepository,
    WorkspaceTemplateRepository, VOLUME_STATUS_ORPHANED,
};
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
        let admin_id = admin.id;

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
                "runc",
                &serde_json::json!({}),
                &serde_json::json!({}),
                &serde_json::json!({}),
                None,
                None,
                "remove",
                0,
                0,
                None,
                "pause", false)
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
        repo.update_host_port(instance.id, Some(12345)).await.unwrap();
        instance.id
    }

    async fn create_starting_instance_without_host_port(&self, template_id: uuid::Uuid) -> uuid::Uuid {
        let repo = WorkspaceInstanceRepository::new(&self.db);
        let instance = repo
            .launch(template_id, self.admin_id, "test-no-port", false, None)
            .await
            .unwrap();
        repo.update_container_id(instance.id, "test-container-id")
            .await
            .unwrap();
        repo.update_status(instance.id, "starting").await.unwrap();
        instance.id
    }

    async fn create_template_with_auto_sleep(
        &self,
        name: &str,
        max_run_seconds: Option<i64>,
        timeout_action: &str,
    ) -> uuid::Uuid {
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
                "kasmvnc",
                "runc",
                &serde_json::json!({}),
                &serde_json::json!({}),
                &serde_json::json!({}),
                None,
                max_run_seconds,
                timeout_action,
                0,
                0,
                None,
                "pause", false)
            .await
            .unwrap();
        template.id
    }

    async fn create_running_instance(
        &self,
        template_id: uuid::Uuid,
        started_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> uuid::Uuid {
        let repo = WorkspaceInstanceRepository::new(&self.db);
        let instance = repo
            .launch(template_id, self.admin_id, "auto-sleep-instance", false, None)
            .await
            .unwrap();
        repo.update_container_id(instance.id, "test-container-id")
            .await
            .unwrap();
        repo.update_status(instance.id, "running").await.unwrap();
        repo.update_started_at(instance.id, started_at).await.unwrap();
        instance.id
    }

    async fn create_template_with_keep_time(
        &self,
        name: &str,
        keep_time_seconds: Option<i64>,
        keep_time_action: &str,
    ) -> uuid::Uuid {
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
                "kasmvnc",
                "runc",
                &serde_json::json!({}),
                &serde_json::json!({}),
                &serde_json::json!({}),
                None,
                None,
                "remove",
                0,
                0,
                keep_time_seconds,
                keep_time_action, false)
            .await
            .unwrap();
        template.id
    }

    async fn create_running_instance_with_last_seen_at(
        &self,
        template_id: uuid::Uuid,
        last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> uuid::Uuid {
        let repo = WorkspaceInstanceRepository::new(&self.db);
        let instance = repo
            .launch(template_id, self.admin_id, "keep-time-instance", false, None)
            .await
            .unwrap();
        repo.update_container_id(instance.id, "test-container-id")
            .await
            .unwrap();
        repo.update_status(instance.id, "running").await.unwrap();
        repo.update_last_seen_at(instance.id, last_seen_at).await.unwrap();
        instance.id
    }
}

fn docker_err(msg: &str) -> bollard::errors::Error {
    std::io::Error::other(msg).into()
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

    let vnc_cache = VncCache::new();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(1))
        .build()
        .unwrap();

    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);

    let _ = health_worker::check_instances(
        &instance_repo,
        &vnc_cache,
        &client,
        "172.17.0.1",
    )
    .await;

    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    assert_eq!(instance.status, "starting", "should remain starting when probe fails and not timed out");
}

// ── Verify that a starting instance with no host_port is skipped (no crash, stays starting) ──

#[tokio::test]
async fn test_probe_no_host_port_skips_instance() {
    let ctx = WorkerTestContext::new().await;
    let template_id = ctx.create_template("probe-no-port", "kasmvnc").await;
    let instance_id = ctx.create_starting_instance_without_host_port(template_id).await;

    let vnc_cache = VncCache::new();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(1))
        .build()
        .unwrap();

    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);

    let _ = health_worker::check_instances(
        &instance_repo,
        &vnc_cache,
        &client,
        "172.17.0.1",
    )
    .await;

    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    assert_eq!(instance.status, "starting", "instance without host_port should be skipped, not failed");
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
        "UPDATE workspace_instances SET updated_at = NOW() - INTERVAL '130 seconds' WHERE id = '{}'",
        id_str
    );
    ctx.db.execute(sea_orm::Statement::from_string(
        sea_orm::DatabaseBackend::Postgres,
        raw_sql,
    )).await.unwrap();

    let vnc_cache = VncCache::new();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(1))
        .build()
        .unwrap();

    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);

    let _ = health_worker::check_instances(
        &instance_repo,
        &vnc_cache,
        &client,
        "172.17.0.1",
    )
    .await;

    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    assert_eq!(instance.status, "error", "should become error after timeout");
}

// ── check_auto_sleep ──

#[tokio::test]
async fn test_auto_sleep_remove_over_limit() {
    let ctx = WorkerTestContext::new().await;
    let now = chrono::Utc::now();
    let template_id = ctx.create_template_with_auto_sleep("auto-sleep-remove", Some(3600), "remove").await;
    let instance_id = ctx.create_running_instance(template_id, Some(now - chrono::Duration::seconds(3601))).await;

    let mut mock_docker = MockDockerService::new();
    mock_docker.expect_stop_container_by_id()
        .returning(|_| Box::pin(async { Ok(()) }));
    mock_docker.expect_remove_container_by_id()
        .returning(|_| Box::pin(async { Ok(()) }));

    let vnc_cache = VncCache::new();
    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);
    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    vnc_cache.insert(&instance.access_token, "running");

    let count = health_worker::check_auto_sleep(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(count, 1);

    assert!(instance_repo.find_by_id(instance_id).await.unwrap().is_none());
    assert!(vnc_cache.get(&instance.access_token).is_none());
}

#[tokio::test]
async fn test_auto_sleep_remove_flips_orphaned_volume() {
    let ctx = WorkerTestContext::new().await;
    let now = chrono::Utc::now();
    let template_id = ctx
        .create_template_with_auto_sleep("auto-sleep-remove-vol", Some(3600), "remove")
        .await;

    // A persistent instance: mount_persistent with a resolved host path plus a
    // registry row for that path — exactly the state a real launch leaves.
    let host_path = format!("/tmp/ow_hw_orphan_{}", std::process::id());
    let repo = WorkspaceInstanceRepository::new(&ctx.db);
    let instance = repo
        .launch(template_id, ctx.admin_id, "auto-sleep-instance", true, Some(&host_path))
        .await
        .unwrap();
    repo.update_container_id(instance.id, "test-container-id")
        .await
        .unwrap();
    repo.update_status(instance.id, "running").await.unwrap();
    repo.update_started_at(instance.id, Some(now - chrono::Duration::seconds(3601)))
        .await
        .unwrap();

    PersistentVolumeRepository::new(&ctx.db)
        .upsert(&host_path, ctx.admin_id)
        .await
        .unwrap();

    let mut mock_docker = MockDockerService::new();
    mock_docker
        .expect_stop_container_by_id()
        .returning(|_| Box::pin(async { Ok(()) }));
    mock_docker
        .expect_remove_container_by_id()
        .returning(|_| Box::pin(async { Ok(()) }));

    let vnc_cache = VncCache::new();
    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);
    let instance = instance_repo.find_by_id(instance.id).await.unwrap().unwrap();
    vnc_cache.insert(&instance.access_token, "running");

    let count = health_worker::check_auto_sleep(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    )
    .await
    .unwrap();
    assert_eq!(count, 1);
    assert!(instance_repo.find_by_id(instance.id).await.unwrap().is_none());

    let volume = PersistentVolumeRepository::new(&ctx.db)
        .find_by_host_path(&host_path)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        volume.status, VOLUME_STATUS_ORPHANED,
        "auto-remove must flip the volume registry row to orphaned"
    );
    std::fs::remove_dir_all(&host_path).ok();
}

#[tokio::test]
async fn test_auto_sleep_remove_ignores_docker_failure() {
    let ctx = WorkerTestContext::new().await;
    let now = chrono::Utc::now();
    let template_id = ctx.create_template_with_auto_sleep("auto-sleep-remove-err", Some(3600), "remove").await;
    let instance_id = ctx.create_running_instance(template_id, Some(now - chrono::Duration::seconds(3601))).await;

    let mut mock_docker = MockDockerService::new();
    mock_docker.expect_stop_container_by_id()
        .returning(|_| Box::pin(async { Err(docker_err("stop failed")) }));
    mock_docker.expect_remove_container_by_id()
        .returning(|_| Box::pin(async { Err(docker_err("remove failed")) }));

    let vnc_cache = VncCache::new();
    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);

    let count = health_worker::check_auto_sleep(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(count, 1);
    assert!(instance_repo.find_by_id(instance_id).await.unwrap().is_none());
}

#[tokio::test]
async fn test_auto_sleep_stop_over_limit() {
    let ctx = WorkerTestContext::new().await;
    let now = chrono::Utc::now();
    let template_id = ctx.create_template_with_auto_sleep("auto-sleep-stop", Some(3600), "stop").await;
    let instance_id = ctx.create_running_instance(template_id, Some(now - chrono::Duration::seconds(3601))).await;

    let mut mock_docker = MockDockerService::new();
    mock_docker.expect_stop_container_by_id()
        .returning(|_| Box::pin(async { Ok(()) }));

    let vnc_cache = VncCache::new();
    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);

    let count = health_worker::check_auto_sleep(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(count, 1);

    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    assert_eq!(instance.status, "stopped");
    assert!(instance.started_at.is_none());
}

#[tokio::test]
async fn test_auto_sleep_pause_over_limit() {
    let ctx = WorkerTestContext::new().await;
    let now = chrono::Utc::now();
    let template_id = ctx.create_template_with_auto_sleep("auto-sleep-pause", Some(3600), "pause").await;
    let instance_id = ctx.create_running_instance(template_id, Some(now - chrono::Duration::seconds(3601))).await;

    let mut mock_docker = MockDockerService::new();
    mock_docker.expect_pause_container_by_id()
        .returning(|_| Box::pin(async { Ok(()) }));

    let vnc_cache = VncCache::new();
    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);

    let count = health_worker::check_auto_sleep(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(count, 1);

    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    assert_eq!(instance.status, "paused");
    assert!(instance.last_seen_at.is_none());
}

#[tokio::test]
async fn test_keep_time_active_connection_resets_timer() {
    let ctx = WorkerTestContext::new().await;
    let now = chrono::Utc::now();
    let template_id = ctx.create_template_with_keep_time("keep-time-connected", Some(3600), "pause").await;
    let instance_id = ctx.create_running_instance_with_last_seen_at(template_id, Some(now - chrono::Duration::seconds(7200))).await;

    let mut mock_docker = MockDockerService::new();
    mock_docker.expect_has_session_connection()
        .returning(|_, _| Box::pin(async { Ok(true) }));

    let vnc_cache = VncCache::new();
    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);

    let count = health_worker::check_keep_time(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(count, 0, "an attached session must not be reclaimed");

    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    assert_eq!(instance.status, "running");
    let refreshed = instance.last_seen_at.expect("last_seen_at should be refreshed");
    assert!(
        refreshed >= now - chrono::Duration::seconds(5),
        "last_seen_at should be reset to ~now, got {}",
        refreshed
    );
}

#[tokio::test]
async fn test_keep_time_connection_check_failure_skips() {
    let ctx = WorkerTestContext::new().await;
    let now = chrono::Utc::now();
    let template_id = ctx.create_template_with_keep_time("keep-time-check-fail", Some(3600), "remove").await;
    let instance_id = ctx.create_running_instance_with_last_seen_at(template_id, Some(now - chrono::Duration::seconds(7200))).await;

    let mut mock_docker = MockDockerService::new();
    mock_docker.expect_has_session_connection()
        .returning(|_, _| Box::pin(async { Err("ss failed".to_string()) }));

    let vnc_cache = VncCache::new();
    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);

    let count = health_worker::check_keep_time(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(count, 0, "connection check failures must fail open (no reclaim)");

    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    assert_eq!(instance.status, "running");
    assert!(instance.last_seen_at.is_some(), "stale last_seen_at must be preserved");
}

#[tokio::test]
async fn test_auto_sleep_not_reached_limit() {
    let ctx = WorkerTestContext::new().await;
    let now = chrono::Utc::now();
    let template_id = ctx.create_template_with_auto_sleep("auto-sleep-not-yet", Some(3600), "remove").await;
    let instance_id = ctx.create_running_instance(template_id, Some(now - chrono::Duration::seconds(3599))).await;

    let mock_docker = MockDockerService::new();
    let vnc_cache = VncCache::new();
    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);

    let count = health_worker::check_auto_sleep(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(count, 0);

    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    assert_eq!(instance.status, "running");
}

#[tokio::test]
async fn test_auto_sleep_skips_old_instance_without_started_at() {
    let ctx = WorkerTestContext::new().await;
    let now = chrono::Utc::now();
    let template_id = ctx.create_template_with_auto_sleep("auto-sleep-old", Some(3600), "remove").await;
    let instance_id = ctx.create_running_instance(template_id, None).await;

    let mock_docker = MockDockerService::new();
    let vnc_cache = VncCache::new();
    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);

    let count = health_worker::check_auto_sleep(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(count, 0);

    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    assert_eq!(instance.status, "running");
}

#[tokio::test]
async fn test_auto_sleep_skips_disabled_template() {
    let ctx = WorkerTestContext::new().await;
    let now = chrono::Utc::now();
    let template_id = ctx.create_template_with_auto_sleep("auto-sleep-disabled", None, "remove").await;
    let instance_id = ctx.create_running_instance(template_id, Some(now - chrono::Duration::seconds(7200))).await;

    let mock_docker = MockDockerService::new();
    let vnc_cache = VncCache::new();
    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);

    let count = health_worker::check_auto_sleep(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(count, 0);

    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    assert_eq!(instance.status, "running");
}

#[tokio::test]
async fn test_auto_sleep_reads_template_config_each_scan() {
    let ctx = WorkerTestContext::new().await;
    let now = chrono::Utc::now();
    let template_id = ctx.create_template_with_auto_sleep("auto-sleep-recheck", Some(7200), "stop").await;
    let instance_id = ctx.create_running_instance(template_id, Some(now - chrono::Duration::seconds(4000))).await;

    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);

    let vnc_cache = VncCache::new();
    let mock_docker = MockDockerService::new();
    let count = health_worker::check_auto_sleep(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(count, 0, "raised limit should not trigger");

    ctx.db.execute(sea_orm::Statement::from_string(
        sea_orm::DatabaseBackend::Postgres,
        format!("UPDATE workspace_templates SET max_run_seconds = 3600 WHERE id = '{}'", template_id),
    )).await.unwrap();

    let mut mock_docker = MockDockerService::new();
    mock_docker.expect_stop_container_by_id()
        .returning(|_| Box::pin(async { Ok(()) }));
    let count = health_worker::check_auto_sleep(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(count, 1, "lowered limit should trigger on next scan");

    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    assert_eq!(instance.status, "stopped");
}

#[tokio::test]
async fn test_auto_sleep_stop_clears_vnc_cache_and_route() {
    let ctx = WorkerTestContext::new().await;
    let now = chrono::Utc::now();
    let template_id = ctx.create_template_with_auto_sleep("auto-sleep-stop-cache", Some(3600), "stop").await;
    let instance_id = ctx.create_running_instance(template_id, Some(now - chrono::Duration::seconds(3601))).await;

    let mut mock_docker = MockDockerService::new();
    mock_docker.expect_stop_container_by_id()
        .returning(|_| Box::pin(async { Ok(()) }));

    let vnc_cache = VncCache::new();
    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);
    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    vnc_cache.insert(&instance.access_token, "running");

    health_worker::check_auto_sleep(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();

    assert!(vnc_cache.get(&instance.access_token).is_none());
}

#[tokio::test]
async fn test_auto_sleep_pause_preserves_vnc_cache() {
    let ctx = WorkerTestContext::new().await;
    let now = chrono::Utc::now();
    let template_id = ctx.create_template_with_auto_sleep("auto-sleep-pause-cache", Some(3600), "pause").await;
    let instance_id = ctx.create_running_instance(template_id, Some(now - chrono::Duration::seconds(3601))).await;

    let mut mock_docker = MockDockerService::new();
    mock_docker.expect_pause_container_by_id()
        .returning(|_| Box::pin(async { Ok(()) }));

    let vnc_cache = VncCache::new();
    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);
    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    vnc_cache.insert(&instance.access_token, "running");

    health_worker::check_auto_sleep(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();

    let found = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    assert_eq!(found.status, "paused");
    assert!(
        vnc_cache.get(&instance.access_token).is_some(),
        "pause should keep the VNC route/cache alive for resume"
    );
}

#[tokio::test]
async fn test_auto_sleep_no_double_trigger_on_rescan() {
    let ctx = WorkerTestContext::new().await;
    let now = chrono::Utc::now();
    let template_id = ctx.create_template_with_auto_sleep("auto-sleep-rescan", Some(3600), "stop").await;
    let instance_id = ctx.create_running_instance(template_id, Some(now - chrono::Duration::seconds(3601))).await;

    let mut mock_docker = MockDockerService::new();
    mock_docker.expect_stop_container_by_id()
        .times(1)
        .returning(|_| Box::pin(async { Ok(()) }));

    let vnc_cache = VncCache::new();
    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);

    let first = health_worker::check_auto_sleep(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(first, 1);

    let second = health_worker::check_auto_sleep(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(second, 0, "already-triggered instance should not re-trigger");

    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    assert_eq!(instance.status, "stopped");
}

// ── check_keep_time ──

#[tokio::test]
async fn test_keep_time_pause_fires() {
    let ctx = WorkerTestContext::new().await;
    let now = chrono::Utc::now();
    let template_id = ctx.create_template_with_keep_time("keep-time-pause", Some(3600), "pause").await;
    let instance_id = ctx.create_running_instance_with_last_seen_at(template_id, Some(now - chrono::Duration::seconds(7200))).await;

    let mut mock_docker = MockDockerService::new();
    mock_docker.expect_has_session_connection()
        .returning(|_, _| Box::pin(async { Ok(false) }));
    mock_docker.expect_pause_container_by_id()
        .returning(|_| Box::pin(async { Ok(()) }));

    let vnc_cache = VncCache::new();
    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);

    let count = health_worker::check_keep_time(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(count, 1);

    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    assert_eq!(instance.status, "paused");
    assert!(instance.last_seen_at.is_none());
}

#[tokio::test]
async fn test_keep_time_stop_fires() {
    let ctx = WorkerTestContext::new().await;
    let now = chrono::Utc::now();
    let template_id = ctx.create_template_with_keep_time("keep-time-stop", Some(3600), "stop").await;
    let instance_id = ctx.create_running_instance_with_last_seen_at(template_id, Some(now - chrono::Duration::seconds(7200))).await;

    let mut mock_docker = MockDockerService::new();
    mock_docker.expect_has_session_connection()
        .returning(|_, _| Box::pin(async { Ok(false) }));
    mock_docker.expect_stop_container_by_id()
        .returning(|_| Box::pin(async { Ok(()) }));

    let vnc_cache = VncCache::new();
    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);
    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    vnc_cache.insert(&instance.access_token, "running");

    let count = health_worker::check_keep_time(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(count, 1);

    assert!(vnc_cache.get(&instance.access_token).is_none());

    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    assert_eq!(instance.status, "stopped");
    assert!(instance.last_seen_at.is_none());
}

#[tokio::test]
async fn test_keep_time_remove_fires() {
    let ctx = WorkerTestContext::new().await;
    let now = chrono::Utc::now();
    let template_id = ctx.create_template_with_keep_time("keep-time-remove", Some(3600), "remove").await;
    let instance_id = ctx.create_running_instance_with_last_seen_at(template_id, Some(now - chrono::Duration::seconds(7200))).await;

    let mut mock_docker = MockDockerService::new();
    mock_docker.expect_has_session_connection()
        .returning(|_, _| Box::pin(async { Ok(false) }));
    mock_docker.expect_stop_container_by_id()
        .returning(|_| Box::pin(async { Ok(()) }));
    mock_docker.expect_remove_container_by_id()
        .returning(|_| Box::pin(async { Ok(()) }));

    let vnc_cache = VncCache::new();
    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);
    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    vnc_cache.insert(&instance.access_token, "running");

    let count = health_worker::check_keep_time(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(count, 1);

    assert!(instance_repo.find_by_id(instance_id).await.unwrap().is_none());
    assert!(vnc_cache.get(&instance.access_token).is_none());
}

#[tokio::test]
async fn test_keep_time_not_yet_expired() {
    let ctx = WorkerTestContext::new().await;
    let now = chrono::Utc::now();
    let template_id = ctx.create_template_with_keep_time("keep-time-recent", Some(3600), "remove").await;
    let instance_id = ctx.create_running_instance_with_last_seen_at(template_id, Some(now)).await;

    let mut mock_docker = MockDockerService::new();
    mock_docker.expect_has_session_connection()
        .returning(|_, _| Box::pin(async { Ok(false) }));
    let vnc_cache = VncCache::new();
    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);

    let count = health_worker::check_keep_time(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(count, 0);

    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    assert_eq!(instance.status, "running");
}

#[tokio::test]
async fn test_keep_time_skips_null_last_seen_at() {
    let ctx = WorkerTestContext::new().await;
    let now = chrono::Utc::now();
    let template_id = ctx.create_template_with_keep_time("keep-time-null-seen", Some(3600), "remove").await;
    let instance_id = ctx.create_running_instance_with_last_seen_at(template_id, None).await;

    let mock_docker = MockDockerService::new();
    let vnc_cache = VncCache::new();
    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);

    let count = health_worker::check_keep_time(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(count, 0);

    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    assert_eq!(instance.status, "running");
}

#[tokio::test]
async fn test_keep_time_skips_disabled_template() {
    let ctx = WorkerTestContext::new().await;
    let now = chrono::Utc::now();
    let template_id = ctx.create_template_with_keep_time("keep-time-disabled", None, "remove").await;
    let instance_id = ctx.create_running_instance_with_last_seen_at(template_id, Some(now - chrono::Duration::seconds(7200))).await;

    let mock_docker = MockDockerService::new();
    let vnc_cache = VncCache::new();
    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);

    let count = health_worker::check_keep_time(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(count, 0);

    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    assert_eq!(instance.status, "running");
}

#[tokio::test]
async fn test_keep_time_honors_midrun_template_change() {
    let ctx = WorkerTestContext::new().await;
    let now = chrono::Utc::now();
    let template_id = ctx.create_template_with_keep_time("keep-time-recheck", Some(60), "stop").await;
    let instance_id = ctx.create_running_instance_with_last_seen_at(template_id, Some(now - chrono::Duration::seconds(120))).await;

    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);

    let template = template_repo.find_by_id(template_id).await.unwrap().unwrap();
    template_repo
        .update(
            template_id,
            &template.name,
            template.description.as_deref(),
            &template.image,
            template.cores,
            template.memory,
            template.gpu_count,
            template.docker_registry.as_deref(),
            &template.remote_type,
            &template.container_runtime,
            &template.run_config,
            &template.exec_config,
            &template.volume_mappings,
            template.persistent_storage_path.as_deref(),
            template.max_run_seconds,
            &template.timeout_action,
            template.network_bandwidth_up_mbps,
            template.network_bandwidth_down_mbps,
            Some(3600),
            "stop", false)
        .await
        .unwrap();

    let vnc_cache = VncCache::new();
    let mut mock_docker = MockDockerService::new();
    mock_docker.expect_has_session_connection()
        .returning(|_, _| Box::pin(async { Ok(false) }));
    let count = health_worker::check_keep_time(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(count, 0, "raised keep_time should not trigger");

    let template = template_repo.find_by_id(template_id).await.unwrap().unwrap();
    template_repo
        .update(
            template_id,
            &template.name,
            template.description.as_deref(),
            &template.image,
            template.cores,
            template.memory,
            template.gpu_count,
            template.docker_registry.as_deref(),
            &template.remote_type,
            &template.container_runtime,
            &template.run_config,
            &template.exec_config,
            &template.volume_mappings,
            template.persistent_storage_path.as_deref(),
            template.max_run_seconds,
            &template.timeout_action,
            template.network_bandwidth_up_mbps,
            template.network_bandwidth_down_mbps,
            Some(60),
            "stop", false)
        .await
        .unwrap();

    let mut mock_docker = MockDockerService::new();
    mock_docker.expect_has_session_connection()
        .returning(|_, _| Box::pin(async { Ok(false) }));
    mock_docker.expect_stop_container_by_id()
        .returning(|_| Box::pin(async { Ok(()) }));
    let count = health_worker::check_keep_time(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(count, 1, "lowered keep_time should trigger on next scan");

    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    assert_eq!(instance.status, "stopped");
}

#[tokio::test]
async fn test_keep_time_no_retrigger_after_pause() {
    let ctx = WorkerTestContext::new().await;
    let now = chrono::Utc::now();
    let template_id = ctx.create_template_with_keep_time("keep-time-rescan", Some(3600), "pause").await;
    let instance_id = ctx.create_running_instance_with_last_seen_at(template_id, Some(now - chrono::Duration::seconds(7200))).await;

    let mut mock_docker = MockDockerService::new();
    mock_docker.expect_has_session_connection()
        .returning(|_, _| Box::pin(async { Ok(false) }));
    mock_docker.expect_pause_container_by_id()
        .times(1)
        .returning(|_| Box::pin(async { Ok(()) }));

    let vnc_cache = VncCache::new();
    let instance_repo = WorkspaceInstanceRepository::new(&ctx.db);
    let template_repo = WorkspaceTemplateRepository::new(&ctx.db);

    let first = health_worker::check_keep_time(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(first, 1);

    let second = health_worker::check_keep_time(
        &instance_repo,
        &template_repo,
        &mock_docker,
        &vnc_cache,
        now,
    ).await.unwrap();
    assert_eq!(second, 0, "already-triggered instance should not re-trigger");

    let instance = instance_repo.find_by_id(instance_id).await.unwrap().unwrap();
    assert_eq!(instance.status, "paused");
    assert!(instance.last_seen_at.is_none());
}
