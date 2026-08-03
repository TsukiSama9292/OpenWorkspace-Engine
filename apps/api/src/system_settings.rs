use sea_orm::sea_query::OnConflict;
use sea_orm::{DatabaseConnection, EntityTrait, Insert, Set};

/// Conservative default host capacity used when docker detection fails and no
/// `OW_HOST_*` environment override is set.
pub const DEFAULT_HOST_CPU_CORES: i32 = 8;
pub const DEFAULT_HOST_RAM_BYTES: i64 = 16 * 1024 * 1024 * 1024;

/// Host totals as reported by the Docker daemon (`docker info`) — the host's
/// CPU/RAM, not the API container's cgroup limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostCapacity {
    pub cpu_cores: i32,
    pub ram_bytes: i64,
}

/// Resolve the host capacity the `system_settings` singleton is seeded with.
/// Precedence: explicit `OW_HOST_CPU_CORES` / `OW_HOST_RAM_BYTES` env overrides
/// > docker-detected host capacity > conservative defaults.
pub fn resolve_host_capacity(
    env_cpu_cores: Option<i32>,
    env_ram_bytes: Option<i64>,
    detected: Option<HostCapacity>,
) -> HostCapacity {
    HostCapacity {
        cpu_cores: env_cpu_cores
            .or_else(|| detected.map(|d| d.cpu_cores))
            .unwrap_or(DEFAULT_HOST_CPU_CORES),
        ram_bytes: env_ram_bytes
            .or_else(|| detected.map(|d| d.ram_bytes))
            .unwrap_or(DEFAULT_HOST_RAM_BYTES),
    }
}

pub mod system_settings {
    use sea_orm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
    #[sea_orm(table_name = "system_settings")]
    pub struct Model {
        #[sea_orm(primary_key, auto_increment = false)]
        pub id: i32,
        pub max_cpu_cores: i32,
        pub max_ram_bytes: i64,
        pub host_instance_limit: i32,
        pub shared_max_cpu: i32,
        pub shared_max_ram: i64,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// The five editable host-capacity / global-policy values exposed by the admin
/// settings API.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SystemSettings {
    pub max_cpu_cores: i32,
    pub max_ram_bytes: i64,
    pub host_instance_limit: i32,
    pub shared_max_cpu: i32,
    pub shared_max_ram: i64,
}

impl From<system_settings::Model> for SystemSettings {
    fn from(m: system_settings::Model) -> Self {
        Self {
            max_cpu_cores: m.max_cpu_cores,
            max_ram_bytes: m.max_ram_bytes,
            host_instance_limit: m.host_instance_limit,
            shared_max_cpu: m.shared_max_cpu,
            shared_max_ram: m.shared_max_ram,
        }
    }
}

impl From<&SystemSettings> for system_settings::ActiveModel {
    fn from(s: &SystemSettings) -> Self {
        Self {
            id: Set(1),
            max_cpu_cores: Set(s.max_cpu_cores),
            max_ram_bytes: Set(s.max_ram_bytes),
            host_instance_limit: Set(s.host_instance_limit),
            shared_max_cpu: Set(s.shared_max_cpu),
            shared_max_ram: Set(s.shared_max_ram),
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
        let model = system_settings::Entity::find_by_id(1)
            .one(self.db)
            .await?;
        Ok(model.map(Into::into))
    }

    /// Upsert the singleton row (id = 1), returning the stored values.
    pub async fn upsert(
        &self,
        settings: &SystemSettings,
    ) -> Result<SystemSettings, sea_orm::DbErr> {
        Insert::one(system_settings::ActiveModel::from(settings))
            .on_conflict(
                OnConflict::column(system_settings::Column::Id)
                    .update_column(system_settings::Column::MaxCpuCores)
                    .update_column(system_settings::Column::MaxRamBytes)
                    .update_column(system_settings::Column::HostInstanceLimit)
                    .update_column(system_settings::Column::SharedMaxCpu)
                    .update_column(system_settings::Column::SharedMaxRam)
                    .to_owned(),
            )
            .exec(self.db)
            .await?;
        self.get()
            .await?
            .ok_or_else(|| sea_orm::DbErr::RecordNotFound("system_settings".into()))
    }

    /// Return the singleton row, creating it with conservative defaults when
    /// absent (the migration normally guarantees it exists). Used by the admin
    /// settings read path so the row always exists for the global lock target.
    pub async fn get_or_create(&self) -> Result<SystemSettings, sea_orm::DbErr> {
        if let Some(existing) = self.get().await? {
            return Ok(existing);
        }
        self.upsert(&SystemSettings {
            max_cpu_cores: DEFAULT_HOST_CPU_CORES,
            max_ram_bytes: DEFAULT_HOST_RAM_BYTES,
            host_instance_limit: 0,
            shared_max_cpu: 0,
            shared_max_ram: 0,
        })
        .await
    }

    /// Provision the singleton from the resolved host capacity at startup. The
    /// migration pre-inserts the row with conservative defaults so the global
    /// lock target always exists, so a missing row is the rare path. A row that
    /// still holds the pristine seed values is refreshed with the resolved
    /// capacity (first-startup auto-detection); an admin-edited row is left
    /// untouched so runtime edits persist across restarts.
    pub async fn seed_host_capacity(
        &self,
        capacity: &HostCapacity,
    ) -> Result<(), sea_orm::DbErr> {
        let pristine = |s: &SystemSettings| {
            s.max_cpu_cores == DEFAULT_HOST_CPU_CORES
                && s.max_ram_bytes == DEFAULT_HOST_RAM_BYTES
                && s.host_instance_limit == 0
                && s.shared_max_cpu == 0
                && s.shared_max_ram == 0
        };

        match self.get().await? {
            Some(existing) if pristine(&existing) => {
                self.upsert(&SystemSettings {
                    max_cpu_cores: capacity.cpu_cores,
                    max_ram_bytes: capacity.ram_bytes,
                    host_instance_limit: 0,
                    shared_max_cpu: 0,
                    shared_max_ram: 0,
                })
                .await?;
                Ok(())
            }
            Some(_) => Ok(()),
            None => {
                self.upsert(&SystemSettings {
                    max_cpu_cores: capacity.cpu_cores,
                    max_ram_bytes: capacity.ram_bytes,
                    host_instance_limit: 0,
                    shared_max_cpu: 0,
                    shared_max_ram: 0,
                })
                .await?;
                Ok(())
            }
        }
    }
}

/// Startup provisioning of the `system_settings` singleton: read the
/// `OW_HOST_CPU_CORES` / `OW_HOST_RAM_BYTES` overrides (if any), auto-detect
/// host capacity via `docker info`, and seed the row. Fail-open: a detection
/// failure logs a `WARN` and falls back to env values then conservative
/// defaults, and a DB failure is left to the caller to log — the API still
/// boots.
pub async fn seed_from_host(
    db: &DatabaseConnection,
    docker: &dyn crate::docker::DockerService,
) -> Result<(), sea_orm::DbErr> {
    let env_cpu_cores = std::env::var("OW_HOST_CPU_CORES")
        .ok()
        .and_then(|v| v.parse::<i32>().ok());
    let env_ram_bytes = std::env::var("OW_HOST_RAM_BYTES")
        .ok()
        .and_then(|v| v.parse::<i64>().ok());

    let detected = docker.host_capacity().await;
    if let Err(e) = &detected {
        tracing::warn!(
            "Failed to auto-detect host capacity via docker info: {}. Falling back to OW_HOST_CPU_CORES / OW_HOST_RAM_BYTES or conservative defaults ({} cores / {} GiB).",
            e,
            DEFAULT_HOST_CPU_CORES,
            DEFAULT_HOST_RAM_BYTES / (1024 * 1024 * 1024)
        );
    }

    let capacity = resolve_host_capacity(env_cpu_cores, env_ram_bytes, detected.ok());
    tracing::info!(
        "Provisioning system_settings with host capacity: {} cores / {} bytes RAM",
        capacity.cpu_cores,
        capacity.ram_bytes
    );
    SystemSettingsRepository::new(db)
        .seed_host_capacity(&capacity)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: i64 = 1024 * 1024 * 1024;

    fn detected() -> HostCapacity {
        HostCapacity {
            cpu_cores: 24,
            ram_bytes: 128 * GIB,
        }
    }

    #[test]
    fn env_overrides_detection() {
        let capacity = resolve_host_capacity(Some(4), Some(32 * GIB), Some(detected()));
        assert_eq!(capacity.cpu_cores, 4);
        assert_eq!(capacity.ram_bytes, 32 * GIB);
    }

    #[test]
    fn detection_used_when_no_env_override() {
        let capacity = resolve_host_capacity(None, None, Some(detected()));
        assert_eq!(capacity.cpu_cores, 24);
        assert_eq!(capacity.ram_bytes, 128 * GIB);
    }

    #[test]
    fn per_value_env_overrides_apply_independently() {
        let capacity = resolve_host_capacity(Some(2), None, Some(detected()));
        assert_eq!(capacity.cpu_cores, 2);
        assert_eq!(capacity.ram_bytes, 128 * GIB);

        let capacity = resolve_host_capacity(None, Some(48 * GIB), Some(detected()));
        assert_eq!(capacity.cpu_cores, 24);
        assert_eq!(capacity.ram_bytes, 48 * GIB);
    }

    #[test]
    fn conservative_defaults_when_detection_fails_and_no_env() {
        let capacity = resolve_host_capacity(None, None, None);
        assert_eq!(capacity.cpu_cores, DEFAULT_HOST_CPU_CORES);
        assert_eq!(capacity.ram_bytes, DEFAULT_HOST_RAM_BYTES);
    }

    #[test]
    fn env_only_without_detection() {
        let capacity = resolve_host_capacity(Some(6), Some(64 * GIB), None);
        assert_eq!(capacity.cpu_cores, 6);
        assert_eq!(capacity.ram_bytes, 64 * GIB);
    }

    #[test]
    fn defaults_match_migration_seed() {
        // The migration seeds id=1 with the same conservative defaults the
        // resolution falls back to, so the pristine-row refresh is coherent.
        assert_eq!(DEFAULT_HOST_CPU_CORES, 8);
        assert_eq!(DEFAULT_HOST_RAM_BYTES, 16 * GIB);
    }
}
