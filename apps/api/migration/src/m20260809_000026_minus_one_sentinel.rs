use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Sentinel flip (resource-quotas spec Decision 1): the "unlimited / disabled"
/// marker moves from `0` (legacy, shared with the real value `0`) to `-1`
/// everywhere, so `0` becomes a real value everywhere.
///
/// The rewrite is behavior-preserving: every `0` that previously meant
/// "unlimited" (template resources/bandwidth, group pool/caps, host caps, the
/// host instance ceiling, personal ceilings) and every `NULL` that meant
/// "disabled" (auto-sleep/keep-time, the Admin group's ceiling) becomes `-1`.
/// Template `gpu_count` keeps `0` as a real zero request. `users
/// .direct_max_instances` stays nullable (`NULL` = inherit the group ceiling).
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        // Template resources / bandwidth: legacy `0` = no Docker limit →
        // `-1` = unlimited. `gpu_count` is intentionally untouched (`0` GPUs is
        // a real zero request, not unlimited).
        conn.execute_unprepared(
            r#"
            UPDATE workspace_templates
            SET cores = -1, memory = -1,
                network_bandwidth_up_mbps = -1,
                network_bandwidth_down_mbps = -1
            WHERE cores = 0 OR memory = 0 OR network_bandwidth_up_mbps = 0
               OR network_bandwidth_down_mbps = 0
            "#,
        )
        .await?;

        // Auto-sleep / keep-time: `NULL` (disabled) → `-1` (disabled), then
        // make `-1` the column default and forbid `NULL` going forward.
        conn.execute_unprepared(
            r#"
            UPDATE workspace_templates
            SET max_run_seconds = -1, keep_time_seconds = -1
            WHERE max_run_seconds IS NULL OR keep_time_seconds IS NULL
            "#,
        )
        .await?;
        conn.execute_unprepared(
            "ALTER TABLE workspace_templates ALTER COLUMN max_run_seconds SET DEFAULT -1",
        )
        .await?;
        conn.execute_unprepared(
            "ALTER TABLE workspace_templates ALTER COLUMN max_run_seconds SET NOT NULL",
        )
        .await?;
        conn.execute_unprepared(
            "ALTER TABLE workspace_templates ALTER COLUMN keep_time_seconds SET DEFAULT -1",
        )
        .await?;
        conn.execute_unprepared(
            "ALTER TABLE workspace_templates ALTER COLUMN keep_time_seconds SET NOT NULL",
        )
        .await?;

        // Group instance ceiling: the Admin group's `NULL` (unlimited) and any
        // legacy `0` (unlimited) → `-1`; `-1` becomes the default and `NULL`
        // is forbidden.
        conn.execute_unprepared(
            r#"
            UPDATE groups SET max_instances = -1
            WHERE max_instances IS NULL OR max_instances = 0
            "#,
        )
        .await?;
        conn.execute_unprepared("ALTER TABLE groups ALTER COLUMN max_instances SET DEFAULT -1")
            .await?;
        conn.execute_unprepared("ALTER TABLE groups ALTER COLUMN max_instances SET NOT NULL")
            .await?;

        // Group resource pools (added in 000025 with legacy `0` = unlimited).
        conn.execute_unprepared(
            r#"
            UPDATE groups
            SET pool_cpu_cores = -1, pool_memory_mb = -1, pool_gpu_count = -1
            WHERE pool_cpu_cores = 0 OR pool_memory_mb = 0 OR pool_gpu_count = 0
            "#,
        )
        .await?;
        conn.execute_unprepared("ALTER TABLE groups ALTER COLUMN pool_cpu_cores SET DEFAULT -1")
            .await?;
        conn.execute_unprepared("ALTER TABLE groups ALTER COLUMN pool_memory_mb SET DEFAULT -1")
            .await?;
        conn.execute_unprepared("ALTER TABLE groups ALTER COLUMN pool_gpu_count SET DEFAULT -1")
            .await?;

        // Host-wide ceilings: instance ceiling and resource caps, all legacy
        // `0` = unlimited → `-1`.
        conn.execute_unprepared(
            "UPDATE system_settings SET host_instance_limit = -1 WHERE host_instance_limit = 0",
        )
        .await?;
        conn.execute_unprepared(
            "ALTER TABLE system_settings ALTER COLUMN host_instance_limit SET DEFAULT -1",
        )
        .await?;
        conn.execute_unprepared(
            r#"
            UPDATE system_settings
            SET host_cpu_cores = -1, host_memory_mb = -1, host_gpu_count = -1
            WHERE host_cpu_cores = 0 OR host_memory_mb = 0 OR host_gpu_count = 0
            "#,
        )
        .await?;
        conn.execute_unprepared("ALTER TABLE system_settings ALTER COLUMN host_cpu_cores SET DEFAULT -1")
            .await?;
        conn.execute_unprepared("ALTER TABLE system_settings ALTER COLUMN host_memory_mb SET DEFAULT -1")
            .await?;
        conn.execute_unprepared("ALTER TABLE system_settings ALTER COLUMN host_gpu_count SET DEFAULT -1")
            .await?;

        // Personal instance ceiling: legacy `0` = unlimited → `-1`; stays
        // nullable (`NULL` = inherit the group ceiling). `-1` becomes the
        // default for fresh inserts.
        conn.execute_unprepared(
            "UPDATE users SET direct_max_instances = -1 WHERE direct_max_instances = 0",
        )
        .await?;
        conn.execute_unprepared("ALTER TABLE users ALTER COLUMN direct_max_instances SET DEFAULT -1")
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        // Best-effort reversion: restore the legacy `0` = unlimited default
        // shape. Data is left as-is (`-1` is a valid value under the old
        // convention too, only `0` is rewritten back to the legacy sentinel).
        conn.execute_unprepared("ALTER TABLE workspace_templates ALTER COLUMN max_run_seconds DROP NOT NULL")
            .await?;
        conn.execute_unprepared("ALTER TABLE workspace_templates ALTER COLUMN max_run_seconds SET DEFAULT NULL")
            .await?;
        conn.execute_unprepared("ALTER TABLE workspace_templates ALTER COLUMN keep_time_seconds DROP NOT NULL")
            .await?;
        conn.execute_unprepared("ALTER TABLE workspace_templates ALTER COLUMN keep_time_seconds SET DEFAULT NULL")
            .await?;

        conn.execute_unprepared("ALTER TABLE groups ALTER COLUMN max_instances DROP NOT NULL")
            .await?;
        conn.execute_unprepared("ALTER TABLE groups ALTER COLUMN max_instances SET DEFAULT 2")
            .await?;
        conn.execute_unprepared("ALTER TABLE groups ALTER COLUMN pool_cpu_cores SET DEFAULT 0")
            .await?;
        conn.execute_unprepared("ALTER TABLE groups ALTER COLUMN pool_memory_mb SET DEFAULT 0")
            .await?;
        conn.execute_unprepared("ALTER TABLE groups ALTER COLUMN pool_gpu_count SET DEFAULT 0")
            .await?;

        conn.execute_unprepared("ALTER TABLE system_settings ALTER COLUMN host_instance_limit SET DEFAULT 0")
            .await?;
        conn.execute_unprepared("ALTER TABLE system_settings ALTER COLUMN host_cpu_cores SET DEFAULT 0")
            .await?;
        conn.execute_unprepared("ALTER TABLE system_settings ALTER COLUMN host_memory_mb SET DEFAULT 0")
            .await?;
        conn.execute_unprepared("ALTER TABLE system_settings ALTER COLUMN host_gpu_count SET DEFAULT 0")
            .await?;

        conn.execute_unprepared("ALTER TABLE users ALTER COLUMN direct_max_instances SET DEFAULT NULL")
            .await?;

        Ok(())
    }
}
