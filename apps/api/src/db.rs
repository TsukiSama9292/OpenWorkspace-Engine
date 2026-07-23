use sqlx::PgPool;
use uuid::Uuid;

fn generate_vnc_token() -> String {
    Uuid::new_v4().as_simple().to_string()
}

// ── User Repository ────────────────────────────────────────────

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

// ── Workspace Config ───────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkspaceConfig {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub owner_id: Uuid,
    pub image: String,
    pub cores: i32,
    pub memory: i64,
    pub gpu_count: i32,
    pub docker_registry: Option<String>,
    pub run_config: serde_json::Value,
    pub exec_config: serde_json::Value,
    pub volume_mappings: serde_json::Value,
    pub persistent_storage_path: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub struct WorkspaceConfigRepository<'a> {
    pub db: &'a PgPool,
}

impl<'a> WorkspaceConfigRepository<'a> {
    pub fn new(db: &'a PgPool) -> Self {
        Self { db }
    }

    pub async fn create(
        &self,
        name: &str,
        description: Option<&str>,
        owner_id: Uuid,
        image: &str,
        cores: i32,
        memory: i64,
        gpu_count: i32,
        docker_registry: Option<&str>,
        run_config: &serde_json::Value,
        exec_config: &serde_json::Value,
        volume_mappings: &serde_json::Value,
        persistent_storage_path: Option<&str>,
    ) -> Result<WorkspaceConfig, sqlx::Error> {
        let id = Uuid::new_v4();
        sqlx::query_as::<_, WorkspaceConfig>(
            "INSERT INTO workspace_configs (id, name, description, owner_id, image, cores, memory, gpu_count, docker_registry, run_config, exec_config, volume_mappings, persistent_storage_path)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
             RETURNING *",
        )
        .bind(id)
        .bind(name)
        .bind(description)
        .bind(owner_id)
        .bind(image)
        .bind(cores)
        .bind(memory)
        .bind(gpu_count)
        .bind(docker_registry)
        .bind(run_config)
        .bind(exec_config)
        .bind(volume_mappings)
        .bind(persistent_storage_path)
        .fetch_one(self.db)
        .await
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<WorkspaceConfig>, sqlx::Error> {
        sqlx::query_as::<_, WorkspaceConfig>("SELECT * FROM workspace_configs WHERE id = $1")
            .bind(id)
            .fetch_optional(self.db)
            .await
    }

    pub async fn list_by_owner(&self, owner_id: Uuid) -> Result<Vec<WorkspaceConfig>, sqlx::Error> {
        sqlx::query_as::<_, WorkspaceConfig>(
            "SELECT * FROM workspace_configs WHERE owner_id = $1 ORDER BY created_at",
        )
        .bind(owner_id)
        .fetch_all(self.db)
        .await
    }

    pub async fn list_all(&self) -> Result<Vec<WorkspaceConfig>, sqlx::Error> {
        sqlx::query_as::<_, WorkspaceConfig>("SELECT * FROM workspace_configs ORDER BY created_at")
            .fetch_all(self.db)
            .await
    }

    pub async fn count_instances(&self, config_id: Uuid) -> Result<i64, sqlx::Error> {
        sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM workspace_instances WHERE config_id = $1",
        )
        .bind(config_id)
        .fetch_one(self.db)
        .await
    }

    pub async fn update(
        &self,
        id: Uuid,
        name: &str,
        description: Option<&str>,
        image: &str,
        cores: i32,
        memory: i64,
        gpu_count: i32,
        docker_registry: Option<&str>,
        run_config: &serde_json::Value,
        exec_config: &serde_json::Value,
        volume_mappings: &serde_json::Value,
        persistent_storage_path: Option<&str>,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE workspace_configs SET name = $1, description = $2, image = $3, cores = $4, memory = $5, gpu_count = $6, docker_registry = $7, run_config = $8, exec_config = $9, volume_mappings = $10, persistent_storage_path = $11, updated_at = NOW() WHERE id = $12",
        )
        .bind(name)
        .bind(description)
        .bind(image)
        .bind(cores)
        .bind(memory)
        .bind(gpu_count)
        .bind(docker_registry)
        .bind(run_config)
        .bind(exec_config)
        .bind(volume_mappings)
        .bind(persistent_storage_path)
        .bind(id)
        .execute(self.db)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM workspace_configs WHERE id = $1")
            .bind(id)
            .execute(self.db)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

// ── Workspace Instance ─────────────────────────────────────────

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct WorkspaceInstance {
    pub id: Uuid,
    pub config_id: Uuid,
    pub name: String,
    pub instance_number: i32,
    pub owner_id: Uuid,
    pub container_id: Option<String>,
    pub status: String,
    pub vnc_token: String,
    pub mount_persistent: bool,
    pub resolved_volume_host_path: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

pub struct WorkspaceInstanceRepository<'a> {
    pub db: &'a PgPool,
}

impl<'a> WorkspaceInstanceRepository<'a> {
    pub fn new(db: &'a PgPool) -> Self {
        Self { db }
    }

    pub async fn launch(
        &self,
        config_id: Uuid,
        owner_id: Uuid,
        config_name: &str,
        mount_persistent: bool,
        resolved_volume_host_path: Option<&str>,
    ) -> Result<WorkspaceInstance, sqlx::Error> {
        let id = Uuid::new_v4();
        let vnc_token = generate_vnc_token();

        // Auto-generate instance name: "{config_name}-{next_number}"
        let next_number: (i32,) = sqlx::query_as(
            "SELECT COALESCE(MAX(instance_number), 0) + 1 FROM workspace_instances WHERE config_id = $1",
        )
        .bind(config_id)
        .fetch_one(self.db)
        .await?;
        let name = format!("{}-{}", config_name, next_number.0);

        sqlx::query_as::<_, WorkspaceInstance>(
            "INSERT INTO workspace_instances (id, config_id, name, instance_number, owner_id, container_id, status, vnc_token, mount_persistent, resolved_volume_host_path)
             VALUES ($1, $2, $3, $4, $5, NULL, 'stopped', $6, $7, $8)
             RETURNING *",
        )
        .bind(id)
        .bind(config_id)
        .bind(&name)
        .bind(next_number.0)
        .bind(owner_id)
        .bind(&vnc_token)
        .bind(mount_persistent)
        .bind(resolved_volume_host_path)
        .fetch_one(self.db)
        .await
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<WorkspaceInstance>, sqlx::Error> {
        sqlx::query_as::<_, WorkspaceInstance>("SELECT * FROM workspace_instances WHERE id = $1")
            .bind(id)
            .fetch_optional(self.db)
            .await
    }

    pub async fn find_by_vnc_token(&self, token: &str) -> Result<Option<WorkspaceInstance>, sqlx::Error> {
        sqlx::query_as::<_, WorkspaceInstance>(
            "SELECT * FROM workspace_instances WHERE vnc_token = $1",
        )
        .bind(token)
        .fetch_optional(self.db)
        .await
    }

    pub async fn list_by_owner(&self, owner_id: Uuid) -> Result<Vec<WorkspaceInstance>, sqlx::Error> {
        sqlx::query_as::<_, WorkspaceInstance>(
            "SELECT * FROM workspace_instances WHERE owner_id = $1 ORDER BY created_at",
        )
        .bind(owner_id)
        .fetch_all(self.db)
        .await
    }

    pub async fn list_all(&self) -> Result<Vec<WorkspaceInstance>, sqlx::Error> {
        sqlx::query_as::<_, WorkspaceInstance>("SELECT * FROM workspace_instances ORDER BY created_at")
            .fetch_all(self.db)
            .await
    }

    pub async fn list_by_config(&self, config_id: Uuid) -> Result<Vec<WorkspaceInstance>, sqlx::Error> {
        sqlx::query_as::<_, WorkspaceInstance>(
            "SELECT * FROM workspace_instances WHERE config_id = $1 ORDER BY created_at",
        )
        .bind(config_id)
        .fetch_all(self.db)
        .await
    }

    pub async fn update_status(&self, id: Uuid, status: &str) -> Result<bool, sqlx::Error> {
        let result =
            sqlx::query("UPDATE workspace_instances SET status = $1, updated_at = NOW() WHERE id = $2")
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
            "UPDATE workspace_instances SET container_id = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(container_id)
        .bind(id)
        .execute(self.db)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM workspace_instances WHERE id = $1")
            .bind(id)
            .execute(self.db)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

// ── Registry Repository ────────────────────────────────────────

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
