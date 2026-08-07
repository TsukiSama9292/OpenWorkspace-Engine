use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, Insert, Order, PaginatorTrait, QueryFilter, QueryOrder, Set, TryIntoModel};
use sea_orm::sea_query::{Expr, OnConflict};
use std::collections::HashMap;
use uuid::Uuid;

use crate::effective_context::{
    calculate_effective_context, EffectiveContext, GroupPolicy, TemplateVisibility, UserPolicy,
};

/// Instance statuses that count toward quota accounting — the Active Set
/// (spec Decision 2). Anything else (`stopped`, `error`) is inactive.
pub const ACTIVE_STATUSES: [&str; 3] = ["running", "starting", "paused"];

/// Registry status of a `persistent_volumes` row: `active` while at least one
/// active instance references its host path, `orphaned` once nothing does.
pub const VOLUME_STATUS_ACTIVE: &str = "active";
pub const VOLUME_STATUS_ORPHANED: &str = "orphaned";

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
        pub direct_max_instances: Option<i32>,
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
        pub keep_time_seconds: Option<i64>,
        pub keep_time_action: String,
        pub docker_in_instance: bool,
        pub visibility: String,
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
        pub host_port: Option<i32>,
        pub started_at: Option<DateTimeUtc>,
        pub last_seen_at: Option<DateTimeUtc>,
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

/// The `persistent_volumes` registry (spec Decision 7). One row per resolved
/// host data path; `owner_id` is nulled (never the row deleted) when the owner
/// user is deleted, and `status` flips between `active` and `orphaned` as
/// instances referencing the path come and go. Host data itself is only ever
/// removed by the explicit double-confirmed cleanup endpoint.
pub mod persistent_volume {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "persistent_volumes")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: Uuid,
        pub owner_id: Option<Uuid>,
        pub host_path: String,
        pub status: String,
        pub created_at: DateTimeUtc,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

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

pub mod group {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "groups")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: Uuid,
        pub name: String,
        pub description: Option<String>,
        /// `admin` | `manager` | `user` | `None` (custom). System groups are
        /// identified by this column, never by name (spec Decision 2/9).
        pub kind: Option<String>,
        pub can_create_template: bool,
        pub can_manage_users: bool,
        pub can_manage_group_instances: bool,
        pub can_manage_docker: bool,
        pub can_manage_registry: bool,
        pub can_view_monitoring: bool,
        /// `None` (NULL) means "unlimited" (the Admin group's ceiling).
        pub max_instances: Option<i32>,
        pub created_at: DateTimeUtc,
        pub updated_at: DateTimeUtc,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod user_group {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "user_groups")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub user_id: Uuid,
        #[sea_orm(primary_key)]
        pub group_id: Uuid,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

pub mod group_template {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "group_templates")]
    pub struct Model {
        #[sea_orm(primary_key)]
        pub group_id: Uuid,
        #[sea_orm(primary_key)]
        pub template_id: Uuid,
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
    pub keep_time_seconds: Option<i64>,
    pub keep_time_action: String,
    pub docker_in_instance: bool,
    pub visibility: TemplateVisibility,
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
            run_config: m.run_config,
            exec_config: m.exec_config,
            volume_mappings: m.volume_mappings,
            persistent_storage_path: m.persistent_storage_path,
            max_run_seconds: m.max_run_seconds,
            timeout_action: m.timeout_action,
            network_bandwidth_up_mbps: m.network_bandwidth_up_mbps,
            network_bandwidth_down_mbps: m.network_bandwidth_down_mbps,
            keep_time_seconds: m.keep_time_seconds,
            keep_time_action: m.keep_time_action,
            docker_in_instance: m.docker_in_instance,
            visibility: m.visibility.parse().unwrap_or_default(),
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
    pub host_port: Option<i32>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
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
            host_port: m.host_port,
            started_at: m.started_at,
            last_seen_at: m.last_seen_at,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }
}

// ── User Repository ───────────────────────────────────────────

/// A user row as returned by the repository read queries: the identity plus
/// the per-user flat-RBAC fields. This replaces the former positional tuples,
/// whose element order and count differed per query (`find_by_username`
/// omitted `created_at`, `list_all` omitted `password_hash`).
#[derive(Debug, Clone)]
pub struct UserRecord {
    pub id: Uuid,
    pub username: String,
    pub password_hash: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// A user row plus the policy rows the management surface exposes: the
/// personal instance ceiling, group memberships, and the derived tier /
/// admin status from the member-group kinds.
#[derive(Debug, Clone)]
pub struct UserWithPolicy {
    pub id: Uuid,
    pub username: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub direct_max_instances: Option<i32>,
    pub group_ids: Vec<Uuid>,
    pub is_admin: bool,
    pub tier: i32,
}

pub struct UserRepository<'a> {
    pub db: &'a DatabaseConnection,
}

impl<'a> UserRepository<'a> {
    pub fn new(db: &'a DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn seed_admin(&self, admin_password: &str) -> Result<(), sea_orm::DbErr> {
        // Root is Admin-group membership: seed the initial admin as a member of
        // the (migration-seeded) Admin group, or return once one exists.
        let Some(admin_group) = group::Entity::find()
            .filter(group::Column::Kind.eq(Some("admin".to_string())))
            .one(self.db)
            .await?
        else {
            return Ok(());
        };
        let already_root = user_group::Entity::find()
            .filter(user_group::Column::GroupId.eq(admin_group.id))
            .count(self.db)
            .await?;
        if already_root > 0 {
            return Ok(());
        }

        let admin_user_id = match user::Entity::find()
            .filter(user::Column::Username.eq("admin"))
            .one(self.db)
            .await?
        {
            Some(existing) => existing.id,
            None => {
                let password_hash =
                    bcrypt::hash(admin_password, 10).expect("Failed to hash admin password");
                let id = Uuid::new_v4();
                user::ActiveModel {
                    id: Set(id),
                    username: Set("admin".to_string()),
                    password_hash: Set(password_hash),
                    ..Default::default()
                }
                .insert(self.db)
                .await?;
                id
            }
        };

        user_group::ActiveModel {
            user_id: Set(admin_user_id),
            group_id: Set(admin_group.id),
        }
        .insert(self.db)
        .await?;

        tracing::info!("Seeded default admin user (username: admin)");
        Ok(())
    }

    pub async fn find_by_username(
        &self,
        username: &str,
    ) -> Result<Option<UserRecord>, sea_orm::DbErr> {
        let model = user::Entity::find()
            .filter(user::Column::Username.eq(username))
            .one(self.db)
            .await?;
        Ok(model.map(|m| UserRecord {
            id: m.id,
            username: m.username,
            password_hash: m.password_hash,
            created_at: m.created_at,
        }))
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<UserRecord>, sea_orm::DbErr> {
        let model = user::Entity::find_by_id(id).one(self.db).await?;
        Ok(model.map(|m| UserRecord {
            id: m.id,
            username: m.username,
            password_hash: m.password_hash,
            created_at: m.created_at,
        }))
    }

    pub async fn create(
        &self,
        username: &str,
        password_hash: &str,
    ) -> Result<Uuid, sea_orm::DbErr> {
        let id = Uuid::new_v4();
        let model = user::ActiveModel {
            id: Set(id),
            username: Set(username.to_string()),
            password_hash: Set(password_hash.to_string()),
            ..Default::default()
        };
        model.insert(self.db).await?;
        Ok(id)
    }

    pub async fn list_all(&self) -> Result<Vec<UserRecord>, sea_orm::DbErr> {
        let models = user::Entity::find()
            .order_by_asc(user::Column::CreatedAt)
            .all(self.db)
            .await?;
        Ok(models
            .into_iter()
            .map(|m| UserRecord {
                id: m.id,
                username: m.username,
                password_hash: m.password_hash,
                created_at: m.created_at,
            })
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

        model.update(self.db).await?;
        Ok(true)
    }

    /// The group ids a user is a member of, from the `user_groups` join table.
    /// Used for the flat-model "shares ≥1 group" instance scope and to expose
    /// an owner's groups on the instance JSON.
    pub async fn list_group_ids(&self, user_id: Uuid) -> Result<Vec<Uuid>, sea_orm::DbErr> {
        let rows = user_group::Entity::find()
            .filter(user_group::Column::UserId.eq(user_id))
            .all(self.db)
            .await?;
        Ok(rows.into_iter().map(|r| r.group_id).collect())
    }

    /// The distinct user ids that belong to any of the given groups. Empty
    /// input yields an empty result (no group, no members).
    pub async fn list_group_members(
        &self,
        group_ids: &[Uuid],
    ) -> Result<Vec<Uuid>, sea_orm::DbErr> {
        if group_ids.is_empty() {
            return Ok(Vec::new());
        }
        let rows = user_group::Entity::find()
            .filter(user_group::Column::GroupId.is_in(group_ids.to_vec()))
            .all(self.db)
            .await?;
        let mut members: Vec<Uuid> = rows.into_iter().map(|r| r.user_id).collect();
        members.sort_unstable();
        members.dedup();
        Ok(members)
    }

    /// A user row with its policy rows (personal ceiling, membership group
    /// ids, derived tier/admin), read in one pass so the management response
    /// reflects a just-applied policy change.
    pub async fn find_by_id_with_policy(
        &self,
        id: Uuid,
    ) -> Result<Option<UserWithPolicy>, sea_orm::DbErr> {
        let Some(model) = user::Entity::find_by_id(id).one(self.db).await? else {
            return Ok(None);
        };
        let group_ids = self.list_group_ids(id).await?;
        let (tier, is_admin) = self.derive_tier(&group_ids).await?;
        Ok(Some(UserWithPolicy {
            id: model.id,
            username: model.username,
            created_at: model.created_at,
            direct_max_instances: model.direct_max_instances,
            group_ids,
            is_admin,
            tier,
        }))
    }

    /// Every user with their policy rows, for the management list.
    pub async fn list_all_with_policy(&self) -> Result<Vec<UserWithPolicy>, sea_orm::DbErr> {
        let models = user::Entity::find()
            .order_by_asc(user::Column::CreatedAt)
            .all(self.db)
            .await?;
        let mut out = Vec::with_capacity(models.len());
        for model in models {
            let group_ids = self.list_group_ids(model.id).await?;
            let (tier, is_admin) = self.derive_tier(&group_ids).await?;
            out.push(UserWithPolicy {
                id: model.id,
                username: model.username,
                created_at: model.created_at,
                direct_max_instances: model.direct_max_instances,
                group_ids,
                is_admin,
                tier,
            });
        }
        Ok(out)
    }

    /// Derived tier (max kind-tier across member groups) and admin status
    /// (member of the Admin group) from a set of member-group ids.
    pub async fn derive_tier(
        &self,
        group_ids: &[Uuid],
    ) -> Result<(i32, bool), sea_orm::DbErr> {
        use crate::effective_context::{group_kind_tier, TIER_USER};
        if group_ids.is_empty() {
            return Ok((TIER_USER, false));
        }
        let groups = group::Entity::find()
            .filter(group::Column::Id.is_in(group_ids.to_vec()))
            .all(self.db)
            .await?;
        let is_admin = groups.iter().any(|g| g.kind.as_deref() == Some("admin"));
        let tier = groups
            .iter()
            .map(|g| group_kind_tier(g.kind.as_deref()))
            .max()
            .unwrap_or(TIER_USER);
        Ok((tier, is_admin))
    }

    /// Reconcile a user's group memberships to exactly the given group ids.
    pub async fn set_group_memberships(
        &self,
        user_id: Uuid,
        group_ids: &[Uuid],
    ) -> Result<(), sea_orm::DbErr> {
        user_group::Entity::delete_many()
            .filter(user_group::Column::UserId.eq(user_id))
            .exec(self.db)
            .await?;
        for &group_id in group_ids {
            user_group::ActiveModel {
                user_id: Set(user_id),
                group_id: Set(group_id),
            }
            .insert(self.db)
            .await?;
        }
        Ok(())
    }

    /// Set (or clear, with `None`) a user's personal instance ceiling. `None`
    /// in the column means "no personal override" per spec Decision 1.
    pub async fn set_direct_max_instances(
        &self,
        user_id: Uuid,
        direct_max_instances: Option<i32>,
    ) -> Result<bool, sea_orm::DbErr> {
        let result = user::Entity::update(user::ActiveModel {
            id: Set(user_id),
            direct_max_instances: Set(direct_max_instances),
            ..Default::default()
        })
        .filter(user::Column::Id.eq(user_id))
        .exec(self.db)
        .await;
        match result {
            Ok(_) => Ok(true),
            Err(sea_orm::DbErr::RecordNotFound(_)) | Err(sea_orm::DbErr::RecordNotUpdated) => Ok(false),
            Err(e) => Err(e),
        }
    }
}

// ── Group Repository ─────────────────────────────────────────

/// A group row as read from the `groups` table. The template whitelist lives
/// in `group_templates` and is read separately via `list_template_ids`.
#[derive(Debug, Clone)]
pub struct GroupRecord {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    /// `admin` | `manager` | `user` | `None` (custom groups).
    pub kind: Option<String>,
    pub can_create_template: bool,
    pub can_manage_users: bool,
    pub can_manage_group_instances: bool,
    pub can_manage_docker: bool,
    pub can_manage_registry: bool,
    pub can_view_monitoring: bool,
    pub max_instances: Option<i32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Group CRUD plus the group template whitelist. Group policy writes are
/// `is_admin`-only at the route layer; this repository never gates.
pub struct GroupRepository<'a> {
    pub db: &'a DatabaseConnection,
}

impl<'a> GroupRepository<'a> {
    pub fn new(db: &'a DatabaseConnection) -> Self {
        Self { db }
    }

    fn from_model(m: group::Model) -> GroupRecord {
        GroupRecord {
            id: m.id,
            name: m.name,
            description: m.description,
            kind: m.kind,
            can_create_template: m.can_create_template,
            can_manage_users: m.can_manage_users,
            can_manage_group_instances: m.can_manage_group_instances,
            can_manage_docker: m.can_manage_docker,
            can_manage_registry: m.can_manage_registry,
            can_view_monitoring: m.can_view_monitoring,
            max_instances: m.max_instances,
            created_at: m.created_at,
            updated_at: m.updated_at,
        }
    }

    pub async fn find_by_name(&self, name: &str) -> Result<Option<GroupRecord>, sea_orm::DbErr> {
        let model = group::Entity::find()
            .filter(group::Column::Name.eq(name))
            .one(self.db)
            .await?;
        Ok(model.map(Self::from_model))
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<GroupRecord>, sea_orm::DbErr> {
        let model = group::Entity::find_by_id(id).one(self.db).await?;
        Ok(model.map(Self::from_model))
    }

    /// The first system group with the given `kind` (e.g. the Admin group for
    /// the default template whitelist and `seed_admin`).
    pub async fn find_by_kind(&self, kind: &str) -> Result<Option<GroupRecord>, sea_orm::DbErr> {
        let model = group::Entity::find()
            .filter(group::Column::Kind.eq(Some(kind.to_string())))
            .one(self.db)
            .await?;
        Ok(model.map(Self::from_model))
    }

    pub async fn list_all(&self) -> Result<Vec<GroupRecord>, sea_orm::DbErr> {
        let models = group::Entity::find()
            .order_by_asc(group::Column::Name)
            .all(self.db)
            .await?;
        Ok(models.into_iter().map(Self::from_model).collect())
    }

    pub async fn create(
        &self,
        name: &str,
        description: Option<&str>,
        can_create_template: bool,
        can_manage_users: bool,
        can_manage_group_instances: bool,
        can_manage_docker: bool,
        can_manage_registry: bool,
        can_view_monitoring: bool,
        max_instances: i32,
    ) -> Result<Uuid, sea_orm::DbErr> {
        let id = Uuid::new_v4();
        let model = group::ActiveModel {
            id: Set(id),
            name: Set(name.to_string()),
            description: Set(description.map(|s| s.to_string())),
            kind: Set(None),
            can_create_template: Set(can_create_template),
            can_manage_users: Set(can_manage_users),
            can_manage_group_instances: Set(can_manage_group_instances),
            can_manage_docker: Set(can_manage_docker),
            can_manage_registry: Set(can_manage_registry),
            can_view_monitoring: Set(can_view_monitoring),
            max_instances: Set(Some(max_instances)),
            ..Default::default()
        };
        model.insert(self.db).await?;
        Ok(id)
    }

    pub async fn update(
        &self,
        id: Uuid,
        name: &str,
        description: Option<&str>,
        can_create_template: bool,
        can_manage_users: bool,
        can_manage_group_instances: bool,
        can_manage_docker: bool,
        can_manage_registry: bool,
        can_view_monitoring: bool,
        max_instances: Option<i32>,
    ) -> Result<bool, sea_orm::DbErr> {
        let result = group::Entity::update(group::ActiveModel {
            id: Set(id),
            name: Set(name.to_string()),
            description: Set(description.map(|s| s.to_string())),
            can_create_template: Set(can_create_template),
            can_manage_users: Set(can_manage_users),
            can_manage_group_instances: Set(can_manage_group_instances),
            can_manage_docker: Set(can_manage_docker),
            can_manage_registry: Set(can_manage_registry),
            can_view_monitoring: Set(can_view_monitoring),
            max_instances: Set(max_instances),
            ..Default::default()
        })
        .filter(group::Column::Id.eq(id))
        .exec(self.db)
        .await;
        match result {
            Ok(_) => Ok(true),
            Err(sea_orm::DbErr::RecordNotFound(_)) | Err(sea_orm::DbErr::RecordNotUpdated) => Ok(false),
            Err(e) => Err(e),
        }
    }

    pub async fn delete(&self, id: Uuid) -> Result<bool, sea_orm::DbErr> {
        let result = group::Entity::delete_by_id(id).exec(self.db).await?;
        Ok(result.rows_affected > 0)
    }

    /// The template ids whitelisted for the group.
    pub async fn list_template_ids(&self, group_id: Uuid) -> Result<Vec<Uuid>, sea_orm::DbErr> {
        let rows = group_template::Entity::find()
            .filter(group_template::Column::GroupId.eq(group_id))
            .all(self.db)
            .await?;
        Ok(rows.into_iter().map(|r| r.template_id).collect())
    }

    /// Reconcile the group's template whitelist to exactly the given ids.
    pub async fn set_template_ids(
        &self,
        group_id: Uuid,
        template_ids: &[Uuid],
    ) -> Result<(), sea_orm::DbErr> {
        group_template::Entity::delete_many()
            .filter(group_template::Column::GroupId.eq(group_id))
            .exec(self.db)
            .await?;
        for &template_id in template_ids {
            group_template::ActiveModel {
                group_id: Set(group_id),
                template_id: Set(template_id),
            }
            .insert(self.db)
            .await?;
        }
        Ok(())
    }
}

/// Whether every given group id exists. Used before reconciling memberships so
/// a bad list gets a 400 instead of a foreign-key 500.
pub async fn validate_group_ids(
    db: &DatabaseConnection,
    ids: &[Uuid],
) -> Result<bool, sea_orm::DbErr> {
    if ids.is_empty() {
        return Ok(true);
    }
    let count = group::Entity::find()
        .filter(group::Column::Id.is_in(ids.to_vec()))
        .count(db)
        .await?;
    Ok(count as usize == ids.len())
}

/// Whether every given template id exists. Used before reconciling the group
/// template whitelist.
pub async fn validate_template_ids(
    db: &DatabaseConnection,
    ids: &[Uuid],
) -> Result<bool, sea_orm::DbErr> {
    if ids.is_empty() {
        return Ok(true);
    }
    let count = workspace_template::Entity::find()
        .filter(workspace_template::Column::Id.is_in(ids.to_vec()))
        .count(db)
        .await?;
    Ok(count as usize == ids.len())
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
        keep_time_seconds: Option<i64>,
        keep_time_action: &str,
        docker_in_instance: bool,
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
            run_config: Set(run_config.clone()),
            exec_config: Set(exec_config.clone()),
            volume_mappings: Set(volume_mappings.clone()),
            persistent_storage_path: Set(persistent_storage_path.map(|s| s.to_string())),
            max_run_seconds: Set(max_run_seconds),
            timeout_action: Set(timeout_action.to_string()),
            network_bandwidth_up_mbps: Set(network_bandwidth_up_mbps),
            network_bandwidth_down_mbps: Set(network_bandwidth_down_mbps),
            keep_time_seconds: Set(keep_time_seconds),
            keep_time_action: Set(keep_time_action.to_string()),
            docker_in_instance: Set(docker_in_instance),
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
        keep_time_seconds: Option<i64>,
        keep_time_action: &str,
        docker_in_instance: bool,
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
            run_config: Set(run_config.clone()),
            exec_config: Set(exec_config.clone()),
            volume_mappings: Set(volume_mappings.clone()),
            persistent_storage_path: Set(persistent_storage_path.map(|s| s.to_string())),
            max_run_seconds: Set(max_run_seconds),
            timeout_action: Set(timeout_action.to_string()),
            network_bandwidth_up_mbps: Set(network_bandwidth_up_mbps),
            network_bandwidth_down_mbps: Set(network_bandwidth_down_mbps),
            keep_time_seconds: Set(keep_time_seconds),
            keep_time_action: Set(keep_time_action.to_string()),
            docker_in_instance: Set(docker_in_instance),
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

    /// Change a template's launch visibility in place. This is the only writer
    /// for the `visibility` column — `create`/`update` leave it untouched and
    /// the DB default (`private`) applies on insert (template-visibility spec
    /// Decision 1).
    pub async fn set_visibility(
        &self,
        id: Uuid,
        visibility: TemplateVisibility,
    ) -> Result<bool, sea_orm::DbErr> {
        let result = workspace_template::Entity::update(workspace_template::ActiveModel {
            id: Set(id),
            visibility: Set(visibility.as_str().to_string()),
            ..Default::default()
        })
        .filter(workspace_template::Column::Id.eq(id))
        .exec(self.db)
        .await;
        match result {
            Ok(_) => Ok(true),
            Err(sea_orm::DbErr::RecordNotFound(_)) | Err(sea_orm::DbErr::RecordNotUpdated) => {
                Ok(false)
            }
            Err(e) => Err(e),
        }
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

    /// Find an existing persistent (`mount_persistent = true`) instance for a
    /// (template, owner) pair, if any. Used to enforce the one-persistent
    /// instance per template-and-user rule at launch.
    pub async fn find_persistent_by_template_and_owner(
        &self,
        template_id: Uuid,
        owner_id: Uuid,
    ) -> Result<Option<WorkspaceInstance>, sea_orm::DbErr> {
        let model = workspace_instance::Entity::find()
            .filter(workspace_instance::Column::TemplateId.eq(template_id))
            .filter(workspace_instance::Column::OwnerId.eq(owner_id))
            .filter(workspace_instance::Column::MountPersistent.eq(true))
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

    /// Instances owned by any of the given users (the same-group scope for
    /// `can_manage_group_instances` holders). Empty input yields an empty
    /// result.
    pub async fn list_by_owner_ids(&self, owner_ids: &[Uuid]) -> Result<Vec<WorkspaceInstance>, sea_orm::DbErr> {
        if owner_ids.is_empty() {
            return Ok(Vec::new());
        }
        let models = workspace_instance::Entity::find()
            .filter(workspace_instance::Column::OwnerId.is_in(owner_ids.to_vec()))
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

    /// The Monitor-dashboard active set: instances the sampler should read
    /// stats for and the snapshot endpoint should list (`running` / `starting`
    /// / `paused`). `stopped` / `error` instances are excluded.
    pub async fn list_active_for_monitoring(
        &self,
    ) -> Result<Vec<WorkspaceInstance>, sea_orm::DbErr> {
        let models = workspace_instance::Entity::find()
            .filter(
                workspace_instance::Column::Status
                    .is_in(["running", "starting", "paused"]),
            )
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

    /// Persist a (re)resolved persistent host path on an Instance. Used to
    /// backfill legacy `mount_persistent = true` records whose path was never
    /// stored, on their first restart.
    pub async fn update_resolved_volume_host_path(
        &self,
        id: Uuid,
        host_path: Option<&str>,
    ) -> Result<bool, sea_orm::DbErr> {
        let result = workspace_instance::Entity::update(workspace_instance::ActiveModel {
            id: Set(id),
            resolved_volume_host_path: Set(host_path.map(|s| s.to_string())),
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

    /// All host ports currently allocated to non-deleted instances, i.e. the
    /// live set the pure allocator must not hand out again.
    pub async fn list_host_ports(&self) -> Result<Vec<i32>, sea_orm::DbErr> {
        let results = workspace_instance::Entity::find()
            .filter(workspace_instance::Column::HostPort.is_not_null())
            .all(self.db)
            .await?;
        Ok(results.into_iter().filter_map(|m| m.host_port).collect())
    }

    /// Commit (or clear) an instance's host port allocation. The UNIQUE index
    /// on `host_port` is the concurrency arbiter: two concurrent launches
    /// cannot both win the same port.
    pub async fn update_host_port(
        &self,
        id: Uuid,
        host_port: Option<i32>,
    ) -> Result<bool, sea_orm::DbErr> {
        let result = workspace_instance::Entity::update(workspace_instance::ActiveModel {
            id: Set(id),
            host_port: Set(host_port),
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

    pub async fn update_last_seen_at(
        &self,
        id: Uuid,
        last_seen_at: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<bool, sea_orm::DbErr> {
        let result = workspace_instance::Entity::update(workspace_instance::ActiveModel {
            id: Set(id),
            last_seen_at: Set(last_seen_at),
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

    pub async fn list_running_with_last_seen_at(
        &self,
    ) -> Result<Vec<WorkspaceInstance>, sea_orm::DbErr> {
        let models = workspace_instance::Entity::find()
            .filter(workspace_instance::Column::Status.eq("running"))
            .filter(Expr::col(workspace_instance::Column::LastSeenAt).is_not_null())
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
        Ok(model.map(|m| m.registry_json))
    }

    pub async fn set_cached(&self, json: &serde_json::Value) -> Result<(), sea_orm::DbErr> {
        let model = registry_cache::ActiveModel {
            id: Set(1),
            registry_json: Set(json.clone()),
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

// ── Persistent-Volume Registry Repository ─────────────────────

/// A `persistent_volumes` registry row as exposed to the API. The owner's
/// username is resolved by the route layer (join-free), so it is not part of
/// this record.
#[derive(Debug, Clone)]
pub struct PersistentVolume {
    pub id: Uuid,
    pub owner_id: Option<Uuid>,
    pub host_path: String,
    pub status: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

impl From<persistent_volume::Model> for PersistentVolume {
    fn from(m: persistent_volume::Model) -> Self {
        Self {
            id: m.id,
            owner_id: m.owner_id,
            host_path: m.host_path,
            status: m.status,
            created_at: m.created_at,
        }
    }
}

/// Registry CRUD. The lifecycle rules live here so every call site shares
/// them: upsert keyed by host path on launch, orphan-flip on instance delete,
/// and row removal only from the explicit cleanup endpoint. No method here
/// ever removes host data — that is the `DockerService` seam's job, invoked
/// solely by the cleanup route.
pub struct PersistentVolumeRepository<'a> {
    pub db: &'a DatabaseConnection,
}

impl<'a> PersistentVolumeRepository<'a> {
    pub fn new(db: &'a DatabaseConnection) -> Self {
        Self { db }
    }

    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<PersistentVolume>, sea_orm::DbErr> {
        let model = persistent_volume::Entity::find_by_id(id).one(self.db).await?;
        Ok(model.map(|m| m.into()))
    }

    pub async fn find_by_host_path(
        &self,
        host_path: &str,
    ) -> Result<Option<PersistentVolume>, sea_orm::DbErr> {
        let model = persistent_volume::Entity::find()
            .filter(persistent_volume::Column::HostPath.eq(host_path))
            .one(self.db)
            .await?;
        Ok(model.map(|m| m.into()))
    }

    /// Record (or re-activate) the registry row for a host path on a
    /// persistent launch. Keyed by the resolved host path — a re-launch of the
    /// same template by the same owner reuses the row and flips it back to
    /// `active`; a path never used before inserts a fresh row. The owner is
    /// always set to the launching user (the path embeds the owner id).
    pub async fn upsert(
        &self,
        host_path: &str,
        owner_id: Uuid,
    ) -> Result<PersistentVolume, sea_orm::DbErr> {
        let existing = self.find_by_host_path(host_path).await?;
        let model = match existing {
            Some(volume) => persistent_volume::ActiveModel {
                id: Set(volume.id),
                owner_id: Set(Some(owner_id)),
                status: Set(VOLUME_STATUS_ACTIVE.to_string()),
                ..Default::default()
            },
            None => persistent_volume::ActiveModel {
                owner_id: Set(Some(owner_id)),
                host_path: Set(host_path.to_string()),
                status: Set(VOLUME_STATUS_ACTIVE.to_string()),
                ..Default::default()
            },
        };
        let model = model.save(self.db).await?.try_into_model()?;
        Ok(model.into())
    }

    /// The registry rows with no referencing active instance, oldest first —
    /// exactly the set the orphaned-volumes view shows.
    pub async fn list_orphaned(&self) -> Result<Vec<PersistentVolume>, sea_orm::DbErr> {
        let models = persistent_volume::Entity::find()
            .filter(persistent_volume::Column::Status.eq(VOLUME_STATUS_ORPHANED))
            .order_by_asc(persistent_volume::Column::CreatedAt)
            .all(self.db)
            .await?;
        Ok(models.into_iter().map(|m| m.into()).collect())
    }

    /// Recompute a row's status from the instances that still reference its
    /// host path: `active` while any active instance references it, `orphaned`
    /// once none does. Called after an instance delete removes the last
    /// reference. No-op when no row exists for the path.
    pub async fn sync_status_for_host_path(&self, host_path: &str) -> Result<(), sea_orm::DbErr> {
        let Some(volume) = self.find_by_host_path(host_path).await? else {
            return Ok(());
        };
        let referencing = workspace_instance::Entity::find()
            .filter(workspace_instance::Column::ResolvedVolumeHostPath.eq(host_path))
            .filter(workspace_instance::Column::Status.is_in(ACTIVE_STATUSES))
            .count(self.db)
            .await?;
        let target = if referencing == 0 {
            VOLUME_STATUS_ORPHANED
        } else {
            VOLUME_STATUS_ACTIVE
        };
        if volume.status != target {
            persistent_volume::ActiveModel {
                id: Set(volume.id),
                status: Set(target.to_string()),
                ..Default::default()
            }
            .update(self.db)
            .await?;
        }
        Ok(())
    }

    /// Remove the registry row itself. Host data is *not* touched here — the
    /// cleanup route calls the Docker seam first and only then removes the row.
    pub async fn delete(&self, id: Uuid) -> Result<bool, sea_orm::DbErr> {
        let result = persistent_volume::Entity::delete_by_id(id).exec(self.db).await?;
        Ok(result.rows_affected > 0)
    }
}

// ── Effective-Context Policy Repository ───────────────────────

/// Reads for the effective-context computation: the user's personal config,
/// their group memberships, and both template whitelists. The policy decision
/// itself stays in the pure `effective_context` module.
pub struct PolicyRepository<'a> {
    pub db: &'a DatabaseConnection,
}

impl<'a> PolicyRepository<'a> {
    pub fn new(db: &'a DatabaseConnection) -> Self {
        Self { db }
    }

    /// Resolve a user's effective context from the database on every call.
    /// Returns `None` when the user row is missing (e.g. deleted after a token
    /// was issued). The context is recomputed per request, so group edits take
    /// effect on the very next call without re-authentication.
    pub async fn load_effective_context(
        &self,
        user_id: Uuid,
    ) -> Result<Option<EffectiveContext>, sea_orm::DbErr> {
        let Some(user) = user::Entity::find_by_id(user_id).one(self.db).await? else {
            return Ok(None);
        };

        let memberships = user_group::Entity::find()
            .filter(user_group::Column::UserId.eq(user_id))
            .all(self.db)
            .await?;
        let member_group_ids: Vec<Uuid> = memberships.iter().map(|m| m.group_id).collect();

        let groups = group::Entity::find()
            .filter(group::Column::Id.is_in(member_group_ids))
            .order_by_asc(group::Column::Name)
            .all(self.db)
            .await?;

        let mut group_template_ids: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
        let mut whitelisted_template_ids: Vec<Uuid> = Vec::new();
        for group_id in groups.iter().map(|g| g.id) {
            let rows = group_template::Entity::find()
                .filter(group_template::Column::GroupId.eq(group_id))
                .all(self.db)
                .await?;
            let ids: Vec<Uuid> = rows.into_iter().map(|r| r.template_id).collect();
            whitelisted_template_ids.extend(ids.iter().copied());
            group_template_ids.insert(group_id, ids);
        }

        // Hidden templates never enter the effective whitelist: collect the ids
        // of any whitelisted template whose visibility is `hidden` so the pure
        // policy engine can strip them (template-visibility spec Decision 3).
        let mut hidden_template_ids: Vec<Uuid> = Vec::new();
        if !whitelisted_template_ids.is_empty() {
            let hidden_rows = workspace_template::Entity::find()
                .filter(
                    workspace_template::Column::Id
                        .is_in(whitelisted_template_ids)
                        .and(workspace_template::Column::Visibility.eq(TemplateVisibility::Hidden.as_str())),
                )
                .all(self.db)
                .await?;
            hidden_template_ids = hidden_rows.into_iter().map(|t| t.id).collect();
        }

        let user_policy = UserPolicy {
            user_id: user.id,
            username: user.username.clone(),
            direct_max_instances: user.direct_max_instances,
        };
        let group_policies: Vec<GroupPolicy> = groups
            .into_iter()
            .map(|g| GroupPolicy {
                id: g.id,
                kind: g.kind,
                max_instances: g.max_instances,
                can_create_template: g.can_create_template,
                can_manage_users: g.can_manage_users,
                can_manage_group_instances: g.can_manage_group_instances,
                can_manage_docker: g.can_manage_docker,
                can_manage_registry: g.can_manage_registry,
                can_view_monitoring: g.can_view_monitoring,
            })
            .collect();

        Ok(Some(calculate_effective_context(
            &user_policy,
            &group_policies,
            &group_template_ids,
            &hidden_template_ids,
        )))
    }

    /// The derived tier (0/1/2) of a user from their group memberships. Used by
    /// the tier guardrails, which compare the actor's tier against the
    /// target's. Returns `None` when the user row is missing.
    pub async fn load_user_tier(&self, user_id: Uuid) -> Result<Option<i32>, sea_orm::DbErr> {
        let exists = user::Entity::find_by_id(user_id).one(self.db).await?;
        if exists.is_none() {
            return Ok(None);
        }
        let memberships = user_group::Entity::find()
            .filter(user_group::Column::UserId.eq(user_id))
            .all(self.db)
            .await?;
        let member_group_ids: Vec<Uuid> = memberships.iter().map(|m| m.group_id).collect();
        let groups = group::Entity::find()
            .filter(group::Column::Id.is_in(member_group_ids))
            .all(self.db)
            .await?;
        let tier = groups
            .iter()
            .map(|g| crate::effective_context::group_kind_tier(g.kind.as_deref()))
            .max()
            .unwrap_or(crate::effective_context::TIER_USER);
        Ok(Some(tier))
    }
}
