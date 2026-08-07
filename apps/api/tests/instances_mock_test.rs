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
use chrono::TimeZone;

static MOCK_COUNTER: AtomicU32 = AtomicU32::new(0);

struct MockContext {
    base_url: String,
    client: reqwest::Client,
    db: DatabaseConnection,
    #[allow(dead_code)]
    db_name: String,
}

fn docker_err(msg: &str) -> bollard::errors::Error {
    std::io::Error::other(msg).into()
}

fn docker_404() -> bollard::errors::Error {
    bollard::errors::Error::DockerResponseServerError {
        status_code: 404,
        message: "No such container".to_string(),
    }
}

/// The launch path lists existing Docker networks and creates the instance's
/// dedicated `/30` network before the container. Registered as a fallback after
/// each test's own expectations — mockall matches expectations FIFO, so a test
/// that declares its own `list_networks`/`create_network` expectations shadows
/// these. Default: an empty used-subnet set (no pre-existing network collides)
/// and an idempotent successful create. Accepts any call count, so tests that
/// fail before the network step (volume-prep) are unaffected.
fn mock_instance_network(m: &mut MockDockerService) {
    m.expect_list_networks()
        .returning(|| Box::pin(async { Ok(Vec::new()) }));
    m.expect_create_network()
        .returning(|_, _, _| Box::pin(async { Ok(()) }));
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
            container_runtime: "runc".to_string(),
            host_gateway_ip: "172.17.0.1".to_string(),
            host_port_start: 10000,
            host_port_end: 20000,
            instance_net_base: "10.200.0.0/16".to_string(),
            instance_dns: "8.8.8.8,1.1.1.1".to_string(),
            port_lock_dir: String::new(),
        };

        openworkspace_api::db::UserRepository::new(&db)
            .seed_admin(&settings.admin_password)
            .await
            .unwrap();

        let mut mock_docker = MockDockerService::new();
        setup_mock(&mut mock_docker);
        mock_instance_network(&mut mock_docker);

        let state = AppState {
            db: db.clone(),
            docker: Arc::new(mock_docker),
            vnc_cache: VncCache::new(),
            settings,
            metrics: Arc::new(openworkspace_api::metrics::MetricsStore::new()),
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

    async fn login_user(&self, username: &str, password: &str) -> String {
        let resp = self.client
            .post(format!("{}/api/auth/login", self.base_url))
            .json(&serde_json::json!({"username": username, "password": password}))
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

    async fn put_auth(&self, path: &str, body: &serde_json::Value, token: &str) -> reqwest::Response {
        self.client
            .put(format!("{}{}", self.base_url, path))
            .header("Cookie", format!("ow_token={}", token))
            .json(body)
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

/// Directly assert that the flock reservation on `port` is no longer held —
/// i.e. the API released it (RAII drop) — rather than relying on a second
/// launch re-picking the same port. The registry is shared across parallel
/// test binaries, so a transient foreign hold is absorbed by a bounded retry
/// instead of flaking the exact-reuse assertion.
fn assert_port_released(port: u16) {
    let dir = openworkspace_api::host_port::resolve_lock_dir("")
        .expect("test suite must resolve a shared lock directory");
    for _ in 0..40 {
        if let Some(_lock) = openworkspace_api::host_port::acquire_lock(&dir, &port.to_string()) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("host port {} was not released within the grace window", port);
}

/// Same as `assert_port_released`, but for a `/30` subnet block's lockfile —
/// proves a successful launch dropped its subnet reservation (RAII).
fn assert_subnet_released(subnet: &str) {
    let dir = openworkspace_api::host_port::resolve_lock_dir("")
        .expect("test suite must resolve a shared lock directory");
    for _ in 0..40 {
        if let Some(_lock) = openworkspace_api::host_port::acquire_lock(&dir, subnet) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("instance subnet {} was not released within the grace window", subnet);
}

#[tokio::test]
async fn test_launch_docker_create_fails() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Err("Docker create failed".to_string()) }));
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

// ── Ticket 01: a launch that fails at container-create must release its flock
//    reservation — the next launch can take the exact same port again. ──

#[tokio::test]
async fn test_launch_failure_releases_port_reservation() {
    let attempted_ports = Arc::new(std::sync::Mutex::new(Vec::new()));
    let calls = Arc::new(AtomicU32::new(0));
    let attempted_for_mock = attempted_ports.clone();
    let calls_for_mock = calls.clone();

    let ctx = MockContext::new(move |m| {
        m.expect_create_container_from_template()
            .returning(move |_, _, config, _, _| {
                let ports = attempted_for_mock.clone();
                let calls = calls_for_mock.clone();
                let host_port = config.host_port.unwrap_or(0);
                Box::pin(async move {
                    ports.lock().unwrap().push(host_port);
                    if calls.fetch_add(1, Ordering::Relaxed) == 0 {
                        Err("Docker create failed".to_string())
                    } else {
                        Ok("fake-container-id-2".to_string())
                    }
                })
            });
    }).await;

    let token = ctx.login_admin().await;
    let template_a = create_template_only(&ctx, &token, "release-port-a").await;
    let template_b = create_template_only(&ctx, &token, "release-port-b").await;

    let first = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_a
    }), &token).await;
    let first_body: serde_json::Value = first.json().await.unwrap();
    assert_eq!(first_body["instance"]["status"], "error");

    let second = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_b
    }), &token).await;
    let second_body: serde_json::Value = second.json().await.unwrap();
    assert_eq!(second_body["instance"]["status"], "starting");

    let attempted = attempted_ports.lock().unwrap().clone();
    assert_eq!(attempted.len(), 2, "two launches, two create attempts");
    assert!(attempted[0] > 0, "first launch must have attempted a real port");
    assert_port_released(attempted[0]);
}

#[tokio::test]
async fn test_launch_retries_on_port_conflict_with_new_port() {
    let attempted_ports = Arc::new(std::sync::Mutex::new(Vec::new()));
    let calls = Arc::new(AtomicU32::new(0));
    let attempted_ports_for_mock = attempted_ports.clone();
    let calls_for_mock = calls.clone();

    let ctx = MockContext::new(move |m| {
        m.expect_create_container_from_template()
            .returning(move |_, _, config, _, _| {
                let ports_for_mock = attempted_ports_for_mock.clone();
                let calls_for_mock = calls_for_mock.clone();
                let host_port = config.host_port.unwrap_or(0);
                Box::pin(async move {
                    ports_for_mock.lock().unwrap().push(host_port);
                    if calls_for_mock.fetch_add(1, Ordering::Relaxed) == 0 {
                        Err("Bind for 172.17.0.1:10000 failed: port is already allocated".to_string())
                    } else {
                        Ok("fake-container-id".to_string())
                    }
                })
            });
    }).await;

    let token = ctx.login_admin().await;
    let (template_id, instance_id) = create_config_and_instance(&ctx, &token, "port-conflict-retry").await;
    let _ = template_id;

    let resp = ctx.get_auth(&format!("/api/instances/{}", instance_id), &token).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"], "starting");

    let attempted = attempted_ports.lock().unwrap().clone();
    assert_eq!(attempted.len(), 2, "conflict should trigger exactly one retry");
    assert_ne!(attempted[0], attempted[1], "retry must pick a different host port");
    assert_eq!(body["instance"]["host_port"], attempted[1]);
}

#[tokio::test]
async fn test_launch_creates_network_before_container_with_ow_dns() {
    let order = Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured = Arc::new(std::sync::Mutex::new(None::<(Option<String>, Option<String>)>));
    let order_for_mock = order.clone();
    let captured_for_mock = captured.clone();

    let ctx = MockContext::new(move |m| {
        let order = order_for_mock.clone();
        m.expect_list_networks()
            .times(1)
            .returning(move || {
                order.lock().unwrap().push("list_networks".to_string());
                Box::pin(async { Ok(Vec::new()) })
            });
        let order = order_for_mock.clone();
        m.expect_create_network()
            .times(1)
            .returning(move |name, subnet, gateway| {
                order.lock().unwrap().push(format!("create_network|{}|{}|{}", name, subnet, gateway));
                Box::pin(async { Ok(()) })
            });
        let order = order_for_mock.clone();
        let captured = captured_for_mock.clone();
        m.expect_create_container_from_template()
            .returning(move |_, _, config, _, _| {
                order.lock().unwrap().push("create_container".to_string());
                *captured.lock().unwrap() = Some((config.network_name.clone(), config.instance_dns.clone()));
                Box::pin(async { Ok("fake-container-id".to_string()) })
            });
    }).await;

    let token = ctx.login_admin().await;
    let (template_id, instance_id) = create_config_and_instance(&ctx, &token, "launch-net-order").await;
    let _ = template_id;

    let expected_net = format!("ow-{}", instance_id);
    let order = order.lock().unwrap();
    assert_eq!(order.len(), 3);
    assert_eq!(order[0], "list_networks");
    // The lowest free block is only deterministic in the unit tests (isolated
    // lock dir). In this shared registry a parallel real-Docker test binary may
    // transiently hold `10.200.0.0`'s flock, so assert an aligned `/30` inside
    // the base range with the gateway one address up instead of the exact block.
    let parts: Vec<&str> = order[1].split('|').collect();
    assert_eq!(parts[0], "create_network");
    assert_eq!(parts[1], expected_net.as_str());
    let subnet_addr: std::net::Ipv4Addr = parts[2].split('/').next().unwrap().parse().unwrap();
    let octets = subnet_addr.octets();
    assert_eq!(octets[0..2], [10, 200], "subnet must stay inside the base range");
    assert_eq!(octets[3] % 4, 0, "subnet must be an aligned /30 block");
    assert_eq!(parts[2], format!("{}/30", subnet_addr));
    let gateway = std::net::Ipv4Addr::new(octets[0], octets[1], octets[2], octets[3] + 1);
    assert_eq!(parts[3], gateway.to_string());
    assert_eq!(order[2], "create_container");

    let captured = captured.lock().unwrap();
    let (network_name, instance_dns) = captured.as_ref().expect("container config must be captured");
    assert_eq!(network_name.as_deref(), Some(expected_net.as_str()));
    assert_eq!(instance_dns.as_deref(), Some("8.8.8.8,1.1.1.1"));
}

#[tokio::test]
async fn test_launch_response_exposes_network_name() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (template_id, instance_id) = create_config_and_instance(&ctx, &token, "launch-net-json").await;
    let expected_net = format!("ow-{}", instance_id);

    let resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id
    }), &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let second_id = body["instance"]["id"].as_str().unwrap().to_string();
    assert_eq!(body["instance"]["network_name"], format!("ow-{}", second_id));

    let resp = ctx.get_auth(&format!("/api/instances/{}", instance_id), &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["network_name"], expected_net);

    let resp = ctx.get_auth("/api/instances", &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let found = body["instances"].as_array().unwrap().iter()
        .find(|i| i["id"] == serde_json::Value::String(instance_id.clone())).unwrap();
    assert_eq!(found["network_name"], expected_net);
}

#[tokio::test]
async fn test_launch_port_conflict_retry_reuses_same_network() {
    let network_calls = Arc::new(std::sync::Mutex::new(Vec::new()));
    let calls = Arc::new(AtomicU32::new(0));
    let network_calls_for_mock = network_calls.clone();
    let calls_for_mock = calls.clone();

    let ctx = MockContext::new(move |m| {
        m.expect_list_networks()
            .times(1)
            .returning(|| Box::pin(async { Ok(Vec::new()) }));
        let names = network_calls_for_mock.clone();
        m.expect_create_network()
            .times(1)
            .returning(move |name, _, _| {
                names.lock().unwrap().push(format!("create_network|{}", name));
                Box::pin(async { Ok(()) })
            });
        let names = network_calls_for_mock.clone();
        let calls_for_mock = calls_for_mock.clone();
        m.expect_create_container_from_template()
            .returning(move |_, _, config, _, _| {
                names.lock().unwrap().push(format!("create_container|{}", config.network_name.as_deref().unwrap_or("")));
                let is_first = calls_for_mock.fetch_add(1, Ordering::Relaxed) == 0;
                Box::pin(async move {
                    if is_first {
                        Err("Bind for 172.17.0.1:10000 failed: port is already allocated".to_string())
                    } else {
                        Ok("fake-container-id".to_string())
                    }
                })
            });
    }).await;

    let token = ctx.login_admin().await;
    let (template_id, instance_id) = create_config_and_instance(&ctx, &token, "retry-same-net").await;
    let _ = template_id;

    let resp = ctx.get_auth(&format!("/api/instances/{}", instance_id), &token).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"], "starting");

    let expected_net = format!("ow-{}", instance_id);
    let names = network_calls.lock().unwrap();
    assert_eq!(names.len(), 3, "one create_network plus two container creates");
    assert_eq!(names[0], format!("create_network|{}", expected_net));
    assert_eq!(names[1], format!("create_container|{}", expected_net));
    assert_eq!(names[2], format!("create_container|{}", expected_net));
}

#[tokio::test]
async fn test_launch_subnet_pool_exhausted_marks_error() {
    let ctx = MockContext::new(|m| {
        m.expect_list_networks()
            .times(1)
            .returning(|| {
                use openworkspace_api::docker::NetworkInfo;
                let base = u32::from(std::net::Ipv4Addr::new(10, 200, 0, 0));
                let subnets = (0..(1u32 << 14))
                    .map(|i| NetworkInfo {
                        name: format!("taken-{}", i),
                        subnet: Some(format!("{}/30", std::net::Ipv4Addr::from(base + i * 4))),
                    })
                    .collect();
                Box::pin(async move { Ok(subnets) })
            });
        m.expect_create_network().never();
        m.expect_create_container_from_template().never();
    }).await;

    let token = ctx.login_admin().await;
    let config_resp = ctx.post_auth("/api/templates", &serde_json::json!({
        "name": "net-exhausted", "image": "busybox:1"
    }), &token).await;
    let template_id = config_resp.json::<serde_json::Value>().await.unwrap()["template"]["id"].as_str().unwrap().to_string();

    let resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id
    }), &token).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"], "error");
    assert!(body["docker_error"].as_str().unwrap().contains("subnet pool exhausted"));
}

#[tokio::test]
async fn test_launch_network_create_failure_marks_error() {
    let ctx = MockContext::new(|m| {
        m.expect_create_network()
            .times(1)
            .returning(|_, _, _| Box::pin(async { Err("Failed to create network 'ow-x': boom".to_string()) }));
        m.expect_create_container_from_template().never();
    }).await;

    let token = ctx.login_admin().await;
    let config_resp = ctx.post_auth("/api/templates", &serde_json::json!({
        "name": "net-create-fail", "image": "busybox:1"
    }), &token).await;
    let template_id = config_resp.json::<serde_json::Value>().await.unwrap()["template"]["id"].as_str().unwrap().to_string();

    let resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id
    }), &token).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"], "error");
    assert!(body["docker_error"].as_str().unwrap().contains("Failed to create network"));
}

#[tokio::test]
async fn test_launch_list_networks_failure_marks_error() {
    let ctx = MockContext::new(|m| {
        m.expect_list_networks()
            .times(1)
            .returning(|| Box::pin(async { Err("Failed to list networks: boom".to_string()) }));
        m.expect_create_network().never();
        m.expect_create_container_from_template().never();
    }).await;

    let token = ctx.login_admin().await;
    let config_resp = ctx.post_auth("/api/templates", &serde_json::json!({
        "name": "net-list-fail", "image": "busybox:1"
    }), &token).await;
    let template_id = config_resp.json::<serde_json::Value>().await.unwrap()["template"]["id"].as_str().unwrap().to_string();

    let resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id
    }), &token).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"], "error");
    assert!(body["docker_error"].as_str().unwrap().contains("Failed to list Docker networks"));
}

#[tokio::test]
async fn test_launch_network_overlap_reallocates_subnet() {
    use openworkspace_api::docker::NetworkInfo;

    let list_calls = Arc::new(AtomicU32::new(0));
    let create_calls = Arc::new(AtomicU32::new(0));
    let created_networks = Arc::new(std::sync::Mutex::new(Vec::new()));
    let captured_network = Arc::new(std::sync::Mutex::new(None::<String>));
    // Subnet the first create attempt tried — the "concurrent launch's network"
    // that a re-list must reveal (mirrors real Docker: the overlap error only
    // happened because that block got committed by a peer).
    let first_attempt = Arc::new(std::sync::Mutex::new(None::<String>));
    let list_calls_for_mock = list_calls.clone();
    let create_calls_for_mock = create_calls.clone();
    let created_networks_for_mock = created_networks.clone();
    let captured_network_for_mock = captured_network.clone();
    let first_attempt_for_mock = first_attempt.clone();

    let ctx = MockContext::new(move |m| {
        let list_calls = list_calls_for_mock.clone();
        let first_attempt_for_list = first_attempt_for_mock.clone();
        m.expect_list_networks()
            .returning(move || {
                let call = list_calls.fetch_add(1, Ordering::Relaxed);
                if call == 0 {
                    // Both launches see the same empty snapshot.
                    Box::pin(async { Ok(Vec::new()) })
                } else {
                    // After the overlap, the concurrent launch's network (the
                    // subnet the first attempt actually collided on) is visible.
                    let first_attempt = first_attempt_for_list.clone();
                    Box::pin(async move {
                        let subnet = first_attempt.lock().unwrap().clone()
                            .expect("first create attempt must have run");
                        Ok(vec![NetworkInfo {
                            name: "ow-other".to_string(),
                            subnet: Some(subnet),
                        }])
                    })
                }
            });
        let create_calls = create_calls_for_mock.clone();
        let created_networks = created_networks_for_mock.clone();
        let first_attempt = first_attempt_for_mock.clone();
        m.expect_create_network()
            .returning(move |name, subnet, gateway| {
                let call = create_calls.fetch_add(1, Ordering::Relaxed);
                created_networks.lock().unwrap().push(format!("{}|{}|{}", name, subnet, gateway));
                if call == 0 {
                    *first_attempt.lock().unwrap() = Some(subnet.to_string());
                    Box::pin(async { Err("invalid pool request: Pool overlaps with other one on this address space".to_string()) })
                } else {
                    Box::pin(async { Ok(()) })
                }
            });
        let captured_network = captured_network_for_mock.clone();
        m.expect_create_container_from_template()
            .returning(move |_, _, config, _, _| {
                *captured_network.lock().unwrap() = config.network_name.clone();
                Box::pin(async { Ok("fake-container-id".to_string()) })
            });
    }).await;

    let token = ctx.login_admin().await;
    let (template_id, instance_id) = create_config_and_instance(&ctx, &token, "net-overlap").await;
    let _ = template_id;
    let expected_net = format!("ow-{}", instance_id);

    let resp = ctx.get_auth(&format!("/api/instances/{}", instance_id), &token).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"], "starting");

    let created = created_networks.lock().unwrap();
    assert_eq!(created.len(), 2, "overlap must trigger exactly one re-allocation");
    let first: Vec<&str> = created[0].split('|').collect();
    let second: Vec<&str> = created[1].split('|').collect();
    assert_eq!(first[0], expected_net);
    assert_eq!(second[0], expected_net, "retry must reuse the same network name");
    let parse_subnet = |spec: &str| {
        let addr: std::net::Ipv4Addr = spec.split('/').next().unwrap().parse().unwrap();
        addr.octets()
    };
    let first_octets = parse_subnet(first[1]);
    assert_eq!(first_octets[0..2], [10, 200], "first attempt stays inside the base range");
    assert_eq!(first_octets[3] % 4, 0, "first attempt is an aligned /30 block");
    assert_eq!(first[2], format!("{}.{}.{}.{}", first_octets[0], first_octets[1], first_octets[2], first_octets[3] + 1));
    assert_ne!(second[1], first[1], "re-allocation must pick a different subnet");
    let second_octets = parse_subnet(second[1]);
    assert_eq!(second_octets[0..2], [10, 200], "retry must stay inside the base range");
    assert_eq!(second_octets[3] % 4, 0, "retry must pick an aligned /30 block");
    assert_eq!(captured_network.lock().unwrap().as_deref(), Some(expected_net.as_str()));
}

// ── Ticket 01 (Seam 2): flock arbitration of /30 subnets through the real HTTP
//    stack. Both launches see the same empty snapshot, yet the flock forces
//    distinct blocks even under overlap. ──

#[tokio::test]
async fn test_concurrent_launches_allocate_distinct_subnets() {
    use openworkspace_api::docker::NetworkInfo;

    let created = Arc::new(std::sync::Mutex::new(Vec::new()));
    let gate = Arc::new(tokio::sync::Barrier::new(2));
    let created_for_mock = created.clone();
    let gate_for_mock = gate.clone();

    let ctx = MockContext::new(move |m| {
        m.expect_list_networks()
            .returning(|| Box::pin(async { Ok(Vec::<NetworkInfo>::new()) }));
        m.expect_create_network()
            .returning(move |_, subnet, _| {
                let created = created_for_mock.clone();
                let gate = gate_for_mock.clone();
                let subnet = subnet.to_string();
                Box::pin(async move {
                    created.lock().unwrap().push(subnet);
                    // Hold the reservation until the sibling launch has also
                    // arrived, so its allocator must skip our block even though
                    // both launched from the same empty snapshot.
                    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), gate.wait()).await;
                    Ok(())
                })
            });
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let admin_token = ctx.login_admin().await;
    let template_id = create_template_only(&ctx, &admin_token, "conc-subnet").await;
    for (username, ceiling) in [("subnet_x", 5), ("subnet_y", 5)] {
        let user_id = create_quota_user(&ctx, &admin_token, username, ceiling).await;
        grant_template_whitelist(&ctx, &user_id, &template_id).await;
    }
    let token_x = ctx.login_user("subnet_x", "password123").await;
    let token_y = ctx.login_user("subnet_y", "password123").await;

    let client = ctx.client.clone();
    let url = format!("{}/api/instances", ctx.base_url);
    let body = serde_json::json!({ "template_id": template_id });
    let mut handles = Vec::new();
    for token in [token_x, token_y] {
        let client = client.clone();
        let url = url.clone();
        let body = body.clone();
        handles.push(tokio::spawn(async move {
            client
                .post(&url)
                .header("Cookie", format!("ow_token={}", token))
                .json(&body)
                .send()
                .await
                .unwrap()
                .status()
        }));
    }
    for handle in handles {
        assert_eq!(handle.await.unwrap(), reqwest::StatusCode::OK);
    }

    let created = created.lock().unwrap();
    assert_eq!(created.len(), 2, "both concurrent launches must create a network");
    assert_ne!(created[0], created[1], "concurrent launches must allocate distinct /30 subnets");
    for subnet in created.iter() {
        let addr: std::net::Ipv4Addr = subnet.split('/').next().unwrap().parse().unwrap();
        let octets = addr.octets();
        assert_eq!(octets[0..2], [10, 200], "subnet must stay inside the base range");
        assert_eq!(octets[3] % 4, 0, "subnet must be an aligned /30 block");
    }
}

#[tokio::test]
async fn test_launch_releases_subnet_reservation_on_success() {
    let used_subnet = Arc::new(std::sync::Mutex::new(None::<String>));
    let used_for_mock = used_subnet.clone();
    let ctx = MockContext::new(move |m| {
        m.expect_create_network()
            .returning(move |_, subnet, _| {
                *used_for_mock.lock().unwrap() = Some(subnet.split('/').next().unwrap().to_string());
                Box::pin(async { Ok(()) })
            });
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    create_config_and_instance(&ctx, &token, "release-subnet").await;

    let subnet = used_subnet.lock().unwrap().clone().expect("launch must allocate a subnet");
    assert_subnet_released(&subnet);
}

#[tokio::test]
async fn test_start_reuses_existing_network_without_allocation() {
    use openworkspace_api::docker::NetworkInfo;

    let existing = Arc::new(std::sync::Mutex::new(None::<String>));
    let create_calls = Arc::new(AtomicU32::new(0));
    let existing_for_mock = existing.clone();
    let create_calls_for_mock = create_calls.clone();

    let ctx = MockContext::new(move |m| {
        m.expect_list_networks()
            .returning(move || {
                let existing = existing_for_mock.clone();
                Box::pin(async move {
                    match existing.lock().unwrap().clone() {
                        Some(name) => Ok(vec![NetworkInfo { name, subnet: Some("10.200.0.0/30".to_string()) }]),
                        None => Ok(Vec::new()),
                    }
                })
            });
        m.expect_create_network()
            .returning(move |_, _, _| {
                create_calls_for_mock.fetch_add(1, Ordering::Relaxed);
                Box::pin(async { Ok(()) })
            });
        m.expect_inspect_container_state()
            .returning(|_| Box::pin(async { Ok(Some("exited".to_string())) }));
        m.expect_start_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
        m.expect_apply_bandwidth_limit()
            .never();
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "reuse-net").await;
    *existing.lock().unwrap() = Some(format!("ow-{}", instance_id));

    set_instance_status(&ctx.db, &instance_id, "stopped", Some("abc123def456")).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({}), &token).await;
    assert_eq!(resp.status(), 200, "start failed: {:?}", resp.text().await);
    assert_eq!(
        create_calls.load(Ordering::Relaxed),
        1,
        "start must reuse the existing instance network without allocating a new one"
    );
}

#[tokio::test]
async fn test_start_recreates_on_port_conflict() {
    let attempted_ports = Arc::new(std::sync::Mutex::new(Vec::new()));
    let calls = Arc::new(AtomicU32::new(0));
    let attempted_ports_for_mock = attempted_ports.clone();
    let calls_for_mock = calls.clone();

    let ctx = MockContext::new(move |m| {
        m.expect_create_container_from_template()
            .returning(move |_, _, config, _, _| {
                let ports_for_mock = attempted_ports_for_mock.clone();
                let calls_for_mock = calls_for_mock.clone();
                let host_port = config.host_port.unwrap_or(0);
                Box::pin(async move {
                    ports_for_mock.lock().unwrap().push(host_port);
                    if calls_for_mock.fetch_add(1, Ordering::Relaxed) == 0 {
                        Ok("id1".to_string())
                    } else {
                        Ok("id2".to_string())
                    }
                })
            });
        m.expect_inspect_container_state()
            .returning(|_| Box::pin(async { Ok(Some("exited".to_string())) }));
        m.expect_start_container_by_id()
            .returning(|_| Box::pin(async {
                Err(docker_err("Bind for 172.17.0.1:10000 failed: port is already allocated"))
            }));
        m.expect_remove_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (template_id, instance_id) = create_config_and_instance(&ctx, &token, "start-conflict-recreate").await;
    let _ = template_id;
    set_instance_status(&ctx.db, &instance_id, "stopped", Some("id1")).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({}), &token).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "starting");
    assert_eq!(body["container_id"], "id2");

    let attempted = attempted_ports.lock().unwrap().clone();
    assert_eq!(attempted.len(), 2, "launch + one recreated container");
    assert_ne!(attempted[0], attempted[1], "recreate must pick a different host port");

    let resp = ctx.get_auth(&format!("/api/instances/{}", instance_id), &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["container_id"], "id2");
    assert_eq!(body["instance"]["host_port"], attempted[1]);
}

#[tokio::test]
async fn test_start_docker_start_fails() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
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
        m.expect_stop_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
        m.expect_remove_container_by_id()
            .returning(|_| Box::pin(async { Err(docker_err("remove failed")) }));
        m.expect_remove_network()
            .returning(|_| Box::pin(async { Ok(()) }));
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

#[tokio::test]
async fn test_stop_preserves_host_port_and_route() {
    let dir = openworkspace_api::route_writer::default_dynamic_dir();
    std::fs::create_dir_all(&dir).unwrap();

    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_stop_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
        m.expect_inspect_container_state()
            .returning(|_| Box::pin(async { Ok(Some("exited".to_string())) }));
        m.expect_start_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
        m.expect_apply_bandwidth_limit()
            .never();
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "stop-keep-port-route").await;

    let id: uuid::Uuid = instance_id.parse().unwrap();
    let repo = WorkspaceInstanceRepository::new(&ctx.db);
    let inst = repo.find_by_id(id).await.unwrap().unwrap();
    let host_port = inst.host_port.expect("launch must persist a host port");
    let access_token = inst.access_token.clone();
    let route_file = dir.join(format!("kasmvnc-{}-ws.yml", access_token));
    assert!(route_file.exists(), "launch must write a route file");

    set_instance_status(&ctx.db, &instance_id, "running", Some("abc123def456")).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({}), &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "stopped");

    assert!(route_file.exists(), "route must survive stop (no route churn)");
    let after_stop = repo.find_by_id(id).await.unwrap().unwrap();
    assert_eq!(after_stop.host_port, Some(host_port), "host port must survive stop");

    let resp = ctx.post_auth(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({}), &token).await;
    assert_eq!(resp.status(), 200, "start failed: {:?}", resp.text().await);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "starting");

    let after_start = repo.find_by_id(id).await.unwrap().unwrap();
    assert_eq!(after_start.host_port, Some(host_port), "host port must be stable across restart");
    let content = std::fs::read_to_string(&route_file).unwrap();
    assert!(
        content.contains(&format!("host.docker.internal:{}", host_port)),
        "route must still target the same host port"
    );

    let _ = std::fs::remove_file(&route_file);
}

// ── Ticket 02: a legacy instance (host_port NULL) is allocated + committed
//    a port on next start, before its container is created. ──

#[tokio::test]
async fn test_start_backfills_host_port_before_create() {
    let captured = std::sync::Arc::new(std::sync::Mutex::new(None));
    let mock_captured = std::sync::Arc::clone(&captured);
    let ctx = MockContext::new(move |m| {
        let captured = std::sync::Arc::clone(&mock_captured);
        m.expect_create_container_from_template()
            .returning(move |_, _, cfg, _, _| {
                *captured.lock().unwrap() = cfg.host_port;
                Box::pin(async { Ok("fake-container-id".to_string()) })
            });
    }).await;

    let token = ctx.login_admin().await;
    let config_resp = ctx.post_auth("/api/templates", &serde_json::json!({
        "name": "legacy-backfill", "image": "busybox:1"
    }), &token).await;
    let template_id = config_resp.json::<serde_json::Value>().await.unwrap()["template"]["id"].as_str().unwrap().to_string();
    let template_uuid: uuid::Uuid = template_id.parse().unwrap();

    let admin = openworkspace_api::db::UserRepository::new(&ctx.db)
        .find_by_username("admin").await.unwrap().unwrap();
    let admin_id = admin.id;
    let repo = WorkspaceInstanceRepository::new(&ctx.db);
    let instance = repo.launch(template_uuid, admin_id, "legacy-instance", false, None).await.unwrap();
    assert!(instance.host_port.is_none(), "legacy row has no host_port");

    let resp = ctx.post_auth(&format!("/api/instances/{}/start", instance.id), &serde_json::json!({}), &token).await;
    assert_eq!(resp.status(), 200, "start failed: {:?}", resp.text().await);

    let after = repo.find_by_id(instance.id).await.unwrap().unwrap();
    let committed = after.host_port.expect("start must backfill and commit a host port");
    let used_for_create = captured.lock().unwrap().expect("create must receive the backfilled host port");
    assert_eq!(
        committed as u16, used_for_create,
        "backfill must be committed before the container create uses it"
    );
}

// ── Ticket 02: deleting an instance removes its row and frees the port for
//    reuse by a later launch. ──

#[tokio::test]
async fn test_delete_frees_host_port() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_stop_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
        m.expect_remove_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
        m.expect_remove_network()
            .returning(|_| Box::pin(async { Ok(()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id_a) = create_config_and_instance(&ctx, &token, "free-port-a").await;
    let repo = WorkspaceInstanceRepository::new(&ctx.db);
    let id_a: uuid::Uuid = instance_id_a.parse().unwrap();
    let port_a = repo.find_by_id(id_a).await.unwrap().unwrap().host_port.unwrap();

    let resp = ctx.delete_auth(&format!("/api/instances/{}", instance_id_a), &token).await;
    assert_eq!(resp.status(), 204);

    // The deleted instance's flock reservation is gone — the port is reusable.
    assert_port_released(port_a as u16);

    let (_, instance_id_b) = create_config_and_instance(&ctx, &token, "free-port-b").await;
    let id_b: uuid::Uuid = instance_id_b.parse().unwrap();
    let port_b = repo.find_by_id(id_b).await.unwrap().unwrap().host_port.unwrap();
    assert!(port_b > 0, "second launch must commit a host port after the delete");
}

// ── Ticket 02: an error-state instance keeps its row (and port reservation)
//    until deleted — a later launch must not take its port. ──

#[tokio::test]
async fn test_error_instance_keeps_port_reservation() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id_a) = create_config_and_instance(&ctx, &token, "reserve-port-a").await;
    let repo = WorkspaceInstanceRepository::new(&ctx.db);
    let id_a: uuid::Uuid = instance_id_a.parse().unwrap();
    let port_a = repo.find_by_id(id_a).await.unwrap().unwrap().host_port.unwrap();

    repo.update_status(id_a, "error").await.unwrap();

    let (_, instance_id_b) = create_config_and_instance(&ctx, &token, "reserve-port-b").await;
    let id_b: uuid::Uuid = instance_id_b.parse().unwrap();
    let port_b = repo.find_by_id(id_b).await.unwrap().unwrap().host_port.unwrap();
    assert_ne!(port_a, port_b, "error-state instance must keep its port reserved");
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

// ── Test 18: delete_instance — no container, just deletes (lines 263-264) ──

#[tokio::test]
async fn test_delete_no_container() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_remove_network()
            .returning(|_| Box::pin(async { Ok(()) }));
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
    }).await;

    let token = ctx.login_admin().await;
    create_config_and_instance(&ctx, &token, "list-test").await;

    let resp = ctx.get_auth("/api/instances", &token).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert!(!body["instances"].as_array().unwrap().is_empty());
}

// ── Test 21: get_instance (lines 210-234) ──

#[tokio::test]
async fn test_get_instance() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
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
        m.expect_stop_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
        m.expect_remove_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
        m.expect_remove_network()
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
        m.expect_stop_container_by_id()
            .returning(|_| Box::pin(async { Err(docker_404()) }));
        m.expect_remove_container_by_id()
            .returning(|_| Box::pin(async { Err(docker_404()) }));
        m.expect_remove_network()
            .returning(|_| Box::pin(async { Ok(()) }));
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
            .withf(|_, _, config, _, _| config.runtime == Some("runc".to_string()))
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
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

// ── Keep Time: heartbeat endpoint ──

#[tokio::test]
async fn test_heartbeat_sets_last_seen_at() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "heartbeat-ok").await;
    set_instance_status(&ctx.db, &instance_id, "running", Some("abc123def456")).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/heartbeat", instance_id), &serde_json::json!({}), &token).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "ok");

    let id: uuid::Uuid = instance_id.parse().unwrap();
    let found = WorkspaceInstanceRepository::new(&ctx.db).find_by_id(id).await.unwrap().unwrap();
    assert!(found.last_seen_at.is_some());

    let resp = ctx.post_auth(&format!("/api/instances/{}/heartbeat", instance_id), &serde_json::json!({}), &token).await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_heartbeat_requires_auth() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "heartbeat-unauth").await;

    let unauth = reqwest::Client::new();
    let resp = unauth
        .post(format!("{}/api/instances/{}/heartbeat", ctx.base_url, instance_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_heartbeat_forbidden_for_non_owner() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let admin_token = ctx.login_admin().await;
    let owner = ctx.post_auth("/api/users", &serde_json::json!({
        "username": "hb-owner", "password": "pass123"
    }), &admin_token).await;
    assert_eq!(owner.status(), 200);
    let owner_id = owner.json::<serde_json::Value>().await.unwrap()["user"]["id"].as_str().unwrap().to_string();
    ctx.post_auth("/api/users", &serde_json::json!({
        "username": "hb-intruder", "password": "pass123"
    }), &admin_token).await;

    let owner_token = ctx.login_user("hb-owner", "pass123").await;
    let intruder_token = ctx.login_user("hb-intruder", "pass123").await;

    let config_resp = ctx.post_auth("/api/templates", &serde_json::json!({
        "name": "heartbeat-owner", "image": "busybox:1"
    }), &admin_token).await;
    let template_id = config_resp.json::<serde_json::Value>().await.unwrap()["template"]["id"].as_str().unwrap().to_string();

    // The owner launches with a personal whitelist entry for the template.
    grant_template_whitelist(&ctx, &owner_id, &template_id).await;

    let launch_resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id
    }), &owner_token).await;
    assert_eq!(launch_resp.status(), 200, "body: {:?}", launch_resp.text().await);
    let instance_id = launch_resp.json::<serde_json::Value>().await.unwrap()["instance"]["id"].as_str().unwrap().to_string();

    let resp = ctx.post_auth(&format!("/api/instances/{}/heartbeat", instance_id), &serde_json::json!({}), &intruder_token).await;
    assert_eq!(resp.status(), 403);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "Forbidden");

    let resp = ctx.post_auth(&format!("/api/instances/{}/heartbeat", instance_id), &serde_json::json!({}), &owner_token).await;
    assert_eq!(resp.status(), 200);
}

#[tokio::test]
async fn test_heartbeat_unknown_instance() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let unknown_id = uuid::Uuid::new_v4();

    let resp = ctx.post_auth(&format!("/api/instances/{}/heartbeat", unknown_id), &serde_json::json!({}), &token).await;
    assert_eq!(resp.status(), 404);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "Instance not found");
}

// ── Keep Time: lifecycle keeps last_seen_at in lockstep with started_at ──

#[tokio::test]
async fn test_unpause_sets_last_seen_at() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_unpause_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "unpause-sets-last-seen").await;
    set_instance_status(&ctx.db, &instance_id, "paused", Some("abc123def456")).await;
    let id: uuid::Uuid = instance_id.parse().unwrap();
    let repo = WorkspaceInstanceRepository::new(&ctx.db);
    repo.update_started_at(id, None).await.unwrap();
    repo.update_last_seen_at(id, None).await.unwrap();

    let resp = ctx.post_auth(&format!("/api/instances/{}/unpause", instance_id), &serde_json::json!({}), &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "running");

    let found = repo.find_by_id(id).await.unwrap().unwrap();
    assert!(found.started_at.is_some());
    assert!(found.last_seen_at.is_some());
}

#[tokio::test]
async fn test_pause_clears_last_seen_at() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_pause_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "pause-clears-last-seen").await;
    set_instance_status(&ctx.db, &instance_id, "running", Some("abc123def456")).await;
    let id: uuid::Uuid = instance_id.parse().unwrap();
    let repo = WorkspaceInstanceRepository::new(&ctx.db);
    repo.update_started_at(id, Some(chrono::Utc::now())).await.unwrap();
    repo.update_last_seen_at(id, Some(chrono::Utc::now())).await.unwrap();

    let resp = ctx.post_auth(&format!("/api/instances/{}/pause", instance_id), &serde_json::json!({}), &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "paused");

    let found = repo.find_by_id(id).await.unwrap().unwrap();
    assert!(found.started_at.is_none());
    assert!(found.last_seen_at.is_none());
}

#[tokio::test]
async fn test_stop_clears_last_seen_at() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_stop_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "stop-clears-last-seen").await;
    set_instance_status(&ctx.db, &instance_id, "running", Some("abc123def456")).await;
    let id: uuid::Uuid = instance_id.parse().unwrap();
    let repo = WorkspaceInstanceRepository::new(&ctx.db);
    repo.update_started_at(id, Some(chrono::Utc::now())).await.unwrap();
    repo.update_last_seen_at(id, Some(chrono::Utc::now())).await.unwrap();

    let resp = ctx.post_auth(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({}), &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["status"], "stopped");

    let found = repo.find_by_id(id).await.unwrap().unwrap();
    assert!(found.started_at.is_none());
    assert!(found.last_seen_at.is_none());
}

// ── Keep Time: deadline & action in instance JSON ──

async fn create_keep_time_config_and_instance(ctx: &MockContext, token: &str, name: &str, keep_time_seconds: i64, keep_time_action: &str) -> (String, String) {
    let config_resp = ctx.post_auth("/api/templates", &serde_json::json!({
        "name": name, "image": "busybox:1",
        "keep_time_seconds": keep_time_seconds,
        "keep_time_action": keep_time_action
    }), token).await;
    let template_id = config_resp.json::<serde_json::Value>().await.unwrap()["template"]["id"].as_str().unwrap().to_string();

    let launch_resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id
    }), token).await;
    let body = launch_resp.json::<serde_json::Value>().await.unwrap();
    let instance_id = body["instance"]["id"].as_str().unwrap().to_string();
    (template_id, instance_id)
}

#[tokio::test]
async fn test_keep_time_deadline_running_instance() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_keep_time_config_and_instance(&ctx, &token, "keep-deadline", 3600, "pause").await;
    set_instance_status(&ctx.db, &instance_id, "running", Some("abc123def456")).await;
    let id: uuid::Uuid = instance_id.parse().unwrap();
    let repo = WorkspaceInstanceRepository::new(&ctx.db);
    let last_seen = chrono::Utc.with_ymd_and_hms(2026, 2, 3, 4, 5, 6).unwrap();
    repo.update_last_seen_at(id, Some(last_seen)).await.unwrap();

    let resp = ctx.get_auth(&format!("/api/instances/{}", instance_id), &token).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let expected = last_seen + chrono::Duration::seconds(3600);
    let expected_str = serde_json::Value::String(expected.to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true));
    assert_eq!(body["instance"]["keep_time_deadline"], expected_str);
    assert_eq!(body["instance"]["keep_time_action"], "pause");

    let resp = ctx.get_auth("/api/instances", &token).await;
    let body: serde_json::Value = resp.json().await.unwrap();
    let found = body["instances"].as_array().unwrap().iter()
        .find(|i| i["id"] == instance_id)
        .unwrap();
    assert_eq!(found["keep_time_deadline"], expected_str);
    assert_eq!(found["keep_time_action"], "pause");
}

#[tokio::test]
async fn test_keep_time_deadline_null_when_not_running() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_keep_time_config_and_instance(&ctx, &token, "keep-not-running", 3600, "stop").await;
    set_instance_status(&ctx.db, &instance_id, "stopped", Some("abc123def456")).await;

    let resp = ctx.get_auth(&format!("/api/instances/{}", instance_id), &token).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["keep_time_deadline"], serde_json::Value::Null);
    assert_eq!(body["instance"]["keep_time_action"], "stop");
}

#[tokio::test]
async fn test_keep_time_null_when_template_unconfigured() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_config_and_instance(&ctx, &token, "keep-unconfigured").await;
    set_instance_status(&ctx.db, &instance_id, "running", Some("abc123def456")).await;

    let resp = ctx.get_auth(&format!("/api/instances/{}", instance_id), &token).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["keep_time_deadline"], serde_json::Value::Null);
    assert_eq!(body["instance"]["keep_time_action"], serde_json::Value::Null);
}

#[tokio::test]
async fn test_keep_time_seconds_in_instance_json() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_keep_time_config_and_instance(&ctx, &token, "keep-seconds-set", 600, "stop").await;

    let resp = ctx.get_auth(&format!("/api/instances/{}", instance_id), &token).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["keep_time_seconds"], 600, "keep_time_seconds should echo the template value even when not running");
    assert_eq!(body["instance"]["keep_time_action"], "stop");

    let (_, instance_id) = create_config_and_instance(&ctx, &token, "keep-seconds-off").await;
    let resp = ctx.get_auth(&format!("/api/instances/{}", instance_id), &token).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["keep_time_seconds"], serde_json::Value::Null, "keep_time_seconds should be null when the template has no keep-time");
    assert_eq!(body["instance"]["keep_time_action"], serde_json::Value::Null);
}

#[tokio::test]
async fn test_heartbeat_refreshes_keep_time_deadline() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let (_, instance_id) = create_keep_time_config_and_instance(&ctx, &token, "hb-deadline", 3600, "stop").await;
    set_instance_status(&ctx.db, &instance_id, "running", Some("abc123def456")).await;

    let before = chrono::Utc::now();
    let resp = ctx.post_auth(&format!("/api/instances/{}/heartbeat", instance_id), &serde_json::json!({}), &token).await;
    assert_eq!(resp.status(), 200);
    let after = chrono::Utc::now();

    let resp = ctx.get_auth(&format!("/api/instances/{}", instance_id), &token).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let deadline_str = body["instance"]["keep_time_deadline"].as_str().unwrap();
    let deadline = chrono::DateTime::parse_from_rfc3339(deadline_str).unwrap().with_timezone(&chrono::Utc);

    let lo = (before + chrono::Duration::seconds(3600) - chrono::Duration::seconds(5)).timestamp_millis();
    let hi = (after + chrono::Duration::seconds(3600) + chrono::Duration::seconds(5)).timestamp_millis();
    assert!(deadline.timestamp_millis() >= lo && deadline.timestamp_millis() <= hi);
}

// ── Ticket 03: launch persistence mode ──────────────────────────

async fn create_persistent_template(
    ctx: &MockContext,
    token: &str,
    name: &str,
    persistent_storage_path: Option<&str>,
) -> String {
    let body = serde_json::json!({
        "name": name,
        "image": "busybox:1",
        "persistent_storage_path": persistent_storage_path,
    });
    let resp = ctx.post_auth("/api/templates", &body, token).await;
    resp.json::<serde_json::Value>().await.unwrap()["template"]["id"].as_str().unwrap().to_string()
}

async fn admin_user_id(ctx: &MockContext) -> String {
    use openworkspace_api::db::UserRepository;
    let admin = UserRepository::new(&ctx.db)
        .find_by_username("admin")
        .await
        .unwrap()
        .expect("admin user must exist");
    admin.id.to_string()
}

#[tokio::test]
async fn test_launch_use_persistent_resolves_server_side() {
    let prepared: std::sync::Arc<std::sync::Mutex<Vec<String>>> = std::sync::Arc::default();
    let prepared_for_mock = prepared.clone();
    let ctx = MockContext::new(move |m| {
        let prepared = prepared_for_mock.clone();
        m.expect_prepare_persistent_volume()
            .returning(move |host_path, volume_name| {
                let mut log = prepared.lock().unwrap();
                log.push(host_path.to_string());
                log.push(volume_name.to_string());
                Box::pin(async { Ok(()) })
            });
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let owner_id = admin_user_id(&ctx).await;
    let template_id = create_persistent_template(&ctx, &token, "persist-srv", Some("/mnt/ow_dir")).await;

    let resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
        "persistence": "use_persistent",
        "resolved_volume_host_path": "/evil/client/ignored",
        "mount_persistent": false,
    }), &token).await;
    assert_eq!(resp.status(), 200, "body: {:?}", resp.text().await);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["mount_persistent"], true);
    let expected_path = format!("/mnt/ow_dir/{}/{}", "persist-srv", owner_id);
    assert_eq!(body["instance"]["resolved_volume_host_path"], expected_path);

    let log = prepared.lock().unwrap();
    assert_eq!(log.len(), 2, "prepare_persistent_volume must be called exactly once with both args");
    assert_eq!(log[0], expected_path, "host_path must be the server-resolved path, not the client's");
    assert_eq!(
        log[1],
        openworkspace_api::persistent_volume::persistent_volume_name(&expected_path)
    );
}

#[tokio::test]
async fn test_launch_legacy_mount_persistent_maps_to_use() {
    let ctx = MockContext::new(|m| {
        m.expect_prepare_persistent_volume()
            .returning(|_, _| Box::pin(async { Ok(()) }));
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let owner_id = admin_user_id(&ctx).await;
    let template_id = create_persistent_template(&ctx, &token, "persist-legacy", Some("/mnt/ow_dir")).await;

    let resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
        "mount_persistent": true,
        "resolved_volume_host_path": "/evil/client/ignored"
    }), &token).await;
    assert_eq!(resp.status(), 200, "body: {:?}", resp.text().await);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["mount_persistent"], true);
    assert_eq!(
        body["instance"]["resolved_volume_host_path"],
        format!("/mnt/ow_dir/{}/{}", "persist-legacy", owner_id)
    );
}

#[tokio::test]
async fn test_launch_second_persistent_same_template_and_owner_conflicts() {
    let ctx = MockContext::new(|m| {
        m.expect_prepare_persistent_volume()
            .times(1)
            .returning(|_, _| Box::pin(async { Ok(()) }));
        m.expect_create_container_from_template()
            .times(2)
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let template_id = create_persistent_template(&ctx, &token, "persist-conflict", Some("/mnt/ow_dir")).await;

    let first = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
        "persistence": "use_persistent",
    }), &token).await;
    assert_eq!(first.status(), 200);
    let first_body: serde_json::Value = first.json().await.unwrap();
    assert_eq!(first_body["instance"]["mount_persistent"], true);

    let second = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
        "persistence": "use_persistent",
    }), &token).await;
    assert_eq!(second.status(), 409);
    let second_body: serde_json::Value = second.json().await.unwrap();
    assert!(second_body["error"].as_str().unwrap().contains("persistent storage already exists"));

    let reset = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
        "persistence": "reset_persistent",
    }), &token).await;
    assert_eq!(reset.status(), 409);

    let non_persistent = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
        "persistence": "no_persistent",
    }), &token).await;
    assert_eq!(non_persistent.status(), 200);
    let np_body: serde_json::Value = non_persistent.json().await.unwrap();
    assert_eq!(np_body["instance"]["mount_persistent"], false);
}

#[tokio::test]
async fn test_launch_persistent_conflict_is_per_owner() {
    let ctx = MockContext::new(|m| {
        m.expect_prepare_persistent_volume()
            .times(2)
            .returning(|_, _| Box::pin(async { Ok(()) }));
        m.expect_create_container_from_template()
            .times(2)
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let template_id = create_persistent_template(&ctx, &token, "persist-per-owner", Some("/mnt/ow_dir")).await;

    let first = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
        "persistence": "use_persistent",
    }), &token).await;
    assert_eq!(first.status(), 200);

    let other = ctx.post_auth("/api/users", &serde_json::json!({
        "username": "other_persist_user",
        "password": "password123",
    }), &token).await;
    assert_eq!(other.status(), 200);
    let other_id = other.json::<serde_json::Value>().await.unwrap()["user"]["id"]
        .as_str().unwrap().to_string();
    grant_template_whitelist(&ctx, &other_id, &template_id).await;
    let other_token = ctx.login_user("other_persist_user", "password123").await;

    let resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
        "persistence": "use_persistent",
    }), &other_token).await;
    assert_eq!(resp.status(), 200, "a different owner must be allowed to launch persistently");
}

#[tokio::test]
async fn test_launch_null_persistent_root_degrades_to_no_persistent() {
    let ctx = MockContext::new(|m| {
        m.expect_prepare_persistent_volume()
            .never();
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let template_id = create_persistent_template(&ctx, &token, "persist-null-root", None).await;

    let resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
        "persistence": "use_persistent",
        "resolved_volume_host_path": "/evil/client/ignored"
    }), &token).await;
    assert_eq!(resp.status(), 200, "body: {:?}", resp.text().await);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["mount_persistent"], false);
    assert_eq!(body["instance"]["resolved_volume_host_path"], serde_json::Value::Null);
}

// ── Ticket 04: reset & remove cleanup ──────────────────────────

#[tokio::test]
async fn test_reset_persistent_removes_then_prepares() {
    let log: std::sync::Arc<std::sync::Mutex<Vec<String>>> = std::sync::Arc::default();
    let log_for_mock = log.clone();
    let ctx = MockContext::new(move |m| {
        let log = log_for_mock.clone();
        m.expect_remove_persistent_volume()
            .returning(move |host_path, volume_name| {
                log.lock().unwrap().push(format!("remove|{}|{}", host_path, volume_name));
                Box::pin(async { Ok(()) })
            });
        let log = log_for_mock.clone();
        m.expect_prepare_persistent_volume()
            .returning(move |host_path, volume_name| {
                log.lock().unwrap().push(format!("prepare|{}|{}", host_path, volume_name));
                Box::pin(async { Ok(()) })
            });
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let owner_id = admin_user_id(&ctx).await;
    let template_id = create_persistent_template(&ctx, &token, "persist-reset", Some("/mnt/ow_dir")).await;

    let resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
        "persistence": "reset_persistent",
        "mount_persistent": false,
    }), &token).await;
    assert_eq!(resp.status(), 200, "body: {:?}", resp.text().await);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["mount_persistent"], true);

    let expected_path = format!("/mnt/ow_dir/{}/{}", "persist-reset", owner_id);
    let expected_volume = openworkspace_api::persistent_volume::persistent_volume_name(&expected_path);
    let log = log.lock().unwrap();
    assert_eq!(
        log.as_slice(),
        &[
            format!("remove|{}|{}", expected_path, expected_volume),
            format!("prepare|{}|{}", expected_path, expected_volume),
        ],
        "reset_persistent must remove the old volume before re-preparing"
    );
}

#[tokio::test]
async fn test_reset_persistent_no_remove_for_plain_use() {
    let log: std::sync::Arc<std::sync::Mutex<Vec<String>>> = std::sync::Arc::default();
    let log_for_mock = log.clone();
    let ctx = MockContext::new(move |m| {
        let log = log_for_mock.clone();
        m.expect_remove_persistent_volume()
            .returning(move |host_path, volume_name| {
                log.lock().unwrap().push(format!("remove|{}|{}", host_path, volume_name));
                Box::pin(async { Ok(()) })
            });
        let log = log_for_mock.clone();
        m.expect_prepare_persistent_volume()
            .returning(move |host_path, volume_name| {
                log.lock().unwrap().push(format!("prepare|{}|{}", host_path, volume_name));
                Box::pin(async { Ok(()) })
            });
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let template_id = create_persistent_template(&ctx, &token, "persist-plain-use", Some("/mnt/ow_dir")).await;

    let resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
        "persistence": "use_persistent",
    }), &token).await;
    assert_eq!(resp.status(), 200, "body: {:?}", resp.text().await);

    let log = log.lock().unwrap();
    assert_eq!(log.len(), 1, "plain use must only prepare (no remove), got {:?}", *log);
    assert!(log[0].starts_with("prepare|"), "plain use must not call remove_persistent_volume");
}

#[tokio::test]
async fn test_reset_persistent_with_existing_instance_conflicts() {
    let ctx = MockContext::new(|m| {
        m.expect_remove_persistent_volume()
            .never();
        m.expect_prepare_persistent_volume()
            .times(1)
            .returning(|_, _| Box::pin(async { Ok(()) }));
        m.expect_create_container_from_template()
            .times(1)
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let template_id = create_persistent_template(&ctx, &token, "persist-reset-conflict", Some("/mnt/ow_dir")).await;

    let first = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
        "persistence": "use_persistent",
    }), &token).await;
    assert_eq!(first.status(), 200);

    let reset = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
        "persistence": "reset_persistent",
    }), &token).await;
    assert_eq!(reset.status(), 409);
    let body: serde_json::Value = reset.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("persistent storage already exists"));
}

#[tokio::test]
async fn test_delete_persistent_instance_preserves_volume() {
    let ctx = MockContext::new(|m| {
        m.expect_prepare_persistent_volume()
            .returning(|_, _| Box::pin(async { Ok(()) }));
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_stop_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
        m.expect_remove_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
        m.expect_remove_network()
            .returning(|_| Box::pin(async { Ok(()) }));
        m.expect_remove_persistent_volume()
            .never();
    }).await;

    let token = ctx.login_admin().await;
    let template_id = create_persistent_template(&ctx, &token, "persist-delete", Some("/mnt/ow_dir")).await;

    let resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
        "persistence": "use_persistent",
    }), &token).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    let instance_id = body["instance"]["id"].as_str().unwrap();

    let resp = ctx.delete_auth(&format!("/api/instances/{}", instance_id), &token).await;
    assert_eq!(resp.status(), 204);

    let list = ctx.get_auth("/api/instances", &token).await;
    let list_body: serde_json::Value = list.json().await.unwrap();
    assert!(
        list_body["instances"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["id"].as_str() != Some(instance_id)),
        "delete must remove the DB record"
    );
}

// ── Ticket 03 gaps: error-state launch, restart volume ensure, migration backfill ──

/// Insert a workspace instance row directly (bypassing the launch route), so
/// tests can fabricate legacy / stopped / error instances.
async fn insert_instance(
    db: &DatabaseConnection,
    template_id: &str,
    owner_id: &str,
    name: &str,
    status: &str,
    mount_persistent: bool,
    resolved_volume_host_path: Option<&str>,
    container_id: Option<&str>,
) -> String {
    use openworkspace_api::db::workspace_instance;
    use sea_orm::{ActiveModelTrait, Set};
    let id = uuid::Uuid::new_v4();
    let model = workspace_instance::ActiveModel {
        id: Set(id),
        template_id: Set(template_id.parse().unwrap()),
        name: Set(name.to_string()),
        instance_number: Set(1),
        owner_id: Set(owner_id.parse().unwrap()),
        container_id: Set(container_id.map(|s| s.to_string())),
        status: Set(status.to_string()),
        access_token: Set(format!("tok-{}", name)),
        access_password: Set("pwd".to_string()),
        mount_persistent: Set(mount_persistent),
        resolved_volume_host_path: Set(resolved_volume_host_path.map(|s| s.to_string())),
        ..Default::default()
    };
    model.insert(db).await.unwrap().id.to_string()
}

async fn instance_by_name(ctx: &MockContext, token: &str, name: &str) -> serde_json::Value {
    let list = ctx.get_auth("/api/instances", token).await;
    let list_body: serde_json::Value = list.json().await.unwrap();
    list_body["instances"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["name"].as_str() == Some(name))
        .expect("instance must exist")
        .clone()
}

#[tokio::test]
async fn test_launch_volume_prep_failure_marks_error_and_keeps_record() {
    let ctx = MockContext::new(|m| {
        m.expect_prepare_persistent_volume()
            .returning(|_, _| Box::pin(async { Err("helper container failed".to_string()) }));
        m.expect_create_container_from_template()
            .never();
    }).await;

    let token = ctx.login_admin().await;
    let template_id = create_persistent_template(&ctx, &token, "persist-prep-fail", Some("/mnt/ow_dir")).await;

    let resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
        "persistence": "use_persistent",
    }), &token).await;
    assert_eq!(resp.status(), 200, "volume-prep failure must follow the existing launch error path");
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"], "error");
    assert!(body["docker_error"].as_str().unwrap().contains("helper container failed"));

    let inst = instance_by_name(&ctx, &token, "persist-prep-fail-1").await;
    assert_eq!(inst["status"], "error", "the failed launch must leave a DB record in error state");
    assert_eq!(inst["mount_persistent"], true);
    assert!(inst["resolved_volume_host_path"].as_str().is_some());
}

#[tokio::test]
async fn test_reset_volume_remove_failure_marks_error() {
    let ctx = MockContext::new(|m| {
        m.expect_remove_persistent_volume()
            .returning(|_, _| Box::pin(async { Err("remove helper failed".to_string()) }));
        m.expect_prepare_persistent_volume()
            .never();
    }).await;

    let token = ctx.login_admin().await;
    let template_id = create_persistent_template(&ctx, &token, "persist-remove-fail", Some("/mnt/ow_dir")).await;

    let resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
        "persistence": "reset_persistent",
    }), &token).await;
    assert_eq!(resp.status(), 200);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["instance"]["status"], "error");
    assert!(body["docker_error"].as_str().unwrap().contains("remove helper failed"));
}

#[tokio::test]
async fn test_reset_replaces_broken_error_instance() {
    let log: std::sync::Arc<std::sync::Mutex<Vec<String>>> = std::sync::Arc::default();
    let log_for_mock = log.clone();
    let ctx = MockContext::new(move |m| {
        m.expect_prepare_persistent_volume()
            .times(1)
            .returning(|_, _| Box::pin(async { Err("first attempt failed".to_string()) }));
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        let log = log_for_mock.clone();
        m.expect_remove_persistent_volume()
            .returning(move |host_path, volume_name| {
                log.lock().unwrap().push(format!("remove|{}|{}", host_path, volume_name));
                Box::pin(async { Ok(()) })
            });
        let log = log_for_mock.clone();
        m.expect_prepare_persistent_volume()
            .returning(move |host_path, volume_name| {
                log.lock().unwrap().push(format!("prepare|{}|{}", host_path, volume_name));
                Box::pin(async { Ok(()) })
            });
    }).await;

    let token = ctx.login_admin().await;
    let owner_id = admin_user_id(&ctx).await;
    let template_id = create_persistent_template(&ctx, &token, "persist-replace-broken", Some("/mnt/ow_dir")).await;

    // First launch fails at volume prep → the record is kept in `error` state.
    let first = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
        "persistence": "use_persistent",
    }), &token).await;
    assert_eq!(first.status(), 200);
    let first_body: serde_json::Value = first.json().await.unwrap();
    assert_eq!(first_body["instance"]["status"], "error");

    // A second persistent launch is normally 409, but a broken (`error`)
    // instance occupies no real tenant slot, so reset may wipe + re-prepare.
    let second = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
        "persistence": "reset_persistent",
    }), &token).await;
    assert_eq!(second.status(), 200, "body: {:?}", second.text().await);
    let second_body: serde_json::Value = second.json().await.unwrap();
    assert_eq!(second_body["instance"]["mount_persistent"], true);

    let expected_path = format!("/mnt/ow_dir/{}/{}", "persist-replace-broken", owner_id);
    let expected_volume = openworkspace_api::persistent_volume::persistent_volume_name(&expected_path);
    let log = log.lock().unwrap();
    assert!(
        log.iter().any(|e| e == &format!("remove|{}|{}", expected_path, expected_volume)),
        "broken-record replacement must wipe the old volume, got {:?}",
        *log
    );
    assert!(
        log.iter().any(|e| e == &format!("prepare|{}|{}", expected_path, expected_volume)),
        "broken-record replacement must re-prepare, got {:?}",
        *log
    );
    let remove_at = log.iter().position(|e| e.starts_with("remove|")).unwrap();
    let prepare_at = log.iter().position(|e| e.starts_with("prepare|")).unwrap();
    assert!(remove_at < prepare_at, "wipe must precede re-prepare, got {:?}", *log);
}

#[tokio::test]
async fn test_start_backfills_resolved_path_and_ensures_volume() {
    let ensured: std::sync::Arc<std::sync::Mutex<Vec<String>>> = std::sync::Arc::default();
    let captured: std::sync::Arc<std::sync::Mutex<Vec<Option<String>>>> = std::sync::Arc::default();
    let ensured_for_mock = ensured.clone();
    let captured_for_mock = captured.clone();
    let ctx = MockContext::new(move |m| {
        let ensured = ensured_for_mock.clone();
        m.expect_ensure_persistent_volume()
            .returning(move |host_path, volume_name| {
                ensured.lock().unwrap().push(format!("{}|{}", host_path, volume_name));
                Box::pin(async { Ok(()) })
            });
        let captured = captured_for_mock.clone();
        m.expect_create_container_from_template()
            .returning(move |_, _, config, _, _| {
                captured.lock().unwrap().push(config.persistent_volume_name.clone());
                Box::pin(async { Ok("fake-container-id".to_string()) })
            });
    }).await;

    let token = ctx.login_admin().await;
    let owner_id = admin_user_id(&ctx).await;
    let template_id = create_persistent_template(&ctx, &token, "persist-legacy-start", Some("/mnt/ow_dir")).await;

    // A legacy instance: mount_persistent = true but no stored host path.
    let instance_id = insert_instance(
        &ctx.db, &template_id, &owner_id, "legacy-start", "stopped", true, None, None,
    ).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({}), &token).await;
    assert_eq!(resp.status(), 200, "body: {:?}", resp.text().await);

    let expected_path = format!("/mnt/ow_dir/{}/{}", "persist-legacy-start", owner_id);
    let expected_volume = openworkspace_api::persistent_volume::persistent_volume_name(&expected_path);

    {
        let ensured = ensured.lock().unwrap();
        assert_eq!(
            ensured.as_slice(),
            &[format!("{}|{}", expected_path, expected_volume)],
            "start must ensure the persistent volume for the resolved path"
        );
    }
    {
        let captured = captured.lock().unwrap();
        assert_eq!(
            captured.as_slice(),
            &[Some(expected_volume.clone())],
            "the recreated container must mount the persistent volume"
        );
    }

    let inst = instance_by_name(&ctx, &token, "legacy-start").await;
    assert_eq!(inst["resolved_volume_host_path"], expected_path, "restart must backfill the resolved path");
    assert_eq!(inst["mount_persistent"], true);
}

#[tokio::test]
async fn test_start_redeclares_missing_volume_on_restart() {
    let ensured: std::sync::Arc<std::sync::Mutex<Vec<String>>> = std::sync::Arc::default();
    let ensured_for_mock = ensured.clone();
    let ctx = MockContext::new(move |m| {
        let ensured = ensured_for_mock.clone();
        m.expect_ensure_persistent_volume()
            .returning(move |host_path, volume_name| {
                ensured.lock().unwrap().push(format!("{}|{}", host_path, volume_name));
                Box::pin(async { Ok(()) })
            });
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let owner_id = admin_user_id(&ctx).await;
    let template_id = create_persistent_template(&ctx, &token, "persist-restart-ensure", Some("/mnt/ow_dir")).await;
    let expected_path = format!("/mnt/ow_dir/{}/{}", "persist-restart-ensure", owner_id);

    // Persistent instance with a stored path, container was lost.
    let instance_id = insert_instance(
        &ctx.db, &template_id, &owner_id, "restart-ensure", "stopped", true, Some(&expected_path), None,
    ).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({}), &token).await;
    assert_eq!(resp.status(), 200, "body: {:?}", resp.text().await);

    let expected_volume = openworkspace_api::persistent_volume::persistent_volume_name(&expected_path);
    let ensured = ensured.lock().unwrap();
    assert_eq!(
        ensured.as_slice(),
        &[format!("{}|{}", expected_path, expected_volume)],
        "restart must re-declare the lost local-bind volume (spec §補充)"
    );
}

#[tokio::test]
async fn test_start_ensure_volume_failure_returns_500() {
    let ctx = MockContext::new(|m| {
        m.expect_ensure_persistent_volume()
            .returning(|_, _| Box::pin(async { Err("volume lost and recreate failed".to_string()) }));
        m.expect_create_container_from_template()
            .never();
    }).await;

    let token = ctx.login_admin().await;
    let owner_id = admin_user_id(&ctx).await;
    let template_id = create_persistent_template(&ctx, &token, "persist-ensure-fail", Some("/mnt/ow_dir")).await;
    let instance_id = insert_instance(
        &ctx.db, &template_id, &owner_id, "ensure-fail", "stopped", true, Some("/mnt/ow_dir/whatever"), None,
    ).await;

    let resp = ctx.post_auth(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({}), &token).await;
    assert_eq!(resp.status(), 500);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["error"], "Failed to ensure persistent volume");
}

// ── Ticket 06: pre-flight gating of launch & start ─────────────────

/// Create a plain account and seed a personal ceiling via SQL (the
/// quota-override PUT is gone; `direct_max_instances` is the new seam).
/// Returns the new user's id.
async fn create_quota_user(
    ctx: &MockContext,
    admin_token: &str,
    username: &str,
    direct_max_instances: i32,
) -> String {
    let create = ctx.post_auth("/api/users", &serde_json::json!({
        "username": username,
        "password": "password123",
    }), admin_token).await;
    assert_eq!(create.status(), 200, "failed to create quota user");
    let user_id = create.json::<serde_json::Value>().await.unwrap()["user"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    seed_direct_max_instances(ctx, &user_id, direct_max_instances).await;
    user_id
}

async fn seed_direct_max_instances(ctx: &MockContext, user_id: &str, limit: i32) {
    use sea_orm::ConnectionTrait;
    ctx.db
        .execute_unprepared(&format!(
            "UPDATE users SET direct_max_instances = {} WHERE id = '{}'",
            limit, user_id
        ))
        .await
        .unwrap();
}

/// Grant the user a group-whitelist entry (to every group they belong to) so
/// the template passes the `pre_flight` whitelist check (the second seat of
/// the policy gate). The personal whitelist is gone: only group grants count.
async fn grant_template_whitelist(ctx: &MockContext, user_id: &str, template_id: &str) {
    use sea_orm::ConnectionTrait;
    ctx.db
        .execute_unprepared(&format!(
            "INSERT INTO group_templates (group_id, template_id) \
             SELECT ug.group_id, '{template_id}' FROM user_groups ug \
             WHERE ug.user_id = '{user_id}' ON CONFLICT DO NOTHING"
        ))
        .await
        .unwrap();
}

/// Grant a template to a specific group's whitelist.
async fn grant_group_template(ctx: &MockContext, group_id: &str, template_id: &str) {
    use sea_orm::ConnectionTrait;
    ctx.db
        .execute_unprepared(&format!(
            "INSERT INTO group_templates (group_id, template_id) \
             VALUES ('{group_id}', '{template_id}') ON CONFLICT DO NOTHING"
        ))
        .await
        .unwrap();
}

/// Create a template only (no launch), returning its id.
async fn create_template_only(ctx: &MockContext, token: &str, name: &str) -> String {
    let resp = ctx.post_auth("/api/templates", &serde_json::json!({
        "name": name, "image": "busybox:1"
    }), token).await;
    resp.json::<serde_json::Value>().await.unwrap()["template"]["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_launch_rejected_by_ceiling_returns_409_and_leaves_no_row() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .times(1)
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let admin_token = ctx.login_admin().await;
    let template_id = create_template_only(&ctx, &admin_token, "ceiling-launch").await;
    let user_id = create_quota_user(&ctx, &admin_token, "ceiling_user", 1).await;
    grant_template_whitelist(&ctx, &user_id, &template_id).await;
    let user_token = ctx.login_user("ceiling_user", "password123").await;

    // The user's first launch fits the ceiling of 1.
    let first = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
    }), &user_token).await;
    assert_eq!(first.status(), 200, "body: {:?}", first.text().await);

    // The second launch is refused fail-fast with the structured rejection body.
    let second = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
    }), &user_token).await;
    assert_eq!(second.status(), 409);
    let body: serde_json::Value = second.json().await.unwrap();
    assert_eq!(body["rejection"]["scope"], "user_instance");
    assert_eq!(body["rejection"]["current"], 1);
    assert_eq!(body["rejection"]["limit"], 1);
    assert_eq!(body["rejection"]["requested"], 1);
    assert!(body["error"].as_str().unwrap().contains("instance limit"));

    // The rejected launch created no DB row: only the first instance exists.
    let list = ctx.get_auth("/api/instances", &user_token).await;
    let list_body: serde_json::Value = list.json().await.unwrap();
    assert_eq!(list_body["instances"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_launch_rejected_by_template_whitelist_returns_403() {
    let ctx = MockContext::new(|m| {
        // Only the post-grant launch reaches the container create.
        m.expect_create_container_from_template()
            .times(1)
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let admin_token = ctx.login_admin().await;
    let template_id = create_template_only(&ctx, &admin_token, "whitelist-launch").await;
    let user_id = create_quota_user(&ctx, &admin_token, "whitelist_user", 5).await;
    let user_token = ctx.login_user("whitelist_user", "password123").await;

    // No whitelist entry, no ownership, no group: default-deny (403).
    let launch = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
    }), &user_token).await;
    assert_eq!(launch.status(), 403);
    let body: serde_json::Value = launch.json().await.unwrap();
    assert_eq!(body["rejection"]["scope"], "template_not_allowed");
    assert_eq!(body["rejection"]["current"], 0);
    assert_eq!(body["rejection"]["limit"], 0);
    assert_eq!(body["rejection"]["requested"], 1);

    // Rejected before any reservation: no row and no Docker call.
    let list = ctx.get_auth("/api/instances", &user_token).await;
    let list_body: serde_json::Value = list.json().await.unwrap();
    assert_eq!(list_body["instances"].as_array().unwrap().len(), 0);

    // Granting the personal whitelist unlocks the template.
    grant_template_whitelist(&ctx, &user_id, &template_id).await;
    let launch = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
    }), &user_token).await;
    assert_eq!(launch.status(), 200, "body: {:?}", launch.text().await);
}

#[tokio::test]
async fn test_start_rejected_by_ceiling_leaves_instance_stopped() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .times(2)
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let admin_token = ctx.login_admin().await;
    let template_id = create_template_only(&ctx, &admin_token, "ceiling-restart").await;
    let user_id = create_quota_user(&ctx, &admin_token, "restart_user", 1).await;
    grant_template_whitelist(&ctx, &user_id, &template_id).await;
    let user_token = ctx.login_user("restart_user", "password123").await;

    // Launch A (active), then stop it: A releases its instance-count slot.
    let a = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
    }), &user_token).await;
    assert_eq!(a.status(), 200, "body: {:?}", a.text().await);
    let a_id = a.json::<serde_json::Value>().await.unwrap()["instance"]["id"]
        .as_str().unwrap().to_string();
    set_instance_status(&ctx.db, &a_id, "stopped", Some("fake-container-id")).await;

    // Launch B: reuses the slot A released, so B becomes the active one.
    let b = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
    }), &user_token).await;
    assert_eq!(b.status(), 200, "body: {:?}", b.text().await);

    // Restarting A would push the user past the ceiling: 409, and A stays stopped.
    let restart = ctx.post_auth(&format!("/api/instances/{}/start", a_id), &serde_json::json!({}), &user_token).await;
    assert_eq!(restart.status(), 409);
    let body: serde_json::Value = restart.json().await.unwrap();
    assert_eq!(body["rejection"]["scope"], "user_instance");
    assert_eq!(body["rejection"]["current"], 1);

    let inst = ctx.get_auth(&format!("/api/instances/{}", a_id), &user_token).await;
    let inst_body: serde_json::Value = inst.json().await.unwrap();
    assert_eq!(inst_body["instance"]["status"], "stopped");
}

#[tokio::test]
async fn test_start_infra_failure_rolls_back_to_stopped() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .times(1)
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_inspect_container_state()
            .returning(|_| Box::pin(async { Ok(Some("exited".to_string())) }));
        m.expect_start_container_by_id()
            .returning(|_| Box::pin(async { Err(docker_err("start failed")) }));
    }).await;

    let admin_token = ctx.login_admin().await;
    let template_id = create_template_only(&ctx, &admin_token, "rollback-start").await;
    let user_id = create_quota_user(&ctx, &admin_token, "rollback_user", 1).await;
    grant_template_whitelist(&ctx, &user_id, &template_id).await;
    let user_token = ctx.login_user("rollback_user", "password123").await;

    let a = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
    }), &user_token).await;
    assert_eq!(a.status(), 200, "body: {:?}", a.text().await);
    let a_id = a.json::<serde_json::Value>().await.unwrap()["instance"]["id"]
        .as_str().unwrap().to_string();
    set_instance_status(&ctx.db, &a_id, "stopped", Some("fake-container-id")).await;

    // The quota gate passes, but the Docker start fails: the reservation must
    // roll the instance back to `stopped` so the user can retry.
    let restart = ctx.post_auth(&format!("/api/instances/{}/start", a_id), &serde_json::json!({}), &user_token).await;
    assert_eq!(restart.status(), 500);

    let inst = ctx.get_auth(&format!("/api/instances/{}", a_id), &user_token).await;
    let inst_body: serde_json::Value = inst.json().await.unwrap();
    assert_eq!(inst_body["instance"]["status"], "stopped");
}

#[tokio::test]
async fn test_user_launch_infra_failure_marks_error_and_keeps_record() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .times(1)
            .returning(|_, _, _, _, _| Box::pin(async { Err("Docker create failed".to_string()) }));
        m.expect_create_container_from_template()
            .times(1)
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let admin_token = ctx.login_admin().await;
    let template_id = create_template_only(&ctx, &admin_token, "err-retry").await;
    let user_id = create_quota_user(&ctx, &admin_token, "err_user", 1).await;
    grant_template_whitelist(&ctx, &user_id, &template_id).await;
    let user_token = ctx.login_user("err_user", "password123").await;

    // Infra failure after the quota gate: the launch is marked `error` and the
    // DB record is kept (visible for the user, spec §1).
    let first = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
    }), &user_token).await;
    assert_eq!(first.status(), 200, "body: {:?}", first.text().await);
    let first_body: serde_json::Value = first.json().await.unwrap();
    assert_eq!(first_body["instance"]["status"], "error");
    assert_eq!(first_body["docker_error"], "Docker create failed");

    // An `error` record is inactive, so it holds no quota: a retry succeeds.
    let second = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
    }), &user_token).await;
    assert_eq!(second.status(), 200, "body: {:?}", second.text().await);
    let second_body: serde_json::Value = second.json().await.unwrap();
    assert_eq!(second_body["instance"]["status"], "starting");
}

#[tokio::test]
async fn test_admin_restart_accounts_against_owner_quota() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .times(2)
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let admin_token = ctx.login_admin().await;
    let template_id = create_template_only(&ctx, &admin_token, "owner-quota").await;
    let owner_id = create_quota_user(&ctx, &admin_token, "owner_user", 1).await;
    grant_template_whitelist(&ctx, &owner_id, &template_id).await;
    let owner_token = ctx.login_user("owner_user", "password123").await;

    // Owner launches A (active), then it is stopped, freeing its slot.
    let a = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
    }), &owner_token).await;
    assert_eq!(a.status(), 200, "body: {:?}", a.text().await);
    let a_id = a.json::<serde_json::Value>().await.unwrap()["instance"]["id"]
        .as_str().unwrap().to_string();
    set_instance_status(&ctx.db, &a_id, "stopped", Some("fake-container-id")).await;

    // Owner launches B, reusing the slot: B is now the active instance.
    let b = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
    }), &owner_token).await;
    assert_eq!(b.status(), 200, "body: {:?}", b.text().await);

    // The Admin restarts A. Quota is counted for the *owner* (at its ceiling),
    // so this is refused with the owner's rejection body — not 500, and not a
    // success driven by the Admin's own (exempt) limits.
    let restart = ctx.post_auth(&format!("/api/instances/{}/start", a_id), &serde_json::json!({}), &admin_token).await;
    assert_eq!(restart.status(), 409);
    let body: serde_json::Value = restart.json().await.unwrap();
    assert_eq!(body["rejection"]["scope"], "user_instance");
    assert_eq!(body["rejection"]["current"], 1);
    assert_eq!(body["rejection"]["limit"], 1);

    let inst = ctx.get_auth(&format!("/api/instances/{}", a_id), &admin_token).await;
    let inst_body: serde_json::Value = inst.json().await.unwrap();
    assert_eq!(inst_body["instance"]["status"], "stopped");
}

#[tokio::test]
async fn test_launch_rejected_by_host_ceiling_returns_409() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .times(1)
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let admin_token = ctx.login_admin().await;

    // Tighten the host ceiling to exactly one active instance.
    let set = ctx.put_auth("/api/admin/settings", &serde_json::json!({
        "host_instance_limit": 1,
    }), &admin_token).await;
    assert_eq!(set.status(), 200, "settings update failed: {:?}", set.text().await);

    let template_id = create_template_only(&ctx, &admin_token, "host-ceiling").await;
    let user_a = create_quota_user(&ctx, &admin_token, "host_a", 5).await;
    let user_b = create_quota_user(&ctx, &admin_token, "host_b", 5).await;
    grant_template_whitelist(&ctx, &user_a, &template_id).await;
    grant_template_whitelist(&ctx, &user_b, &template_id).await;
    let token_a = ctx.login_user("host_a", "password123").await;
    let token_b = ctx.login_user("host_b", "password123").await;

    // The host ceiling counts across *all* users: A fills the single slot.
    let first = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
    }), &token_a).await;
    assert_eq!(first.status(), 200, "body: {:?}", first.text().await);

    // B's launch is fine per-user but bumps the global count past 1.
    let second = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
    }), &token_b).await;
    assert_eq!(second.status(), 409);
    let body: serde_json::Value = second.json().await.unwrap();
    assert_eq!(body["rejection"]["scope"], "host_instance");
    assert_eq!(body["rejection"]["current"], 1);
    assert_eq!(body["rejection"]["limit"], 1);
    assert_eq!(body["rejection"]["requested"], 1);
    assert!(body["error"].as_str().unwrap().contains("Host instance limit"));
}

#[tokio::test]
async fn test_concurrent_launches_same_user_at_ceiling_exactly_one_succeeds() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .times(1)
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let admin_token = ctx.login_admin().await;
    let template_id = create_template_only(&ctx, &admin_token, "conc-same").await;
    let user_id = create_quota_user(&ctx, &admin_token, "conc_user", 1).await;
    grant_template_whitelist(&ctx, &user_id, &template_id).await;
    let user_token = ctx.login_user("conc_user", "password123").await;

    // Hammer the launch endpoint with 4 simultaneous requests. The single
    // user-row lock serializes them: exactly one reserves, the rest 409.
    let client = ctx.client.clone();
    let url = format!("{}/api/instances", ctx.base_url);
    let body = serde_json::json!({ "template_id": template_id });
    let mut handles = Vec::new();
    for _ in 0..4 {
        let client = client.clone();
        let url = url.clone();
        let token = user_token.clone();
        let body = body.clone();
        handles.push(tokio::spawn(async move {
            client
                .post(&url)
                .header("Cookie", format!("ow_token={}", token))
                .json(&body)
                .send()
                .await
                .unwrap()
                .status()
        }));
    }

    let mut successes = 0;
    let mut conflicts = 0;
    for handle in handles {
        match handle.await.unwrap() {
            reqwest::StatusCode::OK => successes += 1,
            reqwest::StatusCode::CONFLICT => conflicts += 1,
            other => panic!("unexpected status: {}", other),
        }
    }
    assert_eq!(successes, 1, "exactly one concurrent launch must win");
    assert_eq!(conflicts, 3);
}

#[tokio::test]
async fn test_concurrent_launches_different_users_both_succeed() {
    // Real Docker surfaces a port collision as a bind failure on the second
    // concurrent create; the launch retry loop then re-allocates a distinct
    // port. Replay that: the 2nd create call conflicts, the retried one wins.
    let calls = Arc::new(AtomicU32::new(0));
    let calls_for_mock = calls.clone();
    let ctx = MockContext::new(move |m| {
        m.expect_create_container_from_template()
            .times(3)
            .returning(move |_, _, _, _, _| {
                let n = calls_for_mock.fetch_add(1, Ordering::Relaxed);
                Box::pin(async move {
                    if n == 1 {
                        Err("Bind for 172.17.0.1:10000 failed: port is already allocated".to_string())
                    } else {
                        Ok("fake-container-id".to_string())
                    }
                })
            });
    }).await;

    let admin_token = ctx.login_admin().await;
    let template_id = create_template_only(&ctx, &admin_token, "conc-diff").await;
    for (username, ceiling) in [("conc_x", 5), ("conc_y", 5)] {
        let user_id = create_quota_user(&ctx, &admin_token, username, ceiling).await;
        grant_template_whitelist(&ctx, &user_id, &template_id).await;
    }
    let token_x = ctx.login_user("conc_x", "password123").await;
    let token_y = ctx.login_user("conc_y", "password123").await;

    let client = ctx.client.clone();
    let url = format!("{}/api/instances", ctx.base_url);
    let body = serde_json::json!({ "template_id": template_id });
    let mut handles = Vec::new();
    for token in [token_x, token_y] {
        let client = client.clone();
        let url = url.clone();
        let body = body.clone();
        handles.push(tokio::spawn(async move {
            client
                .post(&url)
                .header("Cookie", format!("ow_token={}", token))
                .json(&body)
                .send()
                .await
                .unwrap()
                .status()
        }));
    }

    // Different users never contend on a lock: both launches go through.
    for handle in handles {
        assert_eq!(handle.await.unwrap(), reqwest::StatusCode::OK);
    }

    // Both committed distinct host ports (flock arbitration + retry).
    use openworkspace_api::db::workspace_instance;
    use sea_orm::{ColumnTrait, EntityTrait, QueryFilter};
    let instances = workspace_instance::Entity::find()
        .filter(workspace_instance::Column::TemplateId.eq(uuid::Uuid::parse_str(&template_id).unwrap()))
        .all(&ctx.db).await.unwrap();
    let mut ports: Vec<i32> = instances.iter().filter_map(|i| i.host_port).collect();
    ports.sort_unstable();
    assert_eq!(ports.len(), 2, "both concurrent launches must commit a host port");
    assert_ne!(ports[0], ports[1], "concurrent launches must allocate distinct host ports");
}

#[tokio::test]
async fn test_persistent_uniqueness_inside_tx_leaves_no_new_row() {
    let ctx = MockContext::new(|m| {
        m.expect_prepare_persistent_volume()
            .times(1)
            .returning(|_, _| Box::pin(async { Ok(()) }));
        m.expect_create_container_from_template()
            .times(1)
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let token = ctx.login_admin().await;
    let template_id = create_persistent_template(&ctx, &token, "persist-tx", Some("/mnt/ow_dir")).await;

    let first = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
        "persistence": "use_persistent",
    }), &token).await;
    assert_eq!(first.status(), 200, "body: {:?}", first.text().await);

    // The second persistent launch for the same (template, owner) is refused
    // by the in-transaction uniqueness check.
    let second = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
        "persistence": "use_persistent",
    }), &token).await;
    assert_eq!(second.status(), 409);
    let body: serde_json::Value = second.json().await.unwrap();
    assert!(body["error"].as_str().unwrap().contains("persistent storage already exists"));

    // The transaction rolled back: exactly the first instance remains.
    let list = ctx.get_auth("/api/instances", &token).await;
    let list_body: serde_json::Value = list.json().await.unwrap();
    let instances = list_body["instances"].as_array().unwrap();
    assert_eq!(instances.len(), 1, "the conflicting launch must not leave a row behind");
    assert_eq!(instances[0]["mount_persistent"], true);
}

// ── Ticket 08: flag-gated routes ─────────────────────────────────
// Every route gate resolves from the effective-context flags (not the legacy
// role): user management → can_manage_users; template create → can_create_template
// (edit/delete also owned); instance lifecycle → ownership / same-group scope /
// admin; raw Docker → can_manage_docker; registry → can_manage_registry; admin
// settings → is_admin. Each is exercised as permitted → 2xx / denied → 403.

/// Create a plain user through the admin API and log in as them. Returns
/// (user_id, auth token).
async fn create_user_and_token(
    ctx: &MockContext,
    admin_token: &str,
    username: &str,
) -> (String, String) {
    let resp = ctx.post_auth("/api/users", &serde_json::json!({
        "username": username, "password": "password123"
    }), admin_token).await;
    assert_eq!(resp.status(), 200, "failed to create user {}", username);
    let user_id = resp.json::<serde_json::Value>().await.unwrap()["user"]["id"]
        .as_str().unwrap().to_string();
    let token = ctx.login_user(username, "password123").await;
    (user_id, token)
}

/// Insert a `groups` row with exactly the given flag values and return its id.
async fn seed_group(
    ctx: &MockContext,
    name: &str,
    can_create_template: bool,
    can_manage_users: bool,
    can_manage_group_instances: bool,
    can_manage_docker: bool,
    can_manage_registry: bool,
) -> String {
    seed_group_kind(
        ctx,
        name,
        None,
        can_create_template,
        can_manage_users,
        can_manage_group_instances,
        can_manage_docker,
        can_manage_registry,
    )
    .await
}

/// Like `seed_group`, but with an explicit `kind` (`"manager"` raises the
/// member's tier to 1, which the instance-tier guardrail requires for a
/// group-scoped manager to control a tier-0 owner's instances).
async fn seed_group_kind(
    ctx: &MockContext,
    name: &str,
    kind: Option<&str>,
    can_create_template: bool,
    can_manage_users: bool,
    can_manage_group_instances: bool,
    can_manage_docker: bool,
    can_manage_registry: bool,
) -> String {
    use openworkspace_api::db::group;
    use sea_orm::{ActiveModelTrait, Set};
    let id = uuid::Uuid::new_v4();
    group::ActiveModel {
        id: Set(id),
        name: Set(name.to_string()),
        description: Set(None),
        kind: Set(kind.map(|k| k.to_string())),
        can_create_template: Set(can_create_template),
        can_manage_users: Set(can_manage_users),
        can_manage_group_instances: Set(can_manage_group_instances),
        can_manage_docker: Set(can_manage_docker),
        can_manage_registry: Set(can_manage_registry),
        max_instances: Set(Some(4)),
        ..Default::default()
    }
    .insert(&ctx.db)
    .await
    .unwrap();
    id.to_string()
}

async fn add_group_member(ctx: &MockContext, user_id: &str, group_id: &str) {
    use openworkspace_api::db::user_group;
    use sea_orm::{ActiveModelTrait, Set};
    user_group::ActiveModel {
        user_id: Set(user_id.parse().unwrap()),
        group_id: Set(group_id.parse().unwrap()),
    }
    .insert(&ctx.db)
    .await
    .unwrap();
}

/// Create a user, seed a fresh group carrying exactly the given flags, and put
/// the user in it. Returns (user_id, login token).
async fn create_flagged_user(
    ctx: &MockContext,
    admin_token: &str,
    username: &str,
    can_create_template: bool,
    can_manage_users: bool,
    can_manage_group_instances: bool,
    can_manage_docker: bool,
    can_manage_registry: bool,
) -> (String, String) {
    let (user_id, token) = create_user_and_token(ctx, admin_token, username).await;
    let group_id = seed_group(
        ctx,
        &format!("grp-{}", username),
        can_create_template,
        can_manage_users,
        can_manage_group_instances,
        can_manage_docker,
        can_manage_registry,
    ).await;
    add_group_member(ctx, &user_id, &group_id).await;
    (user_id, token)
}

#[tokio::test]
async fn test_gate_user_management_requires_can_manage_users() {
    let ctx = MockContext::new(|_| {}).await;
    let admin_token = ctx.login_admin().await;

    let (uma_id, uma_token) = create_flagged_user(
        &ctx, &admin_token, "gate_users_on",
        false, true, false, false, false,
    ).await;
    // Deleting a user needs the actor's tier strictly above the target's, so
    // the manager also rides a manager-kind group (tier 1).
    let uma_mgr = seed_group_kind(&ctx, "grp-mgr-users", Some("manager"), false, false, false, false, false).await;
    add_group_member(&ctx, &uma_id, &uma_mgr).await;
    let (plain_id, plain_token) = create_user_and_token(&ctx, &admin_token, "gate_users_off").await;

    // Permitted → 2xx: list, create, and read any user.
    assert_eq!(ctx.get_auth("/api/users", &uma_token).await.status(), 200);
    assert_eq!(
        ctx.post_auth("/api/users", &serde_json::json!({"username": "gate_users_managed", "password": "pass123"}), &uma_token).await.status(),
        200
    );
    assert_eq!(ctx.get_auth(&format!("/api/users/{}", plain_id), &uma_token).await.status(), 200);

    // A tier-1 user manager may delete a tier-0 plain user (tier guardrail).
    let managed_id = openworkspace_api::db::UserRepository::new(&ctx.db)
        .find_by_username("gate_users_managed").await.unwrap().unwrap().id;
    assert_eq!(ctx.delete_auth(&format!("/api/users/{}", managed_id), &uma_token).await.status(), 204);

    // Denied → 403 for a user with no flag.
    assert_eq!(ctx.get_auth("/api/users", &plain_token).await.status(), 403);
    assert_eq!(
        ctx.post_auth("/api/users", &serde_json::json!({"username": "gate_users_nope", "password": "pass123"}), &plain_token).await.status(),
        403
    );
    assert_eq!(ctx.delete_auth(&format!("/api/users/{}", uma_id), &plain_token).await.status(), 403);

    // A plain user may still read their own profile.
    assert_eq!(ctx.get_auth(&format!("/api/users/{}", plain_id), &plain_token).await.status(), 200);
}

#[tokio::test]
async fn test_gate_template_create_requires_can_create_template() {
    let ctx = MockContext::new(|_| {}).await;
    let admin_token = ctx.login_admin().await;

    let (_, creator_token) = create_flagged_user(
        &ctx, &admin_token, "gate_tpl_creator",
        true, false, false, false, false,
    ).await;
    let (_, plain_token) = create_user_and_token(&ctx, &admin_token, "gate_tpl_plain").await;

    // Permitted → 2xx.
    let resp = ctx.post_auth("/api/templates", &serde_json::json!({
        "name": "gate-tpl-ok", "image": "busybox:1"
    }), &creator_token).await;
    assert_eq!(resp.status(), 200, "body: {:?}", resp.text().await);

    // Denied → 403.
    let resp = ctx.post_auth("/api/templates", &serde_json::json!({
        "name": "gate-tpl-denied", "image": "busybox:1"
    }), &plain_token).await;
    assert_eq!(resp.status(), 403);

    // Templates are a global browsable catalog: a plain user may list them.
    assert_eq!(ctx.get_auth("/api/templates", &plain_token).await.status(), 200);
}

#[tokio::test]
async fn test_gate_template_edit_delete_owner_only() {
    let ctx = MockContext::new(|_| {}).await;
    let admin_token = ctx.login_admin().await;

    let (creator_a_id, token_a) = create_flagged_user(
        &ctx, &admin_token, "gate_tpl_owner",
        true, false, false, false, false,
    ).await;
    let (_, token_b) = create_flagged_user(
        &ctx, &admin_token, "gate_tpl_other",
        true, false, false, false, false,
    ).await;

    let template_id = create_template_only(&ctx, &token_a, "gate-owned-template").await;
    let edit_body = serde_json::json!({
        "name": "gate-owned-template", "image": "busybox:1",
        "cores": 2, "memory": 4294967296i64, "gpu_count": 0,
        "remote_type": "kasmvnc", "container_runtime": "runc",
        "run_config": {}, "exec_config": {}, "volume_mappings": {},
        "timeout_action": "remove", "keep_time_action": "pause",
    });

    // Another creator (also can_create_template) may not edit/delete → 403.
    assert_eq!(ctx.put_auth(&format!("/api/templates/{}", template_id), &edit_body, &token_b).await.status(), 403);
    assert_eq!(ctx.delete_auth(&format!("/api/templates/{}", template_id), &token_b).await.status(), 403);

    // The owner may edit → 2xx, and delete → 204.
    assert_eq!(ctx.put_auth(&format!("/api/templates/{}", template_id), &edit_body, &token_a).await.status(), 200);

    // Re-create for the admin-legacy check: admin edits/deletes any template.
    let second_id = create_template_only(&ctx, &token_a, "gate-owned-template-2").await;
    assert_eq!(ctx.put_auth(&format!("/api/templates/{}", second_id), &edit_body, &admin_token).await.status(), 200);
    assert_eq!(ctx.delete_auth(&format!("/api/templates/{}", second_id), &admin_token).await.status(), 204);

    let _ = creator_a_id;
}

#[tokio::test]
async fn test_gate_instance_lifecycle_same_group_scope() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
        m.expect_stop_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
        m.expect_remove_container_by_id()
            .returning(|_| Box::pin(async { Ok(()) }));
        m.expect_remove_network()
            .returning(|_| Box::pin(async { Ok(()) }));
    }).await;
    let admin_token = ctx.login_admin().await;

    // Owner and manager share group G (can_create_template + group scope);
    // the manager also rides a manager-kind group so the tier guardrail (actor
    // tier > owner tier) lets them control the tier-0 owner's instances.
    // The outsider lives in group H with no flags.
    let (owner_id, owner_token) = create_user_and_token(&ctx, &admin_token, "gate_owner").await;
    let (manager_id, manager_token) = create_user_and_token(&ctx, &admin_token, "gate_manager").await;
    let (outsider_id, outsider_token) = create_user_and_token(&ctx, &admin_token, "gate_outsider").await;
    let group_g = seed_group(&ctx, "grp-g", true, false, true, false, false).await;
    let mgr_kind = seed_group_kind(&ctx, "grp-mgr", Some("manager"), false, false, false, false, false).await;
    add_group_member(&ctx, &owner_id, &group_g).await;
    add_group_member(&ctx, &manager_id, &group_g).await;
    add_group_member(&ctx, &manager_id, &mgr_kind).await;
    let group_h = seed_group(&ctx, "grp-h", false, false, false, false, false).await;
    add_group_member(&ctx, &outsider_id, &group_h).await;

    // Owner creates a template (group whitelist grants launch) and launches.
    let template_id = create_template_only(&ctx, &owner_token, "gate-scope-template").await;
    grant_group_template(&ctx, &group_g, &template_id).await;
    let launch = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id
    }), &owner_token).await;
    assert_eq!(launch.status(), 200, "body: {:?}", launch.text().await);
    let instance_id = launch.json::<serde_json::Value>().await.unwrap()["instance"]["id"]
        .as_str().unwrap().to_string();

    // The instance JSON carries the owner's group ids (pinned contract),
    // which now includes the default User group alongside the seeded group.
    let owner_view = ctx.get_auth(&format!("/api/instances/{}", instance_id), &owner_token).await;
    let body: serde_json::Value = owner_view.json().await.unwrap();
    assert!(
        body["instance"]["owner_group_ids"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!(group_g)),
        "owner_group_ids must include the seeded group: {}",
        body
    );

    // Owner keeps their powers.
    assert_eq!(ctx.get_auth(&format!("/api/instances/{}", instance_id), &owner_token).await.status(), 200);

    // A same-group manager may read.
    let manager_view = ctx.get_auth(&format!("/api/instances/{}", instance_id), &manager_token).await;
    assert_eq!(manager_view.status(), 200);

    // The outsider shares no group with the owner → 403 on every lifecycle op.
    assert_eq!(ctx.get_auth(&format!("/api/instances/{}", instance_id), &outsider_token).await.status(), 403);
    assert_eq!(ctx.post_auth(&format!("/api/instances/{}/start", instance_id), &serde_json::json!({}), &outsider_token).await.status(), 403);
    assert_eq!(ctx.post_auth(&format!("/api/instances/{}/stop", instance_id), &serde_json::json!({}), &outsider_token).await.status(), 403);
    assert_eq!(ctx.delete_auth(&format!("/api/instances/{}", instance_id), &outsider_token).await.status(), 403);

    // A same-group manager may also delete.
    let del = ctx.delete_auth(&format!("/api/instances/{}", instance_id), &manager_token).await;
    assert_eq!(del.status(), 204, "body: {:?}", del.text().await);
}

#[tokio::test]
async fn test_gate_group_manager_list_includes_same_group_instances() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;
    let admin_token = ctx.login_admin().await;

    let (owner_id, owner_token) = create_user_and_token(&ctx, &admin_token, "gate_list_owner").await;
    let (manager_id, manager_token) = create_user_and_token(&ctx, &admin_token, "gate_list_manager").await;
    let (_, outsider_token) = create_user_and_token(&ctx, &admin_token, "gate_list_outsider").await;
    let group_g = seed_group(&ctx, "grp-list-g", true, false, true, false, false).await;
    let mgr_kind = seed_group_kind(&ctx, "grp-list-mgr", Some("manager"), false, false, false, false, false).await;
    add_group_member(&ctx, &owner_id, &group_g).await;
    add_group_member(&ctx, &manager_id, &group_g).await;
    add_group_member(&ctx, &manager_id, &mgr_kind).await;

    let template_id = create_template_only(&ctx, &owner_token, "gate-list-template").await;
    grant_group_template(&ctx, &group_g, &template_id).await;
    let launch = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id
    }), &owner_token).await;
    assert_eq!(launch.status(), 200, "body: {:?}", launch.text().await);
    let instance_id = launch.json::<serde_json::Value>().await.unwrap()["instance"]["id"]
        .as_str().unwrap().to_string();

    // The same-group manager sees the owner's instance in the list.
    let list = ctx.get_auth("/api/instances", &manager_token).await;
    let body: serde_json::Value = list.json().await.unwrap();
    assert!(
        body["instances"].as_array().unwrap().iter().any(|i| i["id"] == instance_id),
        "group manager list must include same-group instances"
    );

    // An unrelated user's list only has their own instances (here: none).
    let list = ctx.get_auth("/api/instances", &outsider_token).await;
    let body: serde_json::Value = list.json().await.unwrap();
    assert!(body["instances"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn test_gate_docker_raw_requires_can_manage_docker() {
    let ctx = MockContext::new(|m| {
        m.expect_list_containers()
            .returning(|_| Box::pin(async { Ok(Vec::new()) }));
        m.expect_create_container()
            .returning(|_, _| Box::pin(async { Ok("cid-123".to_string()) }));
    }).await;
    let admin_token = ctx.login_admin().await;

    let (_, docker_token) = create_flagged_user(
        &ctx, &admin_token, "gate_docker_on",
        false, false, false, true, false,
    ).await;
    let (_, plain_token) = create_user_and_token(&ctx, &admin_token, "gate_docker_off").await;

    // Permitted → 2xx.
    assert_eq!(ctx.get_auth("/api/docker/containers", &docker_token).await.status(), 200);
    assert_eq!(
        ctx.post_auth("/api/docker/containers/create", &serde_json::json!({"name": "n", "image": "busybox:1"}), &docker_token).await.status(),
        200
    );

    // Denied → 403.
    assert_eq!(ctx.get_auth("/api/docker/containers", &plain_token).await.status(), 403);
    assert_eq!(
        ctx.post_auth("/api/docker/containers/create", &serde_json::json!({"name": "n", "image": "busybox:1"}), &plain_token).await.status(),
        403
    );
}

#[tokio::test]
async fn test_gate_registry_requires_can_manage_registry() {
    let ctx = MockContext::new(|_| {}).await;
    let admin_token = ctx.login_admin().await;

    let (_, reg_token) = create_flagged_user(
        &ctx, &admin_token, "gate_reg_on",
        false, false, false, false, true,
    ).await;
    let (_, plain_token) = create_user_and_token(&ctx, &admin_token, "gate_reg_off").await;

    // Permitted → 2xx.
    assert_eq!(ctx.get_auth("/api/registry/url", &reg_token).await.status(), 200);
    assert_eq!(
        ctx.put_auth("/api/registry/url", &serde_json::json!({"url": "https://example.com/registry.json"}), &reg_token).await.status(),
        200
    );

    // Denied → 403.
    assert_eq!(ctx.get_auth("/api/registry/url", &plain_token).await.status(), 403);
    assert_eq!(
        ctx.put_auth("/api/registry/url", &serde_json::json!({"url": "https://example.com/registry.json"}), &plain_token).await.status(),
        403
    );
}

#[tokio::test]
async fn test_gate_admin_settings_requires_system_admin() {
    let ctx = MockContext::new(|_| {}).await;
    let admin_token = ctx.login_admin().await;

    // A fully-flagged group member is NOT a system admin.
    let (_, flagged_token) = create_flagged_user(
        &ctx, &admin_token, "gate_all_flags",
        true, true, true, true, true,
    ).await;

    let settings_body = serde_json::json!({
        "host_instance_limit": 0,
    });

    // Admin-group membership (the seeded admin) → 2xx.
    assert_eq!(ctx.get_auth("/api/admin/settings", &admin_token).await.status(), 200);
    assert_eq!(ctx.put_auth("/api/admin/settings", &settings_body, &admin_token).await.status(), 200);

    // Group flags alone do not unlock admin settings → 403.
    assert_eq!(ctx.get_auth("/api/admin/settings", &flagged_token).await.status(), 403);
    assert_eq!(ctx.put_auth("/api/admin/settings", &settings_body, &flagged_token).await.status(), 403);
}
