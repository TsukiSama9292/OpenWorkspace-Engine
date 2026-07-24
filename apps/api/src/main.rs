use openworkspace_api::core::Settings;
use openworkspace_api::db::{WorkspaceInstanceRepository, UserRepository};
use openworkspace_api::routes::{api_routes, AppState};
use migration::{Migrator, MigratorTrait};
use sea_orm::Database;
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
            let mut count = 0;
            for inst in &instances {
                if inst.status == "running" {
                    vnc_cache.insert(&inst.vnc_token, &inst.status);
                    count += 1;
                }
            }
            tracing::info!("VNC cache loaded: {} running instances", count);
        }
        Err(e) => {
            tracing::warn!("Failed to load instances for VNC cache: {}", e);
        }
    }

    let state = AppState { db, vnc_cache, settings: settings.clone() };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = api_routes()
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let bind = settings.bind_address();
    let listener = tokio::net::TcpListener::bind(&bind)
        .await
        .unwrap();
    tracing::info!("Server running on http://{}", bind);
    axum::serve(listener, app).await.unwrap();
}
