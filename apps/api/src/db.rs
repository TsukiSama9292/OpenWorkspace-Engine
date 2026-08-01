use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, Insert, Order, PaginatorTrait, QueryFilter, QueryOrder, Set};
use sea_orm::sea_query::{Expr, OnConflict};
use uuid::Uuid;

fn generate_access_token() -> String {
    Uuid::new_v4().as_simple().to_string()
}

pub fn generate_access_password() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let len = 127;
    let pool: Vec<u8> = (b'a'..=b'z')
        .chain(b'A'..=b'Z')
        .chain(b'0'..=b'9')
        .collect();
    (0..len)
        .map(|_| {
            let idx = rng.gen_range(0..pool.len());
            pool[idx] as char
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::generate_access_password;

    #[test]
    fn access_password_uses_only_alphanumeric_chars() {
        for _ in 0..50 {
            let pw = generate_access_password();
            assert_eq!(pw.len(), 127);
            assert!(pw.chars().all(|c| c.is_ascii_alphanumeric()));
        }
    }
}

// ── Entity Models ─────────────────────────────────────────────

pub mod user {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "users")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: Uuid,
        pub username: String,
        pub password_hash: String,
        pub role: String,
        pub created_at: DateTimeUtc,
        pub updated_at: DateTimeUtc,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(has_many = "super::workspace_instance::Entity")]
        WorkspaceInstances,
    }

    impl Related<super::workspace_instance::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::WorkspaceInstances.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod workspace_template {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "workspace_templates")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: Uuid,
        pub name: String,
        pub description: Option<String>,
        pub owner_id: Uuid,
        pub image: String,
        pub cores: i32,
        pub memory: i64,
        pub gpu_count: i32,
        pub docker_registry: Option<String>,
        pub run_config: Json,
        pub exec_config: Json,
        pub volume_mappings: Json,
        pub remote_type: String,
        pub container_runtime: String,
        pub persistent_storage_path: Option<String>,
        pub max_run_seconds: Option<i64>,
        pub timeout_action: String,
        pub network_bandwidth_up_mbps: i32,
        pub network_bandwidth_down_mbps: i32,
        pub created_at: DateTimeUtc,
        pub updated_at: DateTimeUtc,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::user::Entity",
            from = "Column::OwnerId",
            to = "super::user::Column::Id"
        )]
        User,
        #[sea_orm(has_many = "super::workspace_instance::Entity")]
        WorkspaceInstances,
    }

    impl Related<super::user::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::User.def()
        }
    }

    impl Related<super::workspace_instance::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::WorkspaceInstances.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod workspace_instance {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "workspace_instances")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: Uuid,
        pub template_id: Uuid,
        pub name: String,
        pub instance_number: i32,
        pub owner_id: Uuid,
        pub container_id: Option<String>,
        pub status: String,
        pub access_token: String,
        pub access_password: String,
        pub mount_persistent: bool,
        pub resolved_volume_host_path: Option<String>,
        pub started_at: Option<DateTimeUtc>,
        pub created_at: DateTimeUtc,
        pub updated_at: DateTimeUtc,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[sea_orm(
            belongs_to = "super::workspace_template::Entity",
            from = "Column::TemplateId",
            to = "super::workspace_template::Column::Id"
        )]
        WorkspaceTemplate,
        #[sea_orm(
            belongs_to = "super::user::Entity",
            from = "Column::OwnerId",
            to = "super::user::Column::Id"
        )]
        User,
    }

    impl Related<super::workspace_template::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::WorkspaceTemplate.def()
        }
    }

    impl Related<super::user::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::User.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod registry_config {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "registry_config")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub registry_url: String,
        pub updated_at: DateTimeUtc,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod registry_cache {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "registry_cache")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub id: i32,
        pub registry_json: Json,
        pub synced_at: DateTimeUtc,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

// ── Public Model Types (for callers) ──────────────────────────

#[derive(Debug, Clone)]
pub struct WorkspaceTemplate {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub owner_id: Uuid,
    pub image: String,
    pub cores: i32,
    pub memory: i64,
    pub gpu_count: i32,
    pub docker_registry: Option<String>,
    pub remote_type: String,
    pub container_runtime: String,
    pub run_config: serde_json::Value,
    pub exec_config: serde_json::Value,
    pub volume_mappings: serde_json::Value,
    pub persistent_storage_path: Option<String>,
    pub max_run_seconds: Option<i64>,
    pub timeout_action: String,
    pub network_bandwidth_up_mbps: i32,
    pub network_bandwidth_down_mbps: i32,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<workspace_template::Model> for WorkspaceTemplate {
    fn from(m: workspace_template::Model) -> Self {
        Self {
            id: m.id,
            name: m.name,
            description: m.description,
            owner_id: m.owner_id,
            image: m.image,
            cores: m.cores,
            memory: m.memory,
            gpu_count: m.gpu_count,
            docker_registry: m.docker_registry,
            remote_type: m.remote_type,
            container_runtime: m.container_runtime,
            run_config: m.run_config.into(),
            exec_config: m.exec_config.into(),
            volume_mappings: m.volume_mappings.into(),
            persistent_storage_path: m.persistent_storage_path,
            max_run_seconds: m.max_run_seconds,
            timeout_action: m.timeout_action,
            network_bandwidth_up_mbps: m.network_bandwidth_up_mbps,
            network_bandwidth_down_mbps: m.network_bandwidth_down_mbps,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WorkspaceInstance {
    pub id: Uuid,
    pub template_id: Uuid,
    pub name: String,
    pub instance_number: i32,
    pub owner_id: Uuid,
    pub container_id: Option<String>,
    pub status: String,
    pub access_token: String,
    pub access_password: String,
    pub mount_persistent: bool,
    pub resolved_volume_host_path: Option<String>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<workspace_instance::Model> for WorkspaceInstance {
    fn from(m: workspace_instance::Model) -> Self {
        Self {
            id: m.id,
            template_id: m.template_id,
            name: m.name,
            instance_number: m.instance_number,
            owner_id: m.owner_id,
            container_id: m.container_id,
            status: m.status,
            access_token: m.access_token,
            access_password: m.access_password,
            mount_persistent: m.mount_persistent,
            resolved_volume_host_path: m.resolved_volume_host_path,
            started_at: m.started_at,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

// ── User Repository ───────────────────────────────────────────

pub struct UserRepository<'a> {
    pub db: &'a DatabaseConnection,
}

impl<'a> UserRepository<'a> {
    pub fn new(db: &'a DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn seed_admin(&self, admin_password: &str) -> Result<(), sea_orm::DbErr> {
        let existing = user::Entity::find()
            .filter(user::Column::Role.eq("admin"))
            .count(self.db)
            .await?;
        if existing > 0 {
            return Ok(());
        }
        let password_hash = bcrypt::hash(admin_password, 10).expect("Failed to hash admin password");
        self.create("admin", &password_hash, "admin").await?;
        tracing::info!("Seeded default admin user (username: admin)");
        Ok(())
    }

    pub async fn find_by_username(
        &self,
        username: &str,
    ) -> Result<Option<(Uuid, String, String, String)>, sea_orm::DbErr> {
        let model = user::Entity::find()
            .filter(user::Column::Username.eq(username))
            .one(self.db)
            .await?;
        Ok(model.map(|m| (m.id, m.username, m.password_hash, m.role)))
    }

    pub async fn find_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<(Uuid, String, String, String, chrono::DateTime<chrono::Utc>)>, sea_orm::DbErr>
    {
        let model = user::Entity::find_by_id(id).one(self.db).await?;
        Ok(model.map(|m| (m.id, m.username, m.password_hash, m.role, m.created_at)))
    }

    pub async fn create(
        &self,
        username: &str,
        password_hash: &str,
        role: &str,
    ) -> Result<Uuid, sea_orm::DbErr> {
        let id = Uuid::new_v4();
        let model = user::ActiveModel {
            id: Set(id),
            username: Set(username.to_string()),
            password_hash: Set(password_hash.to_string()),
            role: Set(role.to_string()),
            ..Default::default()
        };
        model.insert(self.db).await?;
        Ok(id)
    }

    pub async fn list_all(
        &self,
    ) -> Result<Vec<(Uuid, String, String, chrono::DateTime<chrono::Utc>)>, sea_orm::DbErr> {
        let models = user::Entity::find()
            .order_by_asc(user::Column::CreatedAt)
            .all(self.db)
            .await?;
        Ok(models
            .into_iter()
            .map(|m| (m.id, m.username, m.role, m.created_at))
            .collect())
    }

    pub async fn delete(&self, id: Uuid) -> Result<bool, sea_orm::DbErr> {
        let result = user::Entity::delete_by_id(id).exec(self.db).await?;
        Ok(result.rows_affected > 0)
    }

    pub async fn update(
        &self,
        id: Uuid,
        username: Option<&str>,
        password_hash: Option<&str>,
        role: Option<&str>,
    ) -> Result<bool, sea_orm::DbErr> {
        let existing = user::Entity::find_by_id(id)
            .one(self.db)
            .await?
            .ok_or(sea_orm::DbErr::RecordNotFound("User not found".into()))?;

        let mut model: user::ActiveModel = existing.into();

        if let Some(u) = username {
            model.username = Set(u.to_string());
        }
        if let Some(p) = password_hash {
            model.password_hash = Set(p.to_string());
        }
        if let Some(r) = role {
            model.role = Set(r.to_string());
        }

        model.update(self.db).await?;
        Ok(true)
    }
}

// ── Workspace Template Repository ─────────────────────────────

pub struct WorkspaceTemplateRepository<'a> {
    pub db: &'a DatabaseConnection,
}

impl<'a> WorkspaceTemplateRepository<'a> {
    pub fn new(db: &'a DatabaseConnection) -> Self {
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
        remote_type: &str,
        container_runtime: &str,
        run_config: &serde_json::Value,
        exec_config: &serde_json::Value,
        volume_mappings: &serde_json::Value,
        persistent_storage_path: Option<&str>,
        max_run_seconds: Option<i64>,
        timeout_action: &str,
        network_bandwidth_up_mbps: i32,
        network_bandwidth_down_mbps: i32,
    ) -> Result<WorkspaceTemplate, sea_orm::DbErr> {
        let id = Uuid::new_v4();
        let model = workspace_template::ActiveModel {
            id: Set(id),
            name: Set(name.to_string()),
            description: Set(description.map(|s| s.to_string())),
            owner_id: Set(owner_id),
            image: Set(image.to_string()),
            cores: Set(cores),
            memory: Set(memory),
            gpu_count: Set(gpu_count),
            docker_registry: Set(docker_registry.map(|s| s.to_string())),
            remote_type: Set(remote_type.to_string()),
            container_runtime: Set(container_runtime.to_string()),
            run_config: Set(run_config.clone().into()),
            exec_config: Set(exec_config.clone().into()),
            volume_mappings: Set(volume_mappings.clone().into()),
            persistent_storage_path: Set(persistent_storage_path.map(|s| s.to_string())),
            max_run_seconds: Set(max_run_seconds),
            timeout_action: Set(timeout_action.to_string()),
            network_bandwidth_up_mbps: Set(network_bandwidth_up_mbps),
            network_bandwidth_down_mbps: Set(network_bandwidth_down_mbps),
            ..Default::default()
        };
        let inserted = model.insert(self.db).await?;
        Ok(inserted.into())
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<WorkspaceTemplate>, sea_orm::DbErr> {
        let model = workspace_template::Entity::find_by_id(id).one(self.db).await?;
        Ok(model.map(|m| m.into()))
    }

    pub async fn list_by_owner(&self, owner_id: Uuid) -> Result<Vec<WorkspaceTemplate>, sea_orm::DbErr> {
        let models = workspace_template::Entity::find()
            .filter(workspace_template::Column::OwnerId.eq(owner_id))
            .order_by_asc(workspace_template::Column::CreatedAt)
            .all(self.db)
            .await?;
        Ok(models.into_iter().map(|m| m.into()).collect())
    }

    pub async fn list_all(&self) -> Result<Vec<WorkspaceTemplate>, sea_orm::DbErr> {
        let models = workspace_template::Entity::find()
            .order_by_asc(workspace_template::Column::CreatedAt)
            .all(self.db)
            .await?;
        Ok(models.into_iter().map(|m| m.into()).collect())
    }

    pub async fn count_instances(&self, template_id: Uuid) -> Result<i64, sea_orm::DbErr> {
        let count = workspace_instance::Entity::find()
            .filter(workspace_instance::Column::TemplateId.eq(template_id))
            .count(self.db)
            .await?;
        Ok(count as i64)
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
        remote_type: &str,
        container_runtime: &str,
        run_config: &serde_json::Value,
        exec_config: &serde_json::Value,
        volume_mappings: &serde_json::Value,
        persistent_storage_path: Option<&str>,
        max_run_seconds: Option<i64>,
        timeout_action: &str,
        network_bandwidth_up_mbps: i32,
        network_bandwidth_down_mbps: i32,
    ) -> Result<bool, sea_orm::DbErr> {
        let result = workspace_template::Entity::update(workspace_template::ActiveModel {
            id: Set(id),
            name: Set(name.to_string()),
            description: Set(description.map(|s| s.to_string())),
            image: Set(image.to_string()),
            cores: Set(cores),
            memory: Set(memory),
            gpu_count: Set(gpu_count),
            docker_registry: Set(docker_registry.map(|s| s.to_string())),
            remote_type: Set(remote_type.to_string()),
            container_runtime: Set(container_runtime.to_string()),
            run_config: Set(run_config.clone().into()),
            exec_config: Set(exec_config.clone().into()),
            volume_mappings: Set(volume_mappings.clone().into()),
            persistent_storage_path: Set(persistent_storage_path.map(|s| s.to_string())),
            max_run_seconds: Set(max_run_seconds),
            timeout_action: Set(timeout_action.to_string()),
            network_bandwidth_up_mbps: Set(network_bandwidth_up_mbps),
            network_bandwidth_down_mbps: Set(network_bandwidth_down_mbps),
            ..Default::default()
        })
        .filter(workspace_template::Column::Id.eq(id))
        .exec(self.db)
        .await;
        match result {
            Ok(_) => Ok(true),
            Err(sea_orm::DbErr::RecordNotFound(_)) | Err(sea_orm::DbErr::RecordNotUpdated) => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub async fn delete(&self, id: Uuid) -> Result<bool, sea_orm::DbErr> {
        let result = workspace_template::Entity::delete_by_id(id).exec(self.db).await?;
        Ok(result.rows_affected > 0)
    }
}

// ── Workspace Instance Repository ─────────────────────────────

pub struct WorkspaceInstanceRepository<'a> {
    pub db: &'a DatabaseConnection,
}

impl<'a> WorkspaceInstanceRepository<'a> {
    pub fn new(db: &'a DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn launch(
        &self,
        template_id: Uuid,
        owner_id: Uuid,
        template_name: &str,
        mount_persistent: bool,
        resolved_volume_host_path: Option<&str>,
    ) -> Result<WorkspaceInstance, sea_orm::DbErr> {
        let id = Uuid::new_v4();
        let access_token = generate_access_token();
        let access_password = generate_access_password();

        // Auto-generate instance name: "{template_name}-{next_number}"
        let max_number = workspace_instance::Entity::find()
            .filter(workspace_instance::Column::TemplateId.eq(template_id))
            .order_by(workspace_instance::Column::InstanceNumber, Order::Desc)
            .one(self.db)
            .await?
            .map(|m| m.instance_number)
            .unwrap_or(0);
        let next_number = max_number + 1;
        let name = format!("{}-{}", template_name, next_number);

        let model = workspace_instance::ActiveModel {
            id: Set(id),
            template_id: Set(template_id),
            name: Set(name),
            instance_number: Set(next_number),
            owner_id: Set(owner_id),
            container_id: Set(None),
            status: Set("stopped".to_string()),
            access_token: Set(access_token),
            access_password: Set(access_password),
            mount_persistent: Set(mount_persistent),
            resolved_volume_host_path: Set(resolved_volume_host_path.map(|s| s.to_string())),
            ..Default::default()
        };
        let inserted = model.insert(self.db).await?;
        Ok(inserted.into())
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<WorkspaceInstance>, sea_orm::DbErr> {
        let model = workspace_instance::Entity::find_by_id(id).one(self.db).await?;
        Ok(model.map(|m| m.into()))
    }

    pub async fn find_by_access_token(&self, token: &str) -> Result<Option<WorkspaceInstance>, sea_orm::DbErr> {
        let model = workspace_instance::Entity::find()
            .filter(workspace_instance::Column::AccessToken.eq(token))
            .one(self.db)
            .await?;
        Ok(model.map(|m| m.into()))
    }

    pub async fn list_by_owner(&self, owner_id: Uuid) -> Result<Vec<WorkspaceInstance>, sea_orm::DbErr> {
        let models = workspace_instance::Entity::find()
            .filter(workspace_instance::Column::OwnerId.eq(owner_id))
            .order_by_asc(workspace_instance::Column::CreatedAt)
            .all(self.db)
            .await?;
        Ok(models.into_iter().map(|m| m.into()).collect())
    }

    pub async fn list_by_status(&self, status: &str) -> Result<Vec<WorkspaceInstance>, sea_orm::DbErr> {
        let models = workspace_instance::Entity::find()
            .filter(workspace_instance::Column::Status.eq(status))
            .order_by_asc(workspace_instance::Column::CreatedAt)
            .all(self.db)
            .await?;
        Ok(models.into_iter().map(|m| m.into()).collect())
    }

    pub async fn list_all(&self) -> Result<Vec<WorkspaceInstance>, sea_orm::DbErr> {
        let models = workspace_instance::Entity::find()
            .order_by_asc(workspace_instance::Column::CreatedAt)
            .all(self.db)
            .await?;
        Ok(models.into_iter().map(|m| m.into()).collect())
    }

    pub async fn list_by_template(&self, template_id: Uuid) -> Result<Vec<WorkspaceInstance>, sea_orm::DbErr> {
        let models = workspace_instance::Entity::find()
            .filter(workspace_instance::Column::TemplateId.eq(template_id))
            .order_by_asc(workspace_instance::Column::CreatedAt)
            .all(self.db)
            .await?;
        Ok(models.into_iter().map(|m| m.into()).collect())
    }

    pub async fn update_status(&self, id: Uuid, status: &str) -> Result<bool, sea_orm::DbErr> {
        let result = workspace_instance::Entity::update(workspace_instance::ActiveModel {
            id: Set(id),
            status: Set(status.to_string()),
            ..Default::default()
        })
        .filter(workspace_instance::Column::Id.eq(id))
        .exec(self.db)
        .await;
        match result {
            Ok(_) => Ok(true),
            Err(sea_orm::DbErr::RecordNotFound(_)) | Err(sea_orm::DbErr::RecordNotUpdated) => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub async fn update_container_id(
        &self,
        id: Uuid,
        container_id: &str,
    ) -> Result<bool, sea_orm::DbErr> {
        let result = workspace_instance::Entity::update(workspace_instance::ActiveModel {
            id: Set(id),
            container_id: Set(Some(container_id.to_string())),
            ..Default::default()
        })
        .filter(workspace_instance::Column::Id.eq(id))
        .exec(self.db)
        .await;
        match result {
            Ok(_) => Ok(true),
            Err(sea_orm::DbErr::RecordNotFound(_)) | Err(sea_orm::DbErr::RecordNotUpdated) => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub async fn update_started_at(
        &self,
        id: Uuid,
        started_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<bool, sea_orm::DbErr> {
        let result = workspace_instance::Entity::update(workspace_instance::ActiveModel {
            id: Set(id),
            started_at: Set(started_at),
            ..Default::default()
        })
        .filter(workspace_instance::Column::Id.eq(id))
        .exec(self.db)
        .await;
        match result {
            Ok(_) => Ok(true),
            Err(sea_orm::DbErr::RecordNotFound(_)) | Err(sea_orm::DbErr::RecordNotUpdated) => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub async fn list_running_with_started_at(
        &self,
    ) -> Result<Vec<WorkspaceInstance>, sea_orm::DbErr> {
        let models = workspace_instance::Entity::find()
            .filter(workspace_instance::Column::Status.eq("running"))
            .filter(Expr::col(workspace_instance::Column::StartedAt).is_not_null())
            .order_by_asc(workspace_instance::Column::CreatedAt)
            .all(self.db)
            .await?;
        Ok(models.into_iter().map(|m| m.into()).collect())
    }

    pub async fn delete(&self, id: Uuid) -> Result<bool, sea_orm::DbErr> {
        let result = workspace_instance::Entity::delete_by_id(id).exec(self.db).await?;
        Ok(result.rows_affected > 0)
    }
}

// ── Registry Repository ───────────────────────────────────────

pub struct RegistryRepository<'a> {
    pub db: &'a DatabaseConnection,
}

impl<'a> RegistryRepository<'a> {
    pub fn new(db: &'a DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn get_url(&self) -> Result<Option<String>, sea_orm::DbErr> {
        let model = registry_config::Entity::find_by_id(1).one(self.db).await?;
        Ok(model.map(|m| m.registry_url))
    }

    pub async fn set_url(&self, url: &str) -> Result<(), sea_orm::DbErr> {
        let model = registry_config::ActiveModel {
            id: Set(1),
            registry_url: Set(url.to_string()),
            ..Default::default()
        };
        Insert::one(model)
            .on_conflict(
                OnConflict::column(registry_config::Column::Id)
                    .update_column(registry_config::Column::RegistryUrl)
                    .update_column(registry_config::Column::UpdatedAt)
                    .to_owned(),
            )
            .exec(self.db)
            .await?;
        Ok(())
    }

    pub async fn get_cached(&self) -> Result<Option<serde_json::Value>, sea_orm::DbErr> {
        let model = registry_cache::Entity::find_by_id(1).one(self.db).await?;
        Ok(model.map(|m| m.registry_json.into()))
    }

    pub async fn set_cached(&self, json: &serde_json::Value) -> Result<(), sea_orm::DbErr> {
        let model = registry_cache::ActiveModel {
            id: Set(1),
            registry_json: Set(json.clone().into()),
            ..Default::default()
        };
        Insert::one(model)
            .on_conflict(
                OnConflict::column(registry_cache::Column::Id)
                    .update_column(registry_cache::Column::RegistryJson)
                    .update_column(registry_cache::Column::SyncedAt)
                    .to_owned(),
            )
            .exec(self.db)
            .await?;
        Ok(())
    }
}
