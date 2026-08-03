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
    std::io::Error::new(std::io::ErrorKind::Other, msg).into()
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
            container_runtime: "docker".to_string(),
            host_gateway_ip: "172.17.0.1".to_string(),
            host_port_start: 10000,
            host_port_end: 20000,
            instance_net_base: "10.200.0.0/16".to_string(),
            instance_dns: "8.8.8.8,1.1.1.1".to_string(),
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
            network_lock: Arc::new(tokio::sync::Mutex::new(())),
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
    assert_eq!(order[1], format!("create_network|{}|10.200.0.0/30|10.200.0.1", expected_net));
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
    let list_calls_for_mock = list_calls.clone();
    let create_calls_for_mock = create_calls.clone();
    let created_networks_for_mock = created_networks.clone();
    let captured_network_for_mock = captured_network.clone();

    let ctx = MockContext::new(move |m| {
        let list_calls = list_calls_for_mock.clone();
        m.expect_list_networks()
            .returning(move || {
                let call = list_calls.fetch_add(1, Ordering::Relaxed);
                if call == 0 {
                    // Both launches see the same empty snapshot.
                    Box::pin(async { Ok(Vec::new()) })
                } else {
                    // After the overlap, the concurrent launch's network is visible.
                    Box::pin(async {
                        Ok(vec![NetworkInfo {
                            name: "ow-other".to_string(),
                            subnet: Some("10.200.0.0/30".to_string()),
                        }])
                    })
                }
            });
        let create_calls = create_calls_for_mock.clone();
        let created_networks = created_networks_for_mock.clone();
        m.expect_create_network()
            .returning(move |name, subnet, gateway| {
                let call = create_calls.fetch_add(1, Ordering::Relaxed);
                created_networks.lock().unwrap().push(format!("{}|{}|{}", name, subnet, gateway));
                if call == 0 {
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
    assert_eq!(first[1], "10.200.0.0/30", "first attempt takes the lowest free block");
    assert_eq!(first[2], "10.200.0.1");
    assert_ne!(second[1], first[1], "re-allocation must pick a different subnet");
    let second_net: std::net::Ipv4Addr = second[1].split('/').next().unwrap().parse().unwrap();
    let octets = second_net.octets();
    assert_eq!(octets[0..2], [10, 200], "retry must stay inside the base range");
    assert_eq!(octets[3] % 4, 0, "retry must pick an aligned /30 block");
    assert_ne!(second_net, std::net::Ipv4Addr::new(10, 200, 0, 0), "retry must skip the subnet the concurrent launch took");
    assert_eq!(captured_network.lock().unwrap().as_deref(), Some(expected_net.as_str()));
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

    let (_, instance_id_b) = create_config_and_instance(&ctx, &token, "free-port-b").await;
    let id_b: uuid::Uuid = instance_id_b.parse().unwrap();
    let port_b = repo.find_by_id(id_b).await.unwrap().unwrap().host_port.unwrap();
    assert_eq!(port_a, port_b, "deleted instance's port must be reusable");
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
    assert!(body["instances"].as_array().unwrap().len() >= 1);
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
            .withf(|_, _, config, _, _| config.runtime == Some("docker".to_string()))
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
    ctx.post_auth("/api/users", &serde_json::json!({
        "username": "hb-owner", "password": "pass123"
    }), &admin_token).await;
    ctx.post_auth("/api/users", &serde_json::json!({
        "username": "hb-intruder", "password": "pass123"
    }), &admin_token).await;

    let owner_token = ctx.login_user("hb-owner", "pass123").await;
    let intruder_token = ctx.login_user("hb-intruder", "pass123").await;

    let config_resp = ctx.post_auth("/api/templates", &serde_json::json!({
        "name": "heartbeat-owner", "image": "busybox:1"
    }), &admin_token).await;
    let template_id = config_resp.json::<serde_json::Value>().await.unwrap()["template"]["id"].as_str().unwrap().to_string();

    let launch_resp = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id
    }), &owner_token).await;
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
        "role": "user",
    }), &token).await;
    assert_eq!(other.status(), 200);
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

    let ensured = ensured.lock().unwrap();
    assert_eq!(
        ensured.as_slice(),
        &[format!("{}|{}", expected_path, expected_volume)],
        "start must ensure the persistent volume for the resolved path"
    );
    let captured = captured.lock().unwrap();
    assert_eq!(
        captured.as_slice(),
        &[Some(expected_volume.clone())],
        "the recreated container must mount the persistent volume"
    );

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

// ── Ticket 06: quota pre-flight gating of launch & start ─────────

/// Create a `user`-role account with a per-user instance-limit override, via
/// the admin API. Returns the new user's id.
async fn create_quota_user(
    ctx: &MockContext,
    admin_token: &str,
    username: &str,
    instance_limit: i32,
) -> String {
    let create = ctx.post_auth("/api/users", &serde_json::json!({
        "username": username,
        "password": "password123",
        "role": "user",
    }), admin_token).await;
    assert_eq!(create.status(), 200, "failed to create quota user");
    let user_id = create.json::<serde_json::Value>().await.unwrap()["user"]["id"]
        .as_str()
        .unwrap()
        .to_string();
    let set = ctx.put_auth(&format!("/api/users/{}", user_id), &serde_json::json!({
        "instance_limit": instance_limit,
    }), admin_token).await;
    assert_eq!(set.status(), 200, "failed to set quota override");
    user_id
}

/// Create a template only (no launch), returning its id.
async fn create_template_only(ctx: &MockContext, token: &str, name: &str) -> String {
    let resp = ctx.post_auth("/api/templates", &serde_json::json!({
        "name": name, "image": "busybox:1"
    }), token).await;
    resp.json::<serde_json::Value>().await.unwrap()["template"]["id"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn test_launch_rejected_by_quota_returns_409_and_leaves_no_row() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .times(1)
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let admin_token = ctx.login_admin().await;
    let template_id = create_template_only(&ctx, &admin_token, "quota-launch").await;
    create_quota_user(&ctx, &admin_token, "quota_user", 1).await;
    let user_token = ctx.login_user("quota_user", "password123").await;

    // The user's first launch fits the instance limit of 1.
    let first = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
    }), &user_token).await;
    assert_eq!(first.status(), 200, "body: {:?}", first.text().await);

    // The second launch is refused fail-fast with the structured quota body.
    let second = ctx.post_auth("/api/instances", &serde_json::json!({
        "template_id": template_id,
    }), &user_token).await;
    assert_eq!(second.status(), 409);
    let body: serde_json::Value = second.json().await.unwrap();
    assert_eq!(body["quota"]["scope"], "user_instance");
    assert_eq!(body["quota"]["current"], 1);
    assert_eq!(body["quota"]["limit"], 1);
    assert_eq!(body["quota"]["requested"], 1);
    assert!(body["error"].as_str().unwrap().contains("instance limit"));

    // The rejected launch created no DB row: only the first instance exists.
    let list = ctx.get_auth("/api/instances", &user_token).await;
    let list_body: serde_json::Value = list.json().await.unwrap();
    assert_eq!(list_body["instances"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_start_rejected_by_quota_leaves_instance_stopped() {
    let ctx = MockContext::new(|m| {
        m.expect_create_container_from_template()
            .times(2)
            .returning(|_, _, _, _, _| Box::pin(async { Ok("fake-container-id".to_string()) }));
    }).await;

    let admin_token = ctx.login_admin().await;
    let template_id = create_template_only(&ctx, &admin_token, "quota-restart").await;
    create_quota_user(&ctx, &admin_token, "restart_user", 1).await;
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

    // Restarting A would push the user past the limit: 409, and A stays stopped.
    let restart = ctx.post_auth(&format!("/api/instances/{}/start", a_id), &serde_json::json!({}), &user_token).await;
    assert_eq!(restart.status(), 409);
    let body: serde_json::Value = restart.json().await.unwrap();
    assert_eq!(body["quota"]["scope"], "user_instance");
    assert_eq!(body["quota"]["current"], 1);

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
    create_quota_user(&ctx, &admin_token, "rollback_user", 1).await;
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
    create_quota_user(&ctx, &admin_token, "err_user", 1).await;
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
    create_quota_user(&ctx, &admin_token, "owner_user", 1).await;
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

    // The Admin restarts A. Quota is counted for the *owner* (at its limit),
    // so this is refused with the user's quota body — not 500, and not a
    // success driven by the Admin's own (exempt) limits.
    let restart = ctx.post_auth(&format!("/api/instances/{}/start", a_id), &serde_json::json!({}), &admin_token).await;
    assert_eq!(restart.status(), 409);
    let body: serde_json::Value = restart.json().await.unwrap();
    assert_eq!(body["quota"]["scope"], "user_instance");
    assert_eq!(body["quota"]["current"], 1);
    assert_eq!(body["quota"]["limit"], 1);

    let inst = ctx.get_auth(&format!("/api/instances/{}", a_id), &admin_token).await;
    let inst_body: serde_json::Value = inst.json().await.unwrap();
    assert_eq!(inst_body["instance"]["status"], "stopped");
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
