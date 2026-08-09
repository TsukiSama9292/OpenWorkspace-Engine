use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Expand → contract (resource-quotas spec): add group resource pools and the
/// billing-model switch, the per-instance billing bookkeeping (which group the
/// instance bills against, the frozen pool snapshot, and the actual host
/// resources it consumed), and the host-wide resource ceilings. Existing
/// instances keep `owner_group_id = NULL` (self-billing, unlimited resources),
/// so nothing changes for pre-existing tenants on upgrade.
///
/// Conventions: a pool/cap of `0` means "unlimited", matching the existing
/// `max_instances` and `host_instance_limit` semantics. The usage columns are
/// real host resources (integral cpu cores, MB of memory, gpu count) so billing
/// stays exact without floating-point sums.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        // Group resource pools: each group owns a CPU / memory / GPU pool its
        // members bill against. `billing_model` chooses between the shared pool
        // (all members' active instances sum together) and dedicated (each
        // member's own instances sum against the pool).
        conn.execute_unprepared(
            r#"
            ALTER TABLE groups
                ADD COLUMN billing_model VARCHAR(16) NOT NULL DEFAULT 'shared',
                ADD COLUMN pool_cpu_cores INT NOT NULL DEFAULT 0,
                ADD COLUMN pool_memory_mb INT NOT NULL DEFAULT 0,
                ADD COLUMN pool_gpu_count INT NOT NULL DEFAULT 0
            "#,
        )
        .await?;

        // Per-instance billing bookkeeping. `owner_group_id` is the group the
        // instance bills against; `NULL` means self-billed (unlimited, the
        // pre-quota behavior). `billing_group_snapshot` freezes the pool caps in
        // effect at launch so the UI can report what the instance was launched
        // under even after the group is reconfigured. The `host_*` columns are
        // the actual resources the instance consumed.
        conn.execute_unprepared(
            r#"
            ALTER TABLE workspace_instances
                ADD COLUMN owner_group_id UUID REFERENCES groups(id) ON DELETE SET NULL,
                ADD COLUMN billing_group_snapshot JSONB,
                ADD COLUMN host_cpu_cores INT NOT NULL DEFAULT 0,
                ADD COLUMN host_memory_mb BIGINT NOT NULL DEFAULT 0,
                ADD COLUMN host_gpu_count INT NOT NULL DEFAULT 0
            "#,
        )
        .await?;

        // Host-wide resource ceilings (`0` = disabled), the resource analog of
        // the existing `host_instance_limit`.
        conn.execute_unprepared(
            r#"
            ALTER TABLE system_settings
                ADD COLUMN host_cpu_cores INT NOT NULL DEFAULT 0,
                ADD COLUMN host_memory_mb BIGINT NOT NULL DEFAULT 0,
                ADD COLUMN host_gpu_count INT NOT NULL DEFAULT 0
            "#,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared("ALTER TABLE workspace_instances DROP COLUMN owner_group_id")
            .await?;
        conn.execute_unprepared("ALTER TABLE workspace_instances DROP COLUMN billing_group_snapshot")
            .await?;
        conn.execute_unprepared("ALTER TABLE workspace_instances DROP COLUMN host_cpu_cores")
            .await?;
        conn.execute_unprepared("ALTER TABLE workspace_instances DROP COLUMN host_memory_mb")
            .await?;
        conn.execute_unprepared("ALTER TABLE workspace_instances DROP COLUMN host_gpu_count")
            .await?;

        conn.execute_unprepared("ALTER TABLE groups DROP COLUMN billing_model").await?;
        conn.execute_unprepared("ALTER TABLE groups DROP COLUMN pool_cpu_cores").await?;
        conn.execute_unprepared("ALTER TABLE groups DROP COLUMN pool_memory_mb").await?;
        conn.execute_unprepared("ALTER TABLE groups DROP COLUMN pool_gpu_count").await?;

        conn.execute_unprepared("ALTER TABLE system_settings DROP COLUMN host_cpu_cores")
            .await?;
        conn.execute_unprepared("ALTER TABLE system_settings DROP COLUMN host_memory_mb")
            .await?;
        conn.execute_unprepared("ALTER TABLE system_settings DROP COLUMN host_gpu_count")
            .await?;

        Ok(())
    }
}
