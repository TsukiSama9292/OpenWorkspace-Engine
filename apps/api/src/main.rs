use openworkspace_api::core::Settings;
use openworkspace_api::db::{WorkspaceInstanceRepository, UserRepository};
use openworkspace_api::docker::{DockerClient, DockerService};
use openworkspace_api::routes::{api_routes, AppState};
use migration::{Migrator, MigratorTrait};
use sea_orm::Database;
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_target(false)
        .with_thread_names(true)
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let settings = Settings::new().expect("Failed to load settings");

    tracing::info!("Connecting to database...");

    let db = loop {
        match Database::connect(&settings.database_url).await {
            Ok(conn) => {
                // Run migrations on the connection
                Migrator::up(&conn, None)
                    .await
                    .expect("Failed to run migrations");
                break conn;
            }
            Err(e) => {
                tracing::warn!("Database not ready, retrying in 2s: {}", e);
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    };

    tracing::info!("Migrations done, seeding admin user...");

    UserRepository::new(&db)
        .seed_admin(&settings.admin_password)
        .await
        .expect("Failed to seed admin user");

    tracing::info!("Admin seed done, populating VNC cache...");

    let vnc_cache = openworkspace_api::vnc_cache::VncCache::new();
    let instance_repo = WorkspaceInstanceRepository::new(&db);
    match instance_repo.list_all().await {
        Ok(instances) => {
            let mut running_count = 0;
            let mut starting_count = 0;
            for inst in &instances {
                vnc_cache.insert(&inst.access_token, &inst.status);
                if inst.status == "running" {
                    running_count += 1;
                } else if inst.status == "starting" {
                    starting_count += 1;
                }
            }
            tracing::info!("VNC cache loaded: {} running, {} starting instances", running_count, starting_count);
        }
        Err(e) => {
            tracing::warn!("Failed to load instances for VNC cache: {}", e);
        }
    }

    let docker_client = DockerClient::new()
        .await
        .expect("Failed to connect to Docker");
    let docker: Arc<dyn DockerService> = Arc::new(docker_client);

    // Seed the `system_settings` singleton (host capacity + global policy
    // knobs) from docker-detected host capacity / env overrides. Fail-open:
    // detection failures and DB errors only log — the API still boots.
    if let Err(e) = openworkspace_api::system_settings::seed_from_host(&db, docker.as_ref()).await
    {
        tracing::warn!(
            "Failed to seed system_settings from host capacity: {} (continuing with existing row)",
            e
        );
    }

    let state = AppState {
        db,
        docker,
        vnc_cache: vnc_cache.clone(),
        settings: settings.clone(),
        network_lock: Arc::new(tokio::sync::Mutex::new(())),
    };

    // ── Spawn health worker ──
    {
        let worker_db = state.db.clone();
        let worker_docker = state.docker.clone();
        let worker_vnc_cache = state.vnc_cache.clone();
        let worker_gateway_ip = state.settings.host_gateway_ip.clone();
        tokio::spawn(async move {
            openworkspace_api::health_worker::run(worker_db, worker_docker, worker_vnc_cache, worker_gateway_ip).await;
        });
    }

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = api_routes()
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: std::net::SocketAddr = settings.bind_address().parse().expect("Invalid bind address");

    tracing::info!("Server running on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind listener");
    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}
