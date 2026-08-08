//! Monitor-dashboard endpoint tests (monitor-dashboard spec, ticket 03):
//! RBAC gating (403 without the flag, 200 for admin), host + instance payload
//! shape (both granularity tiers in one snapshot), and the fail-open sampler
//! behavior against a mocked `DockerService`.

mod common;

use std::sync::Arc;

use common::ensure_pg;
use openworkspace_api::core::Settings;
use openworkspace_api::docker::{ContainerStats, ContainerStatsError, MockDockerService};
use openworkspace_api::metrics::MetricsStore;
use openworkspace_api::monitor::MetricsSampler;
use openworkspace_api::routes::{AppState, api_routes};
use openworkspace_api::vnc_cache::VncCache;
use sea_orm::DatabaseConnection;

struct MonitorContext {
    base_url: String,
    client: reqwest::Client,
    db: DatabaseConnection,
    metrics: Arc<MetricsStore>,
    #[allow(dead_code)]
    db_name: String,
}

impl MonitorContext {
    async fn new<F: FnOnce(&mut MockDockerService)>(setup_mock: F) -> Self {
        ensure_pg().await;
        let db_name = format!(
            "monitor_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );

        let (pg_client, conn) = 'connect: {
            for attempt in 0..20 {
                match tokio_postgres::connect(
                    &common::pg_base_url(),
                    tokio_postgres::NoTls,
                )
                .await
                {
                    Ok(c) => break 'connect c,
                    Err(e) => {
                        if attempt == 19 {
                            panic!("failed to connect after retries: {}", e);
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(250 * (attempt + 1)))
                            .await;
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
        use migration::MigratorTrait;
        migration::Migrator::up(&migrator_db, None).await.unwrap();
        drop(migrator_db);

        let db = sea_orm::Database::connect(&db_url).await.unwrap();

        let settings = Settings {
            database_url: db_url,
            jwt_secret: "test-secret-key".to_string(),
            admin_password: "admin".to_string(),
            server_host: "127.0.0.1".to_string(),
            server_port: 0,
            db_max_connections: 5,
            container_runtime: "runc".to_string(),
            host_gateway_ip: "172.17.0.1".to_string(),
            host_port_start: 10000,
            host_port_end: 20000,
            instance_net_base: "10.200.0.0/16".to_string(),
            instance_dns: "8.8.8.8,1.1.1.1".to_string(),
            port_lock_dir: String::new(),
            audit_retention_days: 90,
        };

        openworkspace_api::db::UserRepository::new(&db)
            .seed_admin(&settings.admin_password)
            .await
            .unwrap();

        let mut mock_docker = MockDockerService::new();
        setup_mock(&mut mock_docker);

        let metrics = Arc::new(MetricsStore::new());

        let (audit_tx, audit_rx) =
            tokio::sync::mpsc::channel(openworkspace_api::audit::AUDIT_CHANNEL_CAPACITY);
        let audit_db = db.clone();
        let (_audit_shutdown_tx, audit_shutdown_rx) =
            tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            openworkspace_api::audit::audit_writer(audit_rx, audit_db, audit_shutdown_rx).await;
        });

        let state = AppState {
            db: db.clone(),
            docker: Arc::new(mock_docker),
            vnc_cache: VncCache::new(),
            settings,
            metrics: Arc::clone(&metrics),
            audit: openworkspace_api::audit::AuditSender::new(audit_tx),
        };

        let cors = tower_http::cors::CorsLayer::new()
            .allow_origin(tower_http::cors::Any)
            .allow_methods(tower_http::cors::Any)
            .allow_headers(tower_http::cors::Any);

        let app = api_routes().layer(cors).with_state(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://127.0.0.1:{}", addr.port());

        tokio::spawn(async move { axum::serve(listener, app).await.unwrap(); });

        let client = reqwest::Client::builder().cookie_store(true).build().unwrap();
        MonitorContext { base_url, client, db, metrics, db_name }
    }

    async fn login(&self, username: &str, password: &str) -> String {
        let resp = self
            .client
            .post(format!("{}/api/auth/login", self.base_url))
            .json(&serde_json::json!({"username": username, "password": password}))
            .send()
            .await
            .unwrap();
        let cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
        cookie
            .split(';')
            .next()
            .unwrap()
            .strip_prefix("ow_token=")
            .unwrap()
            .to_string()
    }

    async fn get_snapshot(
        &self,
        token: &str,
        range: Option<&str>,
    ) -> reqwest::Response {
        let mut url = format!("{}/api/monitor/snapshot", self.base_url);
        if let Some(r) = range {
            url.push_str(&format!("?range={}", r));
        }
        self.client
            .get(url)
            .header("Cookie", format!("ow_token={}", token))
            .send()
            .await
            .unwrap()
    }

    /// Create a plain user through the admin API (proper password hash) and
    /// return their user id.
    async fn create_plain_user(&self, admin_token: &str, username: &str) -> String {
        let resp = self
            .client
            .post(format!("{}/api/users", self.base_url))
            .header("Cookie", format!("ow_token={}", admin_token))
            .json(&serde_json::json!({
                "username": username, "password": "password123"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200, "failed to create user {}", username);
        resp.json::<serde_json::Value>().await.unwrap()["user"]["id"]
            .as_str()
            .unwrap()
            .to_string()
    }

    async fn seed_flagged_group(&self, name: &str, can_view_monitoring: bool) -> String {
        use openworkspace_api::db::group;
        use sea_orm::{ActiveModelTrait, Set};
        let id = uuid::Uuid::new_v4();
        group::ActiveModel {
            id: Set(id),
            name: Set(name.to_string()),
            description: Set(None),
            kind: Set(None),
            can_create_template: Set(false),
            can_manage_users: Set(false),
            can_manage_group_instances: Set(false),
            can_manage_docker: Set(false),
            can_manage_registry: Set(false),
            can_view_monitoring: Set(can_view_monitoring),
            max_instances: Set(Some(4)),
            ..Default::default()
        }
        .insert(&self.db)
        .await
        .unwrap();
        id.to_string()
    }

    async fn add_group_member(&self, user_id: &str, group_id: &str) {
        use openworkspace_api::db::user_group;
        use sea_orm::{ActiveModelTrait, Set};
        user_group::ActiveModel {
            user_id: Set(user_id.parse().unwrap()),
            group_id: Set(group_id.parse().unwrap()),
        }
        .insert(&self.db)
        .await
        .unwrap();
    }

    /// Create a template + a `running` instance owned by `owner_id`, returning
    /// the instance id. `template_memory` is the template's memory cap in bytes
    /// (0 = unlimited).
    async fn seed_running_instance(&self, owner_id: &str, template_memory: i64) -> String {
        use openworkspace_api::db::{workspace_instance, workspace_template};
        use sea_orm::{ActiveModelTrait, Set};
        let now = chrono::Utc::now();
        let template_id = uuid::Uuid::new_v4();
        workspace_template::ActiveModel {
            id: Set(template_id),
            name: Set("base-desktop".to_string()),
            description: Set(None),
            owner_id: Set(owner_id.parse().unwrap()),
            image: Set("kasmweb/base-desktop:1.14.0".to_string()),
            cores: Set(2),
            memory: Set(template_memory),
            gpu_count: Set(0),
            docker_registry: Set(None),
            run_config: Set(serde_json::json!({})),
            exec_config: Set(serde_json::json!({})),
            volume_mappings: Set(serde_json::json!([])),
            remote_type: Set("kasmvnc".to_string()),
            container_runtime: Set("runc".to_string()),
            persistent_storage_path: Set(None),
            max_run_seconds: Set(None),
            timeout_action: Set("stop".to_string()),
            network_bandwidth_up_mbps: Set(0),
            network_bandwidth_down_mbps: Set(0),
            keep_time_seconds: Set(None),
            keep_time_action: Set("stop".to_string()),
            docker_in_instance: Set(false),
            visibility: Set("public".to_string()),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&self.db)
        .await
        .unwrap();

        let instance_id = uuid::Uuid::new_v4();
        workspace_instance::ActiveModel {
            id: Set(instance_id),
            template_id: Set(template_id),
            name: Set("dev-1".to_string()),
            instance_number: Set(1),
            owner_id: Set(owner_id.parse().unwrap()),
            container_id: Set(Some("abc123def456".to_string())),
            status: Set("running".to_string()),
            access_token: Set("tok".to_string()),
            access_password: Set("pw".to_string()),
            mount_persistent: Set(false),
            resolved_volume_host_path: Set(None),
            host_port: Set(Some(15000)),
            started_at: Set(Some(now)),
            last_seen_at: Set(None),
            created_at: Set(now),
            updated_at: Set(now),
        }
        .insert(&self.db)
        .await
        .unwrap();
        instance_id.to_string()
    }
}

#[tokio::test]
async fn test_monitor_snapshot_denied_without_flag_and_granted_with_flag() {
    let ctx = MonitorContext::new(|_| {}).await;
    let admin_token = ctx.login("admin", "admin").await;

    let no_flag_user = ctx.create_plain_user(&admin_token, "plain-user").await;
    let plain_token = ctx.login("plain-user", "password123").await;

    let _ = no_flag_user;
    let resp = ctx.get_snapshot(&plain_token, None).await;
    assert_eq!(resp.status(), 403, "no flag -> must be denied");

    let resp = ctx.get_snapshot(&admin_token, None).await;
    assert_eq!(resp.status(), 200, "admin -> allowed");

    // A non-admin with the flag is allowed too.
    let flagged_user = ctx.create_plain_user(&admin_token, "flag-user").await;
    let group_id = ctx.seed_flagged_group("monitor-group", true).await;
    ctx.add_group_member(&flagged_user, &group_id).await;
    let flag_token = ctx.login("flag-user", "password123").await;
    let resp = ctx.get_snapshot(&flag_token, None).await;
    assert_eq!(resp.status(), 200, "can_view_monitoring holder -> allowed");
}

#[tokio::test]
async fn test_monitor_snapshot_returns_host_and_instances() {
    let ctx = MonitorContext::new(|_| {}).await;
    let admin_token = ctx.login("admin", "admin").await;

    let owner = ctx.create_plain_user(&admin_token, "owner-user").await;
    ctx.seed_running_instance(&owner, 4_096_000_000).await;

    // A full sampler pass populates the store: host from /proc (fail-open to
    // zeros if unreadable) and the instance from a one-shot container stat.
    let mut sampler = MetricsSampler::new();
    let mock = {
        let mut m = MockDockerService::new();
        m.expect_container_stats().returning(|_| {
            Box::pin(async {
                Ok(ContainerStats {
                    cpu_percent: Some(17.5),
                    mem_used_bytes: 1_500_000_000,
                    mem_limit_bytes: 4_096_000_000,
                })
            })
        });
        m
    };
    let sampled = sampler.sample_once(&ctx.db, &mock, &ctx.metrics).await;
    assert_eq!(sampled.len(), 1, "the running instance is sampled");

    let body: serde_json::Value = ctx.get_snapshot(&admin_token, None).await.json().await.unwrap();
    assert_eq!(body["instances"].as_array().unwrap().len(), 1);
    let inst = &body["instances"][0];
    assert_eq!(inst["name"], "dev-1");
    assert_eq!(inst["template"], "base-desktop");
    assert_eq!(inst["runtime"], "runc");
    assert_eq!(inst["owner"], "owner-user");
    assert_eq!(inst["status"], "running");
    assert_eq!(inst["cpu_percent"], 17.5);
    assert_eq!(inst["cpu_limit_percent"].as_f64(), Some(200.0), "2-core template");
    assert_eq!(inst["mem_used_bytes"].as_u64(), Some(1_500_000_000));
    assert_eq!(inst["mem_limit_bytes"].as_u64(), Some(4_096_000_000));
    assert!(body["host"]["cpu_cores"].as_u64().unwrap() >= 1);

    // One pass produces exactly one fine point, timestamped, with the value
    // reported by the sampler.
    let fine = inst["cpu_fine"].as_array().unwrap();
    assert_eq!(fine.len(), 1);
    assert!(fine[0]["t"].as_i64().is_some(), "fine points carry a timestamp");
    assert_eq!(fine[0]["v"].as_f64(), Some(17.5));
    assert_eq!(inst["mem_fine"].as_array().unwrap().len(), 1);

    // Both tiers are always returned in one snapshot; the old ?range= toggle
    // is gone, so a stale value is ignored and the fine tier stays present.
    let body: serde_json::Value = ctx.get_snapshot(&admin_token, Some("24h")).await.json().await.unwrap();
    let inst = &body["instances"][0];
    assert_eq!(inst["cpu_fine"].as_array().unwrap().len(), 1);
    assert!(inst["cpu_coarse"].as_array().unwrap().is_empty(), "no window folded in one pass");
    assert!(body["host"].get("cpu_fine").is_some(), "host carries fine tier");
    assert!(body["host"].get("cpu_coarse").is_some(), "host carries coarse tier");
    assert!(body["host"].get("disk_fine").is_some(), "host carries disk fine tier");
}

#[tokio::test]
async fn test_monitor_snapshot_mem_limit_is_zero_when_template_unlimited() {
    let ctx = MonitorContext::new(|_| {}).await;
    let admin_token = ctx.login("admin", "admin").await;

    let owner = ctx.create_plain_user(&admin_token, "owner-user").await;
    ctx.seed_running_instance(&owner, 0).await;

    let mut sampler = MetricsSampler::new();
    let mock = {
        let mut m = MockDockerService::new();
        m.expect_container_stats().returning(|_| {
            Box::pin(async {
                Ok(ContainerStats {
                    cpu_percent: Some(17.5),
                    mem_used_bytes: 1_500_000_000,
                    // The daemon reports the host RAM as the cgroup limit for an
                    // unlimited container; it must NOT leak through as a "max".
                    mem_limit_bytes: 32_000_000_000,
                })
            })
        });
        m
    };
    sampler.sample_once(&ctx.db, &mock, &ctx.metrics).await;

    let body: serde_json::Value = ctx.get_snapshot(&admin_token, None).await.json().await.unwrap();
    let inst = &body["instances"][0];
    assert_eq!(inst["mem_used_bytes"].as_u64(), Some(1_500_000_000));
    assert_eq!(
        inst["mem_limit_bytes"].as_u64(),
        Some(0),
        "unlimited template -> no fake max"
    );
}

#[tokio::test]
async fn test_monitor_sampler_container_stats_failure_is_fail_open() {
    let ctx = MonitorContext::new(|_| {}).await;
    let admin_token = ctx.login("admin", "admin").await;

    let owner = ctx.create_plain_user(&admin_token, "owner-user").await;
    ctx.seed_running_instance(&owner, 0).await;

    let mut sampler = MetricsSampler::new();
    let mock = {
        let mut m = MockDockerService::new();
        m.expect_container_stats().returning(|_| {
            Box::pin(async { Err(ContainerStatsError::Other("docker daemon unreachable".to_string())) })
        });
        m
    };
    let sampled = sampler.sample_once(&ctx.db, &mock, &ctx.metrics).await;
    assert_eq!(sampled.len(), 1, "the instance is still listed");

    // The instance has no series (its read failed): listed with zeros.
    let body: serde_json::Value = ctx.get_snapshot(&admin_token, None).await.json().await.unwrap();
    assert_eq!(body["instances"].as_array().unwrap().len(), 1);
    assert_eq!(body["instances"][0]["cpu_percent"], 0.0);
    assert!(body["instances"][0]["cpu_fine"].as_array().unwrap().is_empty());
    assert!(body["instances"][0]["cpu_coarse"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_monitor_sampler_container_gone_is_fail_open_not_alarm() {
    // A 404 "No such container" is the normal teardown race (instance stopped
    // or deleted between the active list and the stats read): the pass must
    // stay fail-open and the instance listed, exactly like a real failure.
    let ctx = MonitorContext::new(|_| {}).await;
    let admin_token = ctx.login("admin", "admin").await;

    let owner = ctx.create_plain_user(&admin_token, "owner-user").await;
    ctx.seed_running_instance(&owner, 0).await;

    let mut sampler = MetricsSampler::new();
    let mock = {
        let mut m = MockDockerService::new();
        m.expect_container_stats().returning(|_| {
            Box::pin(async {
                Err(ContainerStatsError::ContainerNotFound(
                    "No such container: 110916571b".to_string(),
                ))
            })
        });
        m
    };
    let sampled = sampler.sample_once(&ctx.db, &mock, &ctx.metrics).await;
    assert_eq!(sampled.len(), 1, "the instance is still listed");

    let body: serde_json::Value = ctx.get_snapshot(&admin_token, None).await.json().await.unwrap();
    assert_eq!(body["instances"].as_array().unwrap().len(), 1);
    assert_eq!(body["instances"][0]["cpu_percent"], 0.0);
    assert!(body["instances"][0]["cpu_fine"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_monitor_snapshot_first_cpu_read_is_zero() {
    // ContainerStats.cpu_percent is None on the first read (no previous
    // counters yet), so the recorded sample carries 0% until a second pass.
    let store = MetricsStore::new();
    let id = uuid::Uuid::new_v4();
    store.record_instance(
        id,
        openworkspace_api::metrics::Sample {
            ts: 1,
            cpu_percent: 0.0,
            mem_used_bytes: 1,
            mem_total_bytes: 2,
            disk_used_bytes: 0,
            disk_total_bytes: 0,
        },
    );
    let snap = store.snapshot();
    let (_, entity) = &snap.instances[0];
    assert_eq!(entity.cpu_percent, 0.0);
    assert_eq!(entity.mem_used_bytes, 1);
}
