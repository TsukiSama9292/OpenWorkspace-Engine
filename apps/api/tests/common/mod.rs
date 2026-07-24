mod docker_guard;
mod pg;
pub use docker_guard::*;
pub use pg::*;

use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use openworkspace_api::core::Settings;
use openworkspace_api::db::UserRepository;
use openworkspace_api::routes::{AppState, api_routes};
use migration::{Migrator, MigratorTrait};
use reqwest::Client;

#[allow(dead_code)]
static DB_COUNTER: AtomicU32 = AtomicU32::new(0);

pub struct TestContext {
    pub base_url: String,
    pub client: Client,
    pub db_name: String,
}

#[allow(dead_code)]
impl TestContext {
    pub async fn new() -> Self {
        ensure_pg().await;

        let counter = DB_COUNTER.fetch_add(1, Ordering::Relaxed);
        let db_name = format!("test_{}_{:04}", std::process::id(), counter);
        let base_url = pg_base_url();

        let (client, connection) = 'connect: {
            for attempt in 0..20 {
                match tokio_postgres::connect(&base_url, tokio_postgres::NoTls).await {
                    Ok(conn) => break 'connect conn,
                    Err(e) => {
                        if attempt == 19 {
                            panic!("failed to connect after retries: {}", e);
                        }
                        tokio::time::sleep(Duration::from_millis(250 * (attempt + 1))).await;
                    }
                }
            }
            unreachable!()
        };
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });
        {
            let (cleanup_client, cleanup_conn) = tokio_postgres::connect(&base_url, tokio_postgres::NoTls).await.unwrap();
            tokio::spawn(async move { let _ = cleanup_conn.await; });
            let _ = cleanup_client.execute(
                &format!("SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = '{}'", db_name)[..],
                &[],
            ).await;
            let _ = cleanup_client.execute(
                &format!("DROP DATABASE IF EXISTS \"{}\"", db_name)[..],
                &[],
            ).await;
        }

        client
            .execute(&format!("CREATE DATABASE \"{}\"", db_name)[..], &[])
            .await
            .expect("failed to create test database");

        let db_url = pg_url(&db_name);

        let migrator_db = sea_orm::Database::connect(&db_url)
            .await
            .expect("failed to connect for migrations");
        Migrator::up(&migrator_db, None)
            .await
            .expect("failed to run migrations");
        drop(migrator_db);

        let db = sea_orm::Database::connect(&db_url)
            .await
            .expect("failed to connect");

        let settings = Settings {
            database_url: db_url,
            jwt_secret: "test-secret-key-for-testing".to_string(),
            admin_password: "admin".to_string(),
            server_host: "127.0.0.1".to_string(),
            server_port: 0,
            db_max_connections: 5,
            docker_network: "ow-test".to_string(),
        };

        UserRepository::new(&db)
            .seed_admin(&settings.admin_password)
            .await
            .expect("failed to seed admin");

        let vnc_cache = openworkspace_api::vnc_cache::VncCache::new();
        let state = AppState {
            db: db.clone(),
            vnc_cache,
            settings: settings.clone(),
        };

        let cors = tower_http::cors::CorsLayer::new()
            .allow_origin(tower_http::cors::Any)
            .allow_methods(tower_http::cors::Any)
            .allow_headers(tower_http::cors::Any);

        let app = api_routes().layer(cors).with_state(state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind");
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://127.0.0.1:{}", addr.port());

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let client = Client::builder()
            .cookie_store(true)
            .build()
            .expect("failed to build HTTP client");

        TestContext {
            base_url,
            client,
            db_name,
        }
    }

    #[allow(dead_code)]
    pub async fn cleanup(&self) {
        cleanup_test_containers().await;
    }

    pub async fn login_admin(&self) -> reqwest::Response {
        self.client
            .post(format!("{}/api/auth/login", self.base_url))
            .json(&serde_json::json!({
                "username": "admin",
                "password": "admin",
            }))
            .send()
            .await
            .expect("login request failed")
    }

    pub async fn login_user(&self, username: &str, password: &str) -> reqwest::Response {
        self.client
            .post(format!("{}/api/auth/login", self.base_url))
            .json(&serde_json::json!({
                "username": username,
                "password": password,
            }))
            .send()
            .await
            .expect("login request failed")
    }

    pub async fn get(&self, path: &str) -> reqwest::Response {
        self.client
            .get(format!("{}{}", self.base_url, path))
            .send()
            .await
            .expect("GET request failed")
    }

    pub async fn post(&self, path: &str, body: &serde_json::Value) -> reqwest::Response {
        self.client
            .post(format!("{}{}", self.base_url, path))
            .json(body)
            .send()
            .await
            .expect("POST request failed")
    }

    pub async fn put(&self, path: &str, body: &serde_json::Value) -> reqwest::Response {
        self.client
            .put(format!("{}{}", self.base_url, path))
            .json(body)
            .send()
            .await
            .expect("PUT request failed")
    }

    pub async fn delete(&self, path: &str) -> reqwest::Response {
        self.client
            .delete(format!("{}{}", self.base_url, path))
            .send()
            .await
            .expect("DELETE request failed")
    }

    pub async fn login_token(&self) -> String {
        let resp = self.login_admin().await;
        let headers = resp.headers();
        let cookie = headers.get("set-cookie").unwrap().to_str().unwrap();
        let token = cookie.split(';').next().unwrap();
        let token = token.strip_prefix("ow_token=").unwrap_or(token);
        token.to_string()
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        let url = pg_base_url();
        let db_name = self.db_name.clone();
        let _ = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                if let Ok((client, conn)) =
                    tokio_postgres::connect(&url, tokio_postgres::NoTls).await
                {
                    tokio::spawn(conn);
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
                        .execute(
                            &format!("DROP DATABASE IF EXISTS \"{}\"", db_name)[..],
                            &[],
                        )
                        .await;
                }
            });
        });
    }
}
