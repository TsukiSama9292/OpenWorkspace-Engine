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

pub struct InstanceRepository<'a> {
    pub db: &'a PgPool,
}

impl<'a> InstanceRepository<'a> {
    pub fn new(db: &'a PgPool) -> Self {
        Self { db }
    }

    pub async fn create(
        &self,
        name: &str,
        owner_id: Uuid,
    ) -> Result<(Uuid, i32, String), sqlx::Error> {
        let id = Uuid::new_v4();
        let vnc_token = generate_vnc_token();
        sqlx::query_as::<_, (Uuid, i32, String)>(
            "INSERT INTO instances (id, name, owner_id, vnc_token) VALUES ($1, $2, $3, $4) RETURNING id, instance_number, vnc_token",
        )
        .bind(id)
        .bind(name)
        .bind(owner_id)
        .bind(&vnc_token)
        .fetch_one(self.db)
        .await
    }

    pub async fn find_by_id(
        &self,
        id: Uuid,
    ) -> Result<
        Option<(
            Uuid,
            String,
            i32,
            Option<String>,
            String,
            Uuid,
            chrono::DateTime<chrono::Utc>,
            Option<String>,
        )>,
        sqlx::Error,
    > {
        sqlx::query_as(
            "SELECT id, name, instance_number, container_id, status, owner_id, created_at, vnc_token FROM instances WHERE id = $1",
        )
        .bind(id)
        .fetch_optional(self.db)
        .await
    }

    pub async fn find_by_vnc_token(
        &self,
        token: &str,
    ) -> Result<
        Option<(
            Uuid,
            String,
            i32,
            Option<String>,
            String,
            Option<String>,
        )>,
        sqlx::Error,
    > {
        sqlx::query_as(
            "SELECT id, name, instance_number, container_id, status, vnc_token FROM instances WHERE vnc_token = $1",
        )
        .bind(token)
        .fetch_optional(self.db)
        .await
    }

    pub async fn list_by_owner(
        &self,
        owner_id: Uuid,
    ) -> Result<
        Vec<(
            Uuid,
            String,
            i32,
            Option<String>,
            String,
            Uuid,
            chrono::DateTime<chrono::Utc>,
            Option<String>,
        )>,
        sqlx::Error,
    > {
        sqlx::query_as(
            "SELECT id, name, instance_number, container_id, status, owner_id, created_at, vnc_token FROM instances WHERE owner_id = $1 ORDER BY created_at",
        )
        .bind(owner_id)
        .fetch_all(self.db)
        .await
    }

    pub async fn list_all(
        &self,
    ) -> Result<
        Vec<(
            Uuid,
            String,
            i32,
            Option<String>,
            String,
            Uuid,
            chrono::DateTime<chrono::Utc>,
            Option<String>,
        )>,
        sqlx::Error,
    > {
        sqlx::query_as(
            "SELECT id, name, instance_number, container_id, status, owner_id, created_at, vnc_token FROM instances ORDER BY created_at",
        )
        .fetch_all(self.db)
        .await
    }

    pub async fn update_status(
        &self,
        id: Uuid,
        status: &str,
    ) -> Result<bool, sqlx::Error> {
        let result =
            sqlx::query("UPDATE instances SET status = $1, updated_at = NOW() WHERE id = $2")
                .bind(status)
                .bind(id)
                .execute(self.db)
                .await?;
        Ok(result.rows_affected() > 0)
    }

    #[allow(dead_code)]
    pub async fn update_container_id(
        &self,
        id: Uuid,
        container_id: &str,
    ) -> Result<bool, sqlx::Error> {
        let result = sqlx::query(
            "UPDATE instances SET container_id = $1, updated_at = NOW() WHERE id = $2",
        )
        .bind(container_id)
        .bind(id)
        .execute(self.db)
        .await?;
        Ok(result.rows_affected() > 0)
    }

    pub async fn delete(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM instances WHERE id = $1")
            .bind(id)
            .execute(self.db)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}
