#![cfg(feature = "docker")]

// Ticket 02 — integration: real cross-process port + subnet flock E2E.
//
// Two independent API server processes (the actual compiled binary), each with
// its own database, both leaving `PORT_LOCK_DIR` unset so they resolve the same
// per-UID lock directory by construction (spec §20/§47). Both launch a real
// container through the HTTP API at the same time; the observable outcomes —
// both launches succeed, distinct host ports, distinct `/30` instance subnets,
// both containers running with their ports bound, and zero residual runsc after
// deletion — are the cross-process arbitration acceptance signal that the mock
// harness cannot reach.

mod common;

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use bollard::container::{
    InspectContainerOptions, ListContainersOptions, RemoveContainerOptions, StopContainerOptions,
};
use reqwest::Client;
use serde_json::json;

use common::TestContext;

static DB_COUNTER: AtomicU32 = AtomicU32::new(0);

const JWT_SECRET: &str = "test-secret-key-for-testing";
const CONTAINER_SESSION_PORT: &str = "6901/tcp";

/// The compiled `openworkspace-api` binary. Cargo sets `CARGO_BIN_EXE_<name>`
/// for integration tests; the fallback covers manual invocations.
fn api_binary_path() -> PathBuf {
    if let Some(p) = std::env::var_os("CARGO_BIN_EXE_openworkspace-api") {
        return PathBuf::from(p);
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("target/debug/openworkspace-api")
}

fn free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Create a fresh, empty database for one spawned API process. The process
/// itself runs migrations + admin seed on startup (mirroring `main.rs`), so
/// here we only create the database.
async fn create_test_db() -> String {
    common::ensure_pg().await;
    let counter = DB_COUNTER.fetch_add(1, Ordering::Relaxed);
    let db_name = format!("test_2proc_{}_{:04}", std::process::id(), counter);
    let base_url = common::pg_base_url();

    let (client, conn) = tokio_postgres::connect(&base_url, tokio_postgres::NoTls)
        .await
        .expect("failed to connect to test postgres");
    tokio::spawn(async move {
        let _ = conn.await;
    });
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

    db_name
}

/// Spawn one real API process and hand its child handle to the guard. Returns
/// the base URL to reach it. `PORT_LOCK_DIR` is intentionally not set: the
/// process resolves the shared per-UID lock directory by construction. The
/// settings runtime is left at its default (`runsc`); templates also default
/// to `runsc`, so the launched instances exercise the exact sandbox scenario
/// from the problem statement.
async fn spawn_server(db_name: &str, port: u16, log_dir: &Path, guard: &mut ServersGuard) -> String {
    let bin = api_binary_path();
    assert!(
        bin.exists(),
        "API binary not found at {} — build it first",
        bin.display()
    );
    let db_url = common::pg_url(db_name);
    let base_url = format!("http://127.0.0.1:{}", port);

    let out_path = log_dir.join(format!("{}-{}.out.log", db_name, port));
    let err_path = log_dir.join(format!("{}-{}.err.log", db_name, port));
    let out = std::fs::File::create(&out_path).unwrap();
    let err = std::fs::File::create(&err_path).unwrap();

    let child = tokio::process::Command::new(&bin)
        .env("DATABASE_URL", db_url)
        .env("JWT_SECRET", JWT_SECRET)
        .env("ADMIN_PASSWORD", "admin")
        .env("SERVER_HOST", "127.0.0.1")
        .env("SERVER_PORT", port.to_string())
        .env_remove("PORT_LOCK_DIR")
        .current_dir(std::env::temp_dir())
        .stdout(Stdio::from(out))
        .stderr(Stdio::from(err))
        .spawn()
        .expect("failed to spawn API server");

    guard.take_child(child);
    base_url
}

async fn wait_until_ready(base_url: &str) {
    let client = Client::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
    loop {
        let healthy = client
            .get(format!("{}/health", base_url))
            .send()
            .await
            .ok()
            .map(|r| r.status().is_success())
            .unwrap_or(false);
        if healthy {
            let login = client
                .post(format!("{}/api/auth/login", base_url))
                .json(&json!({ "username": "admin", "password": "admin" }))
                .send()
                .await;
            if let Ok(resp) = login
                && resp.status().is_success() {
                    return;
                }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "API server at {} did not become ready within 45s (logs in {})",
            base_url,
            std::env::temp_dir()
                .join(format!("ow_2proc_{}", std::process::id()))
                .display()
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    }
}

async fn login_client(base_url: &str) -> Client {
    let client = Client::builder()
        .cookie_store(true)
        .build()
        .expect("failed to build HTTP client");
    let resp = client
        .post(format!("{}/api/auth/login", base_url))
        .json(&json!({ "username": "admin", "password": "admin" }))
        .send()
        .await
        .expect("login request failed");
    assert_eq!(resp.status(), 200, "login to {} failed", base_url);
    client
}

async fn create_template(client: &Client, base_url: &str, suffix: &str) -> String {
    let name = format!("ow_test_2proc_{}_{}", std::process::id(), suffix);
    let resp = client
        .post(format!("{}/api/templates", base_url))
        .json(&json!({
            "name": name,
            "image": "busybox:1",
            "cores": 0,
            "memory": 0,
            "run_config": { "command": ["sleep", "3600"] },
        }))
        .send()
        .await
        .expect("create template request failed");
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(
        status,
        200,
        "create template failed: {}",
        serde_json::to_string_pretty(&body).unwrap()
    );
    body["template"]["id"].as_str().unwrap().to_string()
}

async fn get_host_port(client: &Client, base_url: &str, instance_id: &str) -> u16 {
    let resp = client
        .get(format!("{}/api/instances/{}", base_url, instance_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), 200, "get instance failed");
    let body: serde_json::Value = resp.json().await.unwrap();
    body["instance"]["host_port"]
        .as_i64()
        .expect("instance JSON must expose host_port")
        as u16
}

/// Read the `/30` subnet (first IPv4 IPAM config) of an instance's dedicated
/// network `ow-{instance_id}` directly from Docker. The API exposes the network
/// name but not the subnet, so the observable cross-process outcome is read via
/// the daemon the way `list_networks`/`create_network` see it.
async fn get_network_subnet(instance_id: &str) -> String {
    let docker = bollard::Docker::connect_with_local_defaults().unwrap();
    let network_name = format!("ow-{}", instance_id);
    let net = docker
        .inspect_network(&network_name, None::<bollard::network::InspectNetworkOptions<&str>>)
        .await
        .expect("instance network must exist");
    net.ipam
        .and_then(|ipam| ipam.config)
        .and_then(|config| {
            config
                .into_iter()
                .filter_map(|c| c.subnet)
                .find(|s| !s.contains(':'))
        })
        .expect("instance network must carry an IPv4 subnet")
}

/// Poll docker until the instance's container is actually running, then assert
/// the container bound its session port to the instance's allocated host port.
async fn wait_running_and_check_binding(
    base_url: &str,
    client: &Client,
    instance_id: &str,
    expect_host_port: u16,
) {
    let docker = bollard::Docker::connect_with_local_defaults().unwrap();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
    let container_id = loop {
        let resp = client
            .get(format!("{}/api/instances/{}", base_url, instance_id))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let body: serde_json::Value = resp.json().await.unwrap();
        let cid = body["instance"]["container_id"]
            .as_str()
            .expect("instance has no container_id")
            .to_string();
        let running = docker
            .inspect_container(&cid, None::<InspectContainerOptions>)
            .await
            .ok()
            .and_then(|i| i.state)
            .and_then(|s| s.running)
            .unwrap_or(false);
        if running {
            break cid;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "container for instance {} did not become running within 45s",
            instance_id
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
    };

    let inspect = docker
        .inspect_container(&container_id, None::<InspectContainerOptions>)
        .await
        .expect("container exists after running");
    let ports = inspect
        .network_settings
        .and_then(|ns| ns.ports)
        .expect("container has port bindings");
    let bound = ports
        .get(CONTAINER_SESSION_PORT)
        .and_then(|b| b.as_ref())
        .and_then(|b| b.first())
        .and_then(|b| b.host_port.clone())
        .expect("container session port is bound to a host port");
    assert_eq!(
        bound.parse::<u16>().unwrap(),
        expect_host_port,
        "container port binding must equal the instance's allocated host_port"
    );
}

/// Count of `runsc` sandbox/gofer processes whose container id is no longer
/// tracked by Docker — the repo's authoritative orphan/leak signal (same
/// matching as `scripts/cleanup.sh` `sweep_orphans`). Sibling tests in the
/// parallel suite legitimately run runsc containers; their sandbox processes
/// stay tracked by Docker, so they are never counted, making this assertion
/// deterministic even under full-suite parallelism. `usize::MAX` on failure to
/// inspect the host/Docker makes an inability to verify fail loudly.
async fn orphan_runsc_count() -> usize {
    let Ok(docker) = bollard::Docker::connect_with_local_defaults() else {
        return usize::MAX;
    };
    let active = match docker
        .list_containers(Some(ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        }))
        .await
    {
        Ok(list) => list
            .into_iter()
            .filter_map(|c| c.id)
            .collect::<std::collections::HashSet<String>>(),
        Err(_) => return usize::MAX,
    };

    let Ok(entries) = std::fs::read_dir("/proc") else {
        return usize::MAX;
    };
    let mut count = 0;
    for entry in entries.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let comm = std::fs::read_to_string(format!("/proc/{}/comm", pid)).unwrap_or_default();
        match comm.trim() {
            "runsc-sandbox" | "runsc-gofer" => {}
            _ => continue,
        }
        let cmdline = std::fs::read(format!("/proc/{}/cmdline", pid)).unwrap_or_default();
        let cmdline = String::from_utf8_lossy(&cmdline);
        let tokens: Vec<&str> = cmdline.split_whitespace().collect();
        let Some(cid) = tokens.into_iter().rev().find_map(|tok| {
            let t = tok.trim();
            (t.len() == 64 && t.bytes().all(|b| b.is_ascii_hexdigit())).then(|| t.to_string())
        }) else {
            // Not attributable to any container id — mirror `sweep_orphans`, which
            // only ever kills a process it can attribute to a (missing) container.
            continue;
        };
        if !active.contains(&cid) {
            count += 1;
        }
    }
    count
}

/// Poll until no runsc sandbox/gofer is orphaned (the leak signal), bounded.
/// Instances run under the template-default `runsc` runtime, so their sandboxes
/// are torn down when the API removes the container; a brief post-delete
/// teardown lag must not false-fail, while a genuine orphan never clears.
async fn wait_for_no_orphan_runsc() -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    loop {
        if orphan_runsc_count().await == 0 {
            return true;
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn drop_databases(db_names: &[String]) {
    let base = common::pg_base_url();
    if let Ok((client, conn)) = tokio_postgres::connect(&base, tokio_postgres::NoTls).await {
        tokio::spawn(conn);
        for db in db_names {
            let _ = client
                .execute(
                    &format!(
                        "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{}'",
                        db
                    )[..],
                    &[],
                )
                .await;
            let _ = client
                .execute(&format!("DROP DATABASE IF EXISTS \"{}\"", db)[..], &[])
                .await;
        }
    }
}

/// Best-effort teardown of anything the API `DELETE` does not remove: orphan
/// containers (name = `{template}-{n}`) and the instance /30 networks
/// (`ow-<instance_id>`), which instance deletion intentionally preserves.
async fn remove_leftover_containers(template_prefixes: &[String], instance_ids: &[String]) {
    let Ok(docker) = bollard::Docker::connect_with_local_defaults() else {
        return;
    };
    for id in instance_ids {
        let _ = docker
            .remove_network(&openworkspace_api::instance_net::network_name(id))
            .await;
    }
    let Ok(containers) = docker
        .list_containers(Some(ListContainersOptions::<String> {
            all: true,
            ..Default::default()
        }))
        .await
    else {
        return;
    };
    for c in containers {
        let matched = c.names.map(|names| {
            names.iter().any(|n| {
                let name = n.trim_start_matches('/');
                template_prefixes
                    .iter()
                    .any(|t| name.starts_with(&format!("{}-", t)))
            })
        });
        if matched == Some(true) {
            let id = c.id.unwrap_or_default();
            let _ = docker
                .stop_container(&id, None::<StopContainerOptions>)
                .await;
            let _ = docker
                .remove_container(
                    &id,
                    Some(RemoveContainerOptions {
                        force: true,
                        ..Default::default()
                    }),
                )
                .await;
        }
    }
}

/// Kills the spawned servers and drops their databases + orphan containers even
/// when the test panics mid-flight, so a failed run leaves no residue.
struct ServersGuard {
    children: Vec<tokio::process::Child>,
    db_names: Vec<String>,
    template_prefixes: Vec<String>,
    instance_ids: Vec<String>,
}

impl ServersGuard {
    fn new() -> Self {
        Self {
            children: Vec::new(),
            db_names: Vec::new(),
            template_prefixes: Vec::new(),
            instance_ids: Vec::new(),
        }
    }

    fn take_child(&mut self, child: tokio::process::Child) {
        self.children.push(child);
    }

    fn track_db(&mut self, db_name: String) {
        self.db_names.push(db_name);
    }

    fn track_template(&mut self, name: String) {
        self.template_prefixes.push(name);
    }

    fn track_instance(&mut self, id: String) {
        self.instance_ids.push(id);
    }
}

impl Drop for ServersGuard {
    fn drop(&mut self) {
        for child in &mut self.children {
            let _ = child.start_kill();
        }
        let children = std::mem::take(&mut self.children);
        let db_names = std::mem::take(&mut self.db_names);
        let template_prefixes = std::mem::take(&mut self.template_prefixes);
        let instance_ids = std::mem::take(&mut self.instance_ids);
        let _ = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                for mut child in children {
                    let _ = child.wait().await;
                }
                drop_databases(&db_names).await;
                remove_leftover_containers(&template_prefixes, &instance_ids).await;
            });
        });
    }
}

/// Exercising every `TestContext` method keeps the shared `common` module's
/// helpers alive in this binary (repo convention — zero dead-code warnings).
#[tokio::test]
async fn test_context_helpers() {
    let ctx = TestContext::new().await;
    let _ = ctx.login_user("admin", "admin").await;
    let _ = ctx.get("/health").await;
    let _ = ctx.post("/health", &json!({})).await;
    let _ = ctx.put("/health", &json!({})).await;
    let _ = ctx.delete("/health").await;
    let _ = ctx.login_token().await;
}

#[tokio::test]
async fn test_two_process_concurrent_launch_arbitrates_host_ports() {
    common::ensure_network().await;

    let log_dir = std::env::temp_dir().join(format!("ow_2proc_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&log_dir);
    std::fs::create_dir_all(&log_dir).unwrap();

    let mut guard = ServersGuard::new();

    // Two independent API processes, each with its own database. `PORT_LOCK_DIR`
    // is intentionally unset: both resolve the same per-UID lock directory by
    // construction — the cross-process seam under test.
    let db_a = create_test_db().await;
    let db_b = create_test_db().await;
    guard.track_db(db_a.clone());
    guard.track_db(db_b.clone());

    let port_a = free_port();
    let port_b = free_port();
    assert_ne!(port_a, port_b, "free-port probe returned the same port twice");

    let base_url_a = spawn_server(&db_a, port_a, &log_dir, &mut guard).await;
    let base_url_b = spawn_server(&db_b, port_b, &log_dir, &mut guard).await;

    wait_until_ready(&base_url_a).await;
    wait_until_ready(&base_url_b).await;

    let client_a = login_client(&base_url_a).await;
    let client_b = login_client(&base_url_b).await;

    // Templates live in each server's own DB, so each process gets its own.
    let template_a = create_template(&client_a, &base_url_a, "proc_a").await;
    let template_b = create_template(&client_b, &base_url_b, "proc_b").await;
    guard.track_template(format!("ow_test_2proc_{}_proc_a", std::process::id()));
    guard.track_template(format!("ow_test_2proc_{}_proc_b", std::process::id()));

    // Fire both launches concurrently through the two real processes.
    let (ra, rb) = tokio::join!(
        client_a
            .post(format!("{}/api/instances", base_url_a))
            .json(&json!({ "template_id": template_a }))
            .send(),
        client_b
            .post(format!("{}/api/instances", base_url_b))
            .json(&json!({ "template_id": template_b }))
            .send(),
    );
    let ra = ra.expect("launch request to server A failed");
    let rb = rb.expect("launch request to server B failed");
    let ra_status = ra.status();
    let rb_status = rb.status();
    let ba: serde_json::Value = ra.json().await.unwrap();
    let bb: serde_json::Value = rb.json().await.unwrap();
    assert_eq!(
        ra_status,
        200,
        "launch via server A failed: {}",
        serde_json::to_string_pretty(&ba).unwrap()
    );
    assert_eq!(
        rb_status,
        200,
        "launch via server B failed: {}",
        serde_json::to_string_pretty(&bb).unwrap()
    );
    assert_eq!(ba["instance"]["status"].as_str(), Some("starting"));
    assert_eq!(bb["instance"]["status"].as_str(), Some("starting"));

    let id_a = ba["instance"]["id"].as_str().unwrap().to_string();
    let id_b = bb["instance"]["id"].as_str().unwrap().to_string();
    guard.track_instance(id_a.clone());
    guard.track_instance(id_b.clone());

    // Distinct host ports — the observable cross-process arbitration outcome.
    let host_port_a = get_host_port(&client_a, &base_url_a, &id_a).await;
    let host_port_b = get_host_port(&client_b, &base_url_b, &id_b).await;
    assert_ne!(
        host_port_a, host_port_b,
        "two independent API processes allocated the same host port ({})",
        host_port_a
    );

    // Distinct /30 subnets — the flock arbitration outcome for instance
    // networks, read straight from Docker (the API exposes the network name,
    // not the subnet).
    let subnet_a = get_network_subnet(&id_a).await;
    let subnet_b = get_network_subnet(&id_b).await;
    assert_ne!(
        subnet_a, subnet_b,
        "two independent API processes allocated the same /30 subnet ({})",
        subnet_a
    );
    for subnet in [&subnet_a, &subnet_b] {
        let addr: std::net::Ipv4Addr = subnet.split('/').next().unwrap().parse().unwrap();
        let octets = addr.octets();
        assert_eq!(octets[0..2], [10, 200], "subnet {} must stay in the base range", subnet);
        assert_eq!(octets[3] % 4, 0, "subnet {} must be an aligned /30 block", subnet);
    }

    // Both containers actually start and bind their session port to the
    // allocated host port.
    wait_running_and_check_binding(&base_url_a, &client_a, &id_a, host_port_a).await;
    wait_running_and_check_binding(&base_url_b, &client_b, &id_b, host_port_b).await;

    // Clean up both instances through their owning servers.
    let da = client_a
        .delete(format!("{}/api/instances/{}", base_url_a, id_a))
        .send()
        .await
        .unwrap();
    let db_resp = client_b
        .delete(format!("{}/api/instances/{}", base_url_b, id_b))
        .send()
        .await
        .unwrap();
    assert_eq!(da.status(), 204, "delete via server A failed");
    assert_eq!(db_resp.status(), 204, "delete via server B failed");

    // The instances run under the template-default `runsc` runtime (production
    // default), so this is the exact scenario from the problem statement: two
    // concurrent runsc launches must not orphan sandbox processes. Poll for
    // zero orphaned runsc — sibling tests' tracked runsc containers are never
    // counted, so this is deterministic under full-suite parallelism.
    assert!(
        wait_for_no_orphan_runsc().await,
        "residual runsc processes leaked after both instances were deleted"
    );
}
