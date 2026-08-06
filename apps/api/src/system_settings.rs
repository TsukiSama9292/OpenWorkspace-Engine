use sea_orm::sea_query::OnConflict;
use sea_orm::{DatabaseConnection, EntityTrait, Insert, Set};

pub mod entity {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "system_settings")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: i32,
        pub host_instance_limit: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// The single global-policy knob exposed by the admin settings API: the host
/// instance ceiling (`0` = unlimited). The host-capacity / shared-fuse fields
/// were dropped with the old quota pipeline.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, utoipa::ToSchema)]
pub struct SystemSettings {
    pub host_instance_limit: i32,
}

impl From<entity::Model> for SystemSettings {
    fn from(m: entity::Model) -> Self {
        Self {
            host_instance_limit: m.host_instance_limit,
        }
    }
}

impl From<&SystemSettings> for entity::ActiveModel {
    fn from(s: &SystemSettings) -> Self {
        Self {
            id: Set(1),
            host_instance_limit: Set(s.host_instance_limit),
        }
    }
}

/// Single-row repository for the `system_settings` singleton (id = 1). The row
/// doubles as the global lock target for quota checks, so the migration creates
/// it and every upsert below keeps it present.
pub struct SystemSettingsRepository<'a> {
    pub db: &'a DatabaseConnection,
}

impl<'a> SystemSettingsRepository<'a> {
    pub fn new(db: &'a DatabaseConnection) -> Self {
        Self { db }
    }

    /// Fetch the singleton row, or `None` when absent.
    pub async fn get(&self) -> Result<Option<SystemSettings>, sea_orm::DbErr> {
        let model = entity::Entity::find_by_id(1)
            .one(self.db)
            .await?;
        Ok(model.map(Into::into))
    }

    /// Upsert the singleton row (id = 1), returning the stored values.
    pub async fn upsert(
        &self,
        settings: &SystemSettings,
    ) -> Result<SystemSettings, sea_orm::DbErr> {
        Insert::one(entity::ActiveModel::from(settings))
            .on_conflict(
                OnConflict::column(entity::Column::Id)
                    .update_column(entity::Column::HostInstanceLimit)
                    .to_owned(),
            )
            .exec(self.db)
            .await?;
        self.get()
            .await?
            .ok_or_else(|| sea_orm::DbErr::RecordNotFound("system_settings".into()))
    }

    /// Return the singleton row, creating it with the default unlimited ceiling
    /// when absent (the migration normally guarantees it exists). Used by the
    /// admin settings read path and the launch pre-flight so the row always
    /// exists for the global lock target.
    pub async fn get_or_create(&self) -> Result<SystemSettings, sea_orm::DbErr> {
        if let Some(existing) = self.get().await? {
            return Ok(existing);
        }
        self.upsert(&SystemSettings {
            host_instance_limit: 0,
        })
        .await
    }
}
