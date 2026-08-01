mod common;

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

use common::ensure_pg;
use mockall::predicate::eq;
use openworkspace_api::docker::MockDockerService;
use openworkspace_api::routes::{AppState, api_routes};
use openworkspace_api::db::WorkspaceInstanceRepository;
use openworkspace_api::vnc_cache::VncCache;
use openworkspace_api::core::Settings;
use sea_orm::DatabaseConnection;

static MOCK_COUNTER: AtomicU32 = AtomicU32::new(0);

struct MockContext {
    base_url: String,
    client: reqwest::Client,
    db: DatabaseConnection,
    #[allow(dead_code)]
    db_name: String,
}

fn docker_err(msg: &str) -> bollard::errors::Error {
    std::io::Error::new(std::io::ErrorKind::Other, msg).into()
}

fn docker_404() -> bollard::errors::Error {
    bollard::errors::Error::DockerResponseServerError {
        status_code: 404,
        message: "No such container".to_string(),
    }
}

impl MockContext {
    async fn new<F: FnOnce(&mut MockDockerService)>(setup_mock: F) -> Self {
        ensure_pg().await;
        let counter = MOCK_COUNTER.fetch_add(1, Ordering::Relaxed);
        let db_name = format!("mock_test_{}_{:04}", std::process::id(), counter);
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
        pg_client.execute(&format!("DROP DATABASE IF EXISTS \"{}\"", db_name)[..], &[]).await.unwrap();
        pg_client.execute(&format!("CREATE DATABASE \"{}\"", db_name)[..], &[]).await.unwrap();
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
            docker_network: "ow-test".to_string(),
            container_runtime: "docker".to_string(),
        };

        openworkspace_api::db::UserRepository::new(&db)
            .seed_admin(&settings.admin_password)
            .await
            .unwrap();

        let mut mock_docker = MockDockerService::new();
        mock_docker.expect_network_name()
            .return_const("ow-test".to_string());
        setup_mock(&mut mock_docker);

        let state = AppState {
            db: db.clone(),
            docker: Arc::new(mock_docker),
            vnc_cache: VncCache::new(),
            settings,
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

        MockContext { base_url, client, db, db_name }
    }

    async fn login_admin(&self) -> String {
        let resp = self.client
            .post(format!("{}/api/auth/login", self.base_url))
            .json(&serde_json::json!({"username": "admin", "password": "admin"}))
            .send()
            .await
            .unwrap();
        let cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
        cookie.split(';').next().unwrap().strip_prefix("ow_token=").unwrap().to_string()
    }

    async fn post_auth(&self, path: &str, body: &serde_json::Value, token: &str) -> reqwest::Response {
        self.client
            .post(format!("{}{}", self.base_url, path))
            .header("Cookie", format!("ow_token={}", token))
            .json(body)
            .send()
            .await
            .unwrap()
    }

    async fn get_auth(&self, path: &str, token: &str) -> reqwest::Response {
        self.client
            .get(format!("{}{}", self.base_url, path))
            .header("Cookie", format!("ow_token={}", token))
            .send()
            .await
            .unwrap()
    }

    async fn delete_auth(&self, path: &str, token: &str) -> reqwest::Response {
        self.client
            .delete(format!("{}{}", self.base_url, path))
            .header("Cookie", format!("ow_token={}", token))
            .send()
            .await
            .unwrap()
    }
}

async fn create_config_and_instance(ctx: &MockContext, token: &str, name: &str) -> (String, String) {
    let config_resp = ctx.post_auth("/api/templates", &serde_json::json!({
        "name": name, "image": "busybox:1"
    }), token).await;
    let template_id = config_resp.json::<serde_json::Value>().await.unwrap()["template"]["id"].as_str().unwrap().to_string();

    let launch_resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id
    }), token).await;
    let body = launch_resp.json::<serde_json::Value>().await.unwrap();
    let instance_id = body["instance"]["id"].as_str().unwrap().to_string();
    (template_id, instance_id)
}

async fn set_instance_status(db: &DatabaseConnection, instance_id: &str, status: &str, container_id: Option<&str>) {
    use openworkspace_api::db::workspace_instance;
    use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
    let id: uuid::Uuid = instance_id.parse().unwrap();
    let repo = WorkspaceInstanceRepository::new(db);
    repo.update_status(id, status).await.unwrap();
    let model = workspace_instance::Entity::find()
        .filter(workspace_instance::Column::Id.eq(id))
        .one(db).await.unwrap().unwrap();
    let mut active: workspace_instance::ActiveModel = model.into();
    match container_id {
        Some(cid) => { active.container_id = Set(Some(cid.to_string())); }
        None => { active.container_id = Set(None); }
    }
    active.update(db).await.unwrap();
}

#[tokio::test]
async fn test_launch_docker_create_fails() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Err("Docker create failed".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Err("no ip".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (template_id, _) = create_config_and_instance(&ctx, &token, "launch-create-fail").await;

    let resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id
    }), &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"], "error");
    assert_eq!(body["docker_error"], "Docker create failed");
}

#[tokio::test]
async fn test_start_docker_start_fails() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
        m.expect_inspect_container_state()
            .returning(|_| Box::pin(async { Ok(Some("exited".to_string())) }));
        m.expect_start_container_by_id()
            .returning(|_| Box::pin(async { Err(docker_err("start failed")) }));
    }).await;

    let token = ctx.login_admin().await;
    let (template_id, instance_id) = create_config_and_instance(&ctx, &token, "start-fail").await;
    let _ = template_id;
    set_instance_status(&ctx.db, &instance_id, "stopped", Some("abc123def456")).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({}), &token).await;
    assert_eq!(resp.status(), 500);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "Failed to start container");
}

#[tokio::test]
async fn test_start_docker_create_after_inspect_fails() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .times(1)
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Err("create failed".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
        m.expect_inspect_container_state()
            .returning(|_| Box::pin(async { Err(docker_err("not found")) }));
    }).await;

    let token = ctx.login_admin().await;
    let (template_id, instance_id) = create_config_and_instance(&ctx, &token, "start-recreate-fail").await;
    let _ = template_id;
    set_instance_status(&ctx.db, &instance_id, "stopped", Some("abc123def456")).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({}), &token).await;
    assert_eq!(resp.status(), 500);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "Failed to create container");
}

#[tokio::test]
async fn test_start_docker_create_new_fails() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .times(1)
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Err("new create failed".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "start-new-create-fail").await;
    set_instance_status(&ctx.db, &instance_id, "stopped", None).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({}), &token).await;
    assert_eq!(resp.status(), 500);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "Failed to create container");
}

#[tokio::test]
async fn test_pause_docker_pause_fails() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
        m.expect_pause_container_by_id()
            .returning(|_| Box::pin(async { Err(docker_err("pause failed")) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "pause-fail").await;
    set_instance_status(&ctx.db, &instance_id, "running", Some("abc123def456")).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/pause", instance_id), &serde_json::json!({}), &token).await;
    assert_eq!(resp.status(), 500);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "Failed to pause container");
}

#[tokio::test]
async fn test_unpause_docker_unpause_fails() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
        m.expect_unpause_container_by_id()
            .returning(|_| Box::pin(async { Err(docker_err("unpause failed")) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "unpause-fail").await;
    set_instance_status(&ctx.db, &instance_id, "paused", Some("abc123def456")).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/unpause", instance_id), &serde_json::json!({}), &token).await;
    assert_eq!(resp.status(), 500);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "Failed to resume container");
}

// ── Test 7: launch_instance — success path (lines 169-193) ──

#[tokio::test]
async fn test_launch_success() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("real-container-id-12345678".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.5".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (config_id, _) = create_config_and_instance(&ctx, &token, "launch-success").await;

    let resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": config_id
    }), &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"], "starting");
    assert_eq!(body["instance"]["container_id"], "real-container-id-12345678");
}

// ── Test 8: stop_instance — paused instance triggers unpause before stop (lines 440-443) ──

#[tokio::test]
async fn test_stop_paused_instance_unpauses_first() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
        m.expect_unpause_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
        m.expect_stop_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "stop-paused").await;
    set_instance_status(&ctx.db, &instance_id, "paused", Some("abc123def456")).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({}), &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "stopped");
}

// ── Test 9: stop_instance — stop_container_by_id error (lines 447-453) ──

#[tokio::test]
async fn test_stop_container_error_still_updates_db() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
        m.expect_stop_container_by_id()
            .returning(|_| Box::pin(async { Err(docker_err("stop failed")) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "stop-err").await;
    set_instance_status(&ctx.db, &instance_id, "running", Some("abc123def456")).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({}), &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "stopped");
}

// ── Test 10: delete_instance — container present, remove_container fails (lines 255-260) ──

#[tokio::test]
async fn test_delete_container_remove_fails() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
        m.expect_stop_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
        m.expect_remove_container_by_id()
            .returning(|_| Box::pin(async { Err(docker_err("remove failed")) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "delete-remove-fail").await;

    let resp = ctx.delete_auth(&format!("/api/instances/{}", instance_id), &token).await;
    assert_eq!(resp.status(), 204);
}

// ── Test 11: start_instance — container already running (line 304-305) ──

#[tokio::test]
async fn test_start_container_already_running() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
        m.expect_inspect_container_state()
            .returning(|_| Box::pin(async { Ok(Some("running".to_string())) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "start-already-running").await;
    set_instance_status(&ctx.db, &instance_id, "stopped", Some("abc123def456")).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({}), &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "starting");
}

// ── Test 12: start_instance — inspect fails, config found, recreates (lines 318-340) ──

#[tokio::test]
async fn test_start_inspect_fails_recreates_container() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
        m.expect_inspect_container_state()
            .returning(|_| Box::pin(async { Err(docker_err("not found")) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "start-inspect-fail").await;
    set_instance_status(&ctx.db, &instance_id, "stopped", Some("abc123def456")).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({}), &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "starting");
}

// ── Test 13: pause_instance — success (lines 509-515) ──

#[tokio::test]
async fn test_pause_success() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
        m.expect_pause_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "pause-success").await;
    set_instance_status(&ctx.db, &instance_id, "running", Some("abc123def456")).await;
    use openworkspace_api::db::WorkspaceInstanceRepository;
    let id: uuid::Uuid = instance_id.parse().unwrap();
    WorkspaceInstanceRepository::new(&ctx.db).update_started_at(id, Some(chrono::Utc::now())).await.unwrap();

    let resp = ctx.post_auth(&format!("/api/instances/{}/pause", instance_id), &serde_json::json!({}), &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "paused");

    let found = WorkspaceInstanceRepository::new(&ctx.db).find_by_id(id).await.unwrap().unwrap();
    assert!(found.started_at.is_none());
}

// ── Test 14: unpause_instance — success (lines 564-570) ──

#[tokio::test]
async fn test_unpause_success() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
        m.expect_unpause_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "unpause-success").await;
    set_instance_status(&ctx.db, &instance_id, "paused", Some("abc123def456")).await;
    use openworkspace_api::db::WorkspaceInstanceRepository;
    let id: uuid::Uuid = instance_id.parse().unwrap();
    WorkspaceInstanceRepository::new(&ctx.db).update_started_at(id, None).await.unwrap();

    let resp = ctx.post_auth(&format!("/api/instances/{}/unpause", instance_id), &serde_json::json!({}), &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "running");

    let found = WorkspaceInstanceRepository::new(&ctx.db).find_by_id(id).await.unwrap().unwrap();
    assert!(found.started_at.is_some());
}

#[tokio::test]
async fn test_stop_clears_started_at() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
        m.expect_stop_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "stop-clears-started-at").await;
    set_instance_status(&ctx.db, &instance_id, "running", Some("abc123def456")).await;
    use openworkspace_api::db::WorkspaceInstanceRepository;
    let id: uuid::Uuid = instance_id.parse().unwrap();
    WorkspaceInstanceRepository::new(&ctx.db).update_started_at(id, Some(chrono::Utc::now())).await.unwrap();

    let resp = ctx.post_auth(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({}), &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "stopped");

    let found = WorkspaceInstanceRepository::new(&ctx.db).find_by_id(id).await.unwrap().unwrap();
    assert!(found.started_at.is_none());
}

// ── Test 15: start_instance — start with container_id, start succeeds (lines 306-316) ──

#[tokio::test]
async fn test_start_stopped_container_success() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
        m.expect_inspect_container_state()
            .returning(|_| Box::pin(async { Ok(Some("exited".to_string())) }));
        m.expect_start_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
        m.expect_apply_bandwidth_limit()
            .never();
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "start-stopped-ok").await;
    set_instance_status(&ctx.db, &instance_id, "stopped", Some("abc123def456")).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({}), &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "starting");
}

// ── Bandwidth limit: applied on restart of an existing stopped container ──

async fn create_bw_template(ctx: &MockContext, token: &str, name: &str, up_mbps: i32, down_mbps: i32) -> String {
    let resp = ctx.post_auth("/api/templates", &serde_json::json!({
        "name": name,
        "image": "busybox:1",
        "network_bandwidth_up_mbps": up_mbps,
        "network_bandwidth_down_mbps": down_mbps
    }), token).await;
    resp.json::<serde_json::Value>().await.unwrap()["template"]["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_start_stopped_container_applies_bandwidth() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
        m.expect_inspect_container_state()
            .returning(|_| Box::pin(async { Ok(Some("exited".to_string())) }));
        m.expect_start_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
        m.expect_apply_bandwidth_limit()
            .with(eq("abc123def456"), eq(10), eq(20))
            .returning(|_, _, _| Box::pin(async { Ok(()) }));
    }).await;

    let token = ctx.login_admin().await;
    let template_id = create_bw_template(&ctx, &token, "start-bw-ok", 10, 20).await;
    let launch_resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id
    }), &token).await;
    let body = launch_resp.json::<serde_json::Value>().await.unwrap();
    let instance_id = body["instance"]["id"].as_str().unwrap().to_string();
    set_instance_status(&ctx.db, &instance_id, "stopped", Some("abc123def456")).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({}), &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "starting");
}

// ── Test 16: start_instance — container missing, recreate succeeds (lines 318-340) ──

#[tokio::test]
async fn test_start_recreate_success() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .times(1)
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("new-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
        m.expect_inspect_container_state()
            .returning(|_| Box::pin(async { Err(docker_err("not found")) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "start-recreate-ok").await;
    set_instance_status(&ctx.db, &instance_id, "stopped", Some("abc123def456")).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({}), &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "starting");
}

// ── Test 17: launch_instance — get_container_ip fails, still returns running (line 180) ──

#[tokio::test]
async fn test_launch_get_ip_fails_still_succeeds() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("real-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Err("no ip available".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (config_id, _) = create_config_and_instance(&ctx, &token, "launch-ip-fail").await;

    let resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": config_id
    }), &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"], "starting");
}

// ── Test 18: delete_instance — no container, just deletes (lines 263-264) ──

#[tokio::test]
async fn test_delete_no_container() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "delete-no-container").await;
    set_instance_status(&ctx.db, &instance_id, "stopped", None).await;

    let resp = ctx.delete_auth(&format!("/api/instances/{}", instance_id), &token).await;
    assert_eq!(resp.status(), 204);
}

// ── Test 19: stop_instance — no container, just updates DB (lines 433-438, 462-468) ──

#[tokio::test]
async fn test_stop_no_container() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "stop-no-container").await;
    set_instance_status(&ctx.db, &instance_id, "running", None).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({}), &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "stopped");
}

// ── Test 20: list_instances (lines 54-101) ──

#[tokio::test]
async fn test_list_instances() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    create_config_and_instance(&ctx, &token, "list-test").await;

    let resp = ctx.get_auth("/api/instances", &token).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(body["instances"].as_array().unwrap().len() >= 1);
}

// ── Test 21: get_instance (lines 210-234) ──

#[tokio::test]
async fn test_get_instance() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "get-test").await;

    let resp = ctx.get_auth(&format!("/api/instances/{}", instance_id), &token).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["id"], instance_id);
    assert_eq!(body["instance"]["started_at"], serde_json::Value::Null, "started_at should be emitted in instance JSON (null for fresh instance)");
}

// ── Test 22: start_instance — already running conflict (lines 293-297) ──

#[tokio::test]
async fn test_start_already_running_conflict() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "start-rc-conflict").await;
    set_instance_status(&ctx.db, &instance_id, "running", Some("fake-container-id")).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({}), &token).await;
    assert_eq!(resp.status(), 409);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "Instance is already running");
}

// ── Test 23: stop_instance — already stopped conflict (lines 433-437) ──

#[tokio::test]
async fn test_stop_already_stopped_conflict() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "stop-sc-conflict").await;
    set_instance_status(&ctx.db, &instance_id, "stopped", Some("fake-container-id")).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({}), &token).await;
    assert_eq!(resp.status(), 409);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "Instance is already stopped");
}

// ── Test 24: pause_instance — not running conflict (line 501) ──

#[tokio::test]
async fn test_pause_not_running_conflict() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "pause-nr-conflict").await;
    set_instance_status(&ctx.db, &instance_id, "stopped", Some("fake-container-id")).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/pause", instance_id), &serde_json::json!({}), &token).await;
    assert_eq!(resp.status(), 409);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "Instance must be running to pause");
}

// ── Test 25: unpause_instance — not paused conflict (line 556) ──

#[tokio::test]
async fn test_unpause_not_paused_conflict() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "unpause-np-conflict").await;
    set_instance_status(&ctx.db, &instance_id, "running", Some("fake-container-id")).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/unpause", instance_id), &serde_json::json!({}), &token).await;
    assert_eq!(resp.status(), 409);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "Instance must be paused to resume");
}

// ── Test 26: start_instance — no container_id, creates new (lines 350-372) ──

#[tokio::test]
async fn test_start_no_container_id_success() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("brand-new-container".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.9".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "start-no-cid").await;
    set_instance_status(&ctx.db, &instance_id, "stopped", None).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({}), &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "starting");
}

// ── Test 27: delete_instance — container present, success (lines 255-267) ──

#[tokio::test]
async fn test_delete_with_container_success() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
        m.expect_stop_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
        m.expect_remove_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "delete-ok").await;

    let resp = ctx.delete_auth(&format!("/api/instances/{}", instance_id), &token).await;
    assert_eq!(resp.status(), 204);
}

// ── Test 27b: delete_instance — container already gone (404 on stop and remove) ──

#[tokio::test]
async fn test_delete_with_container_already_removed() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
        m.expect_stop_container_by_id()
            .returning(|_| Box::pin(async { Err(docker_404()) }));
        m.expect_remove_container_by_id()
            .returning(|_| Box::pin(async { Err(docker_404()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "delete-404").await;

    let resp = ctx.delete_auth(&format!("/api/instances/{}", instance_id), &token).await;
    assert_eq!(resp.status(), 204);
}

// ── Test 28: pause_instance — no container_id conflict (lines 504-506) ──

#[tokio::test]
async fn test_pause_no_container_conflict() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "pause-no-cid").await;
    set_instance_status(&ctx.db, &instance_id, "running", None).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/pause", instance_id), &serde_json::json!({}), &token).await;
    assert_eq!(resp.status(), 409);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "No container attached");
}

// ── Test 29: unpause_instance — no container_id conflict (lines 559-561) ──

#[tokio::test]
async fn test_unpause_no_container_conflict() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "unpause-no-cid").await;
    set_instance_status(&ctx.db, &instance_id, "paused", None).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/unpause", instance_id), &serde_json::json!({}), &token).await;
    assert_eq!(resp.status(), 409);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "No container attached");
}

#[tokio::test]
async fn test_launch_instance_forwards_runsc_runtime() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .withf(|_, _, config, _, _| config.runtime == Some("runsc".to_string()))
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;

    let config_resp = ctx.post_auth("/api/templates", &serde_json::json!({
        "name": "runsc-forward",
        "image": "busybox:1",
        "container_runtime": "runsc"
    }), &token).await;
    let template_id = config_resp.json::<serde_json::Value>().await.unwrap()["template"]["id"].as_str().unwrap().to_string();

    let resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id
    }), &token).await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_launch_instance_defaults_to_settings_runtime() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .withf(|_, _, config, _, _| config.runtime == Some("docker".to_string()))
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_get_container_ip()
            .returning(|_, _| Box::pin(async { Ok("172.17.0.2".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;

    let config_resp = ctx.post_auth("/api/templates", &serde_json::json!({
        "name": "default-runtime-forward",
        "image": "busybox:1"
    }), &token).await;
    let template_id = config_resp.json::<serde_json::Value>().await.unwrap()["template"]["id"].as_str().unwrap().to_string();

    let resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id
    }), &token).await;
    assert_eq!(resp.status(), 200);
}
