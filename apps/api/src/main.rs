#![forbid(unsafe_code)]

use openworkspace_api::audit::{audit_writer, AuditSender, AUDIT_CHANNEL_CAPACITY};
use openworkspace_api::core::Settings;
use openworkspace_api::db::{WorkspaceInstanceRepository, UserRepository};
use openworkspace_api::docker::{DockerClient, DockerService};
use openworkspace_api::routes::{api_routes, with_audit_middleware, AppState};
use migration::{Migrator, MigratorTrait};
use sea_orm::Database;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal::unix::{signal, SignalKind};
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

    // ── Audit channel: one writer task, best-effort batching ──
    let (audit_tx, audit_rx) = tokio::sync::mpsc::channel(AUDIT_CHANNEL_CAPACITY);
    let audit = AuditSender::new(audit_tx);
    // The writer exits on this signal rather than on channel closure: the
    // health worker holds its own `AuditSender` clone for its whole lifetime,
    // so the channel never closes while the process lives.
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let audit_db = db.clone();
    let audit_writer_handle = tokio::spawn(async move {
        audit_writer(audit_rx, audit_db, shutdown_rx).await;
    });

    let state = AppState {
        db,
        docker,
        vnc_cache: vnc_cache.clone(),
        settings: settings.clone(),
        metrics: Arc::new(openworkspace_api::metrics::MetricsStore::new()),
        audit: audit.clone(),
    };

    // ── Spawn health worker ──
    {
        let worker_db = state.db.clone();
        let worker_docker = state.docker.clone();
        let worker_vnc_cache = state.vnc_cache.clone();
        let worker_gateway_ip = state.settings.host_gateway_ip.clone();
        let worker_metrics = state.metrics.clone();
        let worker_audit = state.audit.clone();
        let worker_retention_days = state.settings.audit_retention_days;
        tokio::spawn(async move {
            openworkspace_api::health_worker::run(worker_db, worker_docker, worker_vnc_cache, worker_gateway_ip, worker_metrics, worker_audit, worker_retention_days).await;
        });
    }

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = with_audit_middleware(api_routes(), &state)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: std::net::SocketAddr = settings.bind_address().parse().expect("Invalid bind address");

    tracing::info!("Server running on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("Failed to bind listener");
    axum::serve(listener, app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();

    // Graceful shutdown complete: the app (and its AppState clone of the audit
    // sender) is dropped, so in-flight requests have finished. Signal the
    // writer to drain the channel's remainder and flush the tail of the audit
    // stream to the DB before the process exits.
    let _ = shutdown_tx.send(true);
    drop(audit);
    if tokio::time::timeout(Duration::from_secs(5), audit_writer_handle)
        .await
        .is_err()
    {
        tracing::warn!("audit writer did not flush within 5s on shutdown");
    }
}

/// Wait for SIGTERM or SIGINT, then return so `with_graceful_shutdown` can
/// drain in-flight requests. This is the API's first signal handler.
async fn shutdown_signal() {
    let mut terminate = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
    tokio::select! {
        _ = terminate.recv() => tracing::info!("SIGTERM received, shutting down gracefully..."),
        _ = tokio::signal::ctrl_c() => tracing::info!("SIGINT received, shutting down gracefully..."),
    }
}
