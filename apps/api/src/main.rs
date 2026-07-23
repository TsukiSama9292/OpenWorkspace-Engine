mod auth;
mod db;
mod docker;
mod routes;
mod vnc_cache;
mod vnc_trafik;

use db::{WorkspaceRepository, UserRepository};
use routes::{api_routes, AppState};
use sqlx::postgres::PgPoolOptions;
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

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");

    tracing::info!("Connecting to database...");

    let db = loop {
        match PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
        {
            Ok(pool) => break pool,
            Err(e) => {
                tracing::warn!("Database not ready, retrying in 2s: {}", e);
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }
        }
    };

    tracing::info!("Database connected, running migrations...");

    sqlx::migrate!("./migrations")
        .run(&db)
        .await
        .expect("Failed to run migrations");

    tracing::info!("Migrations done, seeding admin user...");

    UserRepository::new(&db)
        .seed_admin()
        .await
        .expect("Failed to seed admin user");

    tracing::info!("Admin seed done, populating VNC cache...");

    let vnc_cache = vnc_cache::VncCache::new();
    let workspace_repo = WorkspaceRepository::new(&db);
    match workspace_repo.list_all().await {
        Ok(workspaces) => {
            let mut count = 0;
            for ws in &workspaces {
                if ws.status == "running" {
                    if let Some(ref token) = ws.vnc_token {
                        vnc_cache.insert(token, &ws.status, ws.owner_id);
                        count += 1;
                    }
                }
            }
            tracing::info!("VNC cache loaded: {} running workspaces", count);
        }
        Err(e) => {
            tracing::warn!("Failed to load workspaces for VNC cache: {}", e);
        }
    }

    let state = AppState { db, vnc_cache };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = api_routes()
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000")
        .await
        .unwrap();
    tracing::info!("Server running on http://0.0.0.0:3000");
    axum::serve(listener, app).await.unwrap();
}
