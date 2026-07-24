use std::sync::OnceLock;

use bollard::network::CreateNetworkOptions;

const NETWORK_NAME: &str = "ow-test";

#[derive(Clone, Debug)]
struct PgInstance {
    host: String,
    port: u16,
}

static PG: OnceLock<PgInstance> = OnceLock::new();

pub fn pg_url(db_name: &str) -> String {
    let inst = PG.get().expect("pg not initialized — ensure_pg() must be called first");
    format!("postgres://postgres@{}:{}/{}", inst.host, inst.port, db_name)
}

pub fn pg_base_url() -> String {
    let inst = PG.get().expect("pg not initialized — ensure_pg() must be called first");
    format!("postgres://postgres@{}:{}/postgres", inst.host, inst.port)
}

pub async fn ensure_pg() {
    if PG.get().is_some() {
        return;
    }

    let host = std::env::var("PG_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port: u16 = std::env::var("PG_PORT")
        .expect("PG_PORT must be set")
        .parse()
        .expect("PG_PORT must be a valid port number");

    let _ = PG.set(PgInstance { host, port });
}

#[allow(dead_code)]
pub async fn ensure_network() {
    let Ok(docker) = bollard::Docker::connect_with_local_defaults() else {
        return;
    };
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
