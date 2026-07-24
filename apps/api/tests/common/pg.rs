use std::sync::OnceLock;

use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{runners::AsyncRunner, ImageExt},
};

use bollard::network::CreateNetworkOptions;

const NETWORK_NAME: &str = "ow-test";

struct PgInstance {
    host: String,
    port: u16,
}

static PG: OnceLock<PgInstance> = OnceLock::new();
static PG_INIT: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
static CONTAINER_ID: OnceLock<String> = OnceLock::new();

extern "C" fn cleanup_test_containers() {
    if let Some(id) = CONTAINER_ID.get() {
        let rt = tokio::runtime::Runtime::new().expect("failed to create runtime for cleanup");
        rt.block_on(async {
            if let Ok(docker) = bollard::Docker::connect_with_local_defaults() {
                let _ = docker
                    .remove_container(
                        id,
                        Some(bollard::container::RemoveContainerOptions {
                            force: true,
                            ..Default::default()
                        }),
                    )
                    .await;
            }
        });
    }
}

pub fn pg_url(db_name: &str) -> String {
    let inst = PG.get().expect("pg not initialized — call ensure_pg() first");
    format!("postgres://postgres@{}:{}/{}", inst.host, inst.port, db_name)
}

pub fn pg_base_url() -> String {
    let inst = PG.get().expect("pg not initialized — call ensure_pg() first");
    format!("postgres://postgres@{}:{}/postgres", inst.host, inst.port)
}

pub async fn ensure_pg() {
    PG_INIT
        .get_or_init(|| async {
            let container = Postgres::default()
                .with_tag("18-alpine")
                .with_env_var("POSTGRES_HOST_AUTH_METHOD", "trust")
                .start()
                .await
                .expect("failed to start postgres container");

            let id = container.id().to_string();
            let host = container.get_host().await.unwrap().to_string();
            let port = container.get_host_port_ipv4(5432).await.unwrap();

            let _ = CONTAINER_ID.set(id);
            let _ = PG.set(PgInstance { host, port });

            unsafe { libc::atexit(cleanup_test_containers) };

            std::mem::forget(container);
        })
        .await;
}

#[allow(dead_code)]
pub async fn ensure_network() {
    let docker = bollard::Docker::connect_with_local_defaults().expect("docker not available");
    if docker.inspect_network::<&str>(NETWORK_NAME, None).await.is_ok() {
        return;
    }
    let _ = docker
        .create_network(CreateNetworkOptions {
            name: NETWORK_NAME,
            driver: "bridge",
            ..Default::default()
        })
        .await;
}
