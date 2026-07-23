use sqlx::PgPool;
use uuid::Uuid;

fn generate_vnc_token() -> String {
    Uuid::new_v4().as_simple().to_string()
}

pub struct UserRepository<'a> {
    pub db: &'a PgPool,
}

impl<'a> UserRepository<'a> {
    pub fn new(db: &'a PgPool) -> Self {
        Self { db }
    }

    pub async fn seed_admin(&self) -> Result<(), sqlx::Error> {
        let existing = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM users WHERE role = 'admin'")
            .fetch_one(self.db)
            .await?;
        if existing > 0 {
            return Ok(());
        }
        let password = std::env::var("ADMIN_PASSWORD").unwrap_or_else(|_| "admin".to_string());
        let password_hash = bcrypt::hash(&password, 10).expect("Failed to hash admin password");
        self.create("admin", &password_hash, "admin").await?;
        tracing::info!("Seeded default admin user (username: admin)");
        Ok(())
    }

    pub async fn find_by_username(
        &self,
        username: &str,
    ) -> Result<Option<(Uuid, String, String, String)>, sqlx::Error> {
        sqlx::query_as(
            "SELECT id, username, password_hash, role FROM users WHERE username = $1",
        )
        .bind(username)
        .fetch_optional(self.db)
        .await
    }

    pub async fn find_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<(Uuid, String, String, String, chrono::DateTime<chrono::Utc>)>, sqlx::Error>
    {
        sqlx::query_as("SELECT id, username, password_hash, role, created_at FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(self.db)
            .await
    }

    pub async fn create(
        &self,
        username: &str,
        password_hash: &str,
        role: &str,
    ) -> Result<Uuid, sqlx::Error> {
        let id = Uuid::new_v4();
        sqlx::query("INSERT INTO users (id, username, password_hash, role) VALUES ($1, $2, $3, $4)")
            .bind(id)
            .bind(username)
            .bind(password_hash)
            .bind(role)
            .execute(self.db)
            .await?;
        Ok(id)
    }

    pub async fn list_all(
        &self,
    ) -> Result<Vec<(Uuid, String, String, chrono::DateTime<chrono::Utc>)>, sqlx::Error> {
        sqlx::query_as("SELECT id, username, role, created_at FROM users ORDER BY created_at")
            .fetch_all(self.db)
            .await
    }

    pub async fn delete(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM users WHERE id = $1")
            .bind(id)
            .execute(self.db)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

#[derive(Debug, Clone)]
pub struct Workspace {
    pub id: Uuid,
    pub name: String,
    pub instance_number: i32,
    pub container_id: Option<String>,
    pub status: String,
    pub owner_id: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub vnc_token: Option<String>,
    pub image: Option<String>,
    pub cores: Option<i32>,
    pub memory: Option<i64>,
    pub gpu_count: Option<i32>,
    pub persistent_storage: Option<bool>,
    pub volume_host_path: Option<String>,
    pub volume_container_path: Option<String>,
    pub owner_username: Option<String>,
}

pub struct WorkspaceRepository<'a> {
    pub db: &'a PgPool,
}

impl<'a> WorkspaceRepository<'a> {
    pub fn new(db: &'a PgPool) -> Self {
        Self { db }
    }

    pub async fn create(
        &self,
        name: &str,
        owner_id: Uuid,
        image: &str,
        cores: i32,
        memory: i64,
        gpu_count: i32,
        persistent_storage: bool,
        volume_host_path: Option<&str>,
        volume_container_path: &str,
    ) -> Result<Workspace, sqlx::Error> {
        let id = Uuid::new_v4();
        let vnc_token = generate_vnc_token();
        sqlx::query_as::<_, Workspace>(
            "INSERT INTO workspaces (id, name, owner_id, vnc_token, image, cores, memory, gpu_count, persistent_storage, volume_host_path, volume_container_path)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
             RETURNING id, name, instance_number, container_id, status, owner_id, created_at, vnc_token, image, cores, memory, gpu_count, persistent_storage, volume_host_path, volume_container_path, NULL AS owner_username",
        )
        .bind(id)
        .bind(name)
        .bind(owner_id)
        .bind(&vnc_token)
        .bind(image)
        .bind(cores)
        .bind(memory)
        .bind(gpu_count)
        .bind(persistent_storage)
        .bind(volume_host_path)
        .bind(volume_container_path)
        .fetch_one(self.db)
        .await
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<Workspace>, sqlx::Error> {
        sqlx::query_as::<_, Workspace>(
            "SELECT w.id, w.name, w.instance_number, w.container_id, w.status, w.owner_id, w.created_at, w.vnc_token, w.image, w.cores, w.memory, w.gpu_count, w.persistent_storage, w.volume_host_path, w.volume_container_path, u.username AS owner_username FROM workspaces w LEFT JOIN users u ON w.owner_id = u.id WHERE w.id = $1",
        )
        .bind(id)
        .fetch_optional(self.db)
        .await
    }

    pub async fn find_by_vnc_token(&self, token: &str) -> Result<Option<Workspace>, sqlx::Error> {
        sqlx::query_as::<_, Workspace>(
            "SELECT w.id, w.name, w.instance_number, w.container_id, w.status, w.owner_id, w.created_at, w.vnc_token, w.image, w.cores, w.memory, w.gpu_count, w.persistent_storage, w.volume_host_path, w.volume_container_path, u.username AS owner_username FROM workspaces w LEFT JOIN users u ON w.owner_id = u.id WHERE w.vnc_token = $1",
        )
        .bind(token)
        .fetch_optional(self.db)
        .await
    }

    pub async fn list_by_owner(&self, owner_id: Uuid) -> Result<Vec<Workspace>, sqlx::Error> {
        sqlx::query_as::<_, Workspace>(
            "SELECT w.id, w.name, w.instance_number, w.container_id, w.status, w.owner_id, w.created_at, w.vnc_token, w.image, w.cores, w.memory, w.gpu_count, w.persistent_storage, w.volume_host_path, w.volume_container_path, u.username AS owner_username FROM workspaces w LEFT JOIN users u ON w.owner_id = u.id WHERE w.owner_id = $1 ORDER BY w.created_at",
        )
        .bind(owner_id)
        .fetch_all(self.db)
        .await
    }

    pub async fn list_all(&self) -> Result<Vec<Workspace>, sqlx::Error> {
        sqlx::query_as::<_, Workspace>(
            "SELECT w.id, w.name, w.instance_number, w.container_id, w.status, w.owner_id, w.created_at, w.vnc_token, w.image, w.cores, w.memory, w.gpu_count, w.persistent_storage, w.volume_host_path, w.volume_container_path, u.username AS owner_username FROM workspaces w LEFT JOIN users u ON w.owner_id = u.id ORDER BY w.created_at",
        )
        .fetch_all(self.db)
        .await
    }

    pub async fn update_status(&self, id: Uuid, status: &str) -> Result<bool, sqlx::Error> {
        let result =
            sqlx::query("UPDATE workspaces SET status = $1, updated_at = NOW() WHERE id = $2")
                .bind(status)
                .bind(id)
                .execute(self.db)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn update_container_id(
        &self,
        id: Uuid,
        container_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE workspaces SET container_id = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(container_id)
        .bind(id)
        .execute(self.db)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM workspaces WHERE id = $1")
            .bind(id)
            .execute(self.db)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

pub struct RegistryRepository<'a> {
    pub db: &'a PgPool,
}

impl<'a> RegistryRepository<'a> {
    pub fn new(db: &'a PgPool) -> Self {
        Self { db }
    }

    pub async fn get_url(&self) -> Result<Option<String>, sqlx::Error> {
        sqlx::query_scalar::<_, String>("SELECT registry_url FROM registry_config WHERE id = 1")
            .fetch_optional(self.db)
            .await
    }

    pub async fn set_url(&self, url: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO registry_config (id, registry_url, updated_at) VALUES (1, $1, NOW())
             ON CONFLICT (id) DO UPDATE SET registry_url = $1, updated_at = NOW()",
        )
        .bind(url)
        .execute(self.db)
        .await?;
        Ok(())
    }

    pub async fn get_cached(&self) -> Result<Option<serde_json::Value>, sqlx::Error> {
        sqlx::query_scalar::<_, serde_json::Value>(
            "SELECT registry_json FROM registry_cache WHERE id = 1",
        )
        .fetch_optional(self.db)
        .await
    }

    pub async fn set_cached(&self, json: &serde_json::Value) -> Result<(), sqlx::Error> {
        sqlx::query(
            "INSERT INTO registry_cache (id, registry_json, synced_at) VALUES (1, $1, NOW())
             ON CONFLICT (id) DO UPDATE SET registry_json = $1, synced_at = NOW()",
        )
        .bind(json)
        .execute(self.db)
        .await?;
        Ok(())
    }
}
