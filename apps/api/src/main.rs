mod auth;
mod db;
mod docker;
mod routes;
mod vnc_trafik;

use db::UserRepository;
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

    tracing::info!("Admin seed done, starting server...");

    let state = AppState { db };

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
