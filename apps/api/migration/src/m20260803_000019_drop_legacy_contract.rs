use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared("ALTER TABLE users DROP COLUMN role").await?;

        conn.execute_unprepared("ALTER TABLE users DROP COLUMN instance_limit")
            .await?;
        conn.execute_unprepared("ALTER TABLE users DROP COLUMN max_cpu_cores")
            .await?;
        conn.execute_unprepared("ALTER TABLE users DROP COLUMN max_ram_bytes")
            .await?;

        conn.execute_unprepared(
            "ALTER TABLE workspace_templates DROP COLUMN allocation_mode",
        )
        .await?;

        conn.execute_unprepared("ALTER TABLE system_settings DROP COLUMN max_cpu_cores")
            .await?;
        conn.execute_unprepared("ALTER TABLE system_settings DROP COLUMN max_ram_bytes")
            .await?;
        conn.execute_unprepared("ALTER TABLE system_settings DROP COLUMN shared_max_cpu")
            .await?;
        conn.execute_unprepared("ALTER TABLE system_settings DROP COLUMN shared_max_ram")
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        // Restore the legacy columns with their original types/defaults so the
        // full migration chain can be rolled back. The system_settings singleton
        // row still exists at this point, so the restored quota columns carry the
        // original seed defaults.
        conn.execute_unprepared(
            "ALTER TABLE users ADD COLUMN role VARCHAR NOT NULL DEFAULT 'user'",
        )
        .await?;

        conn.execute_unprepared("ALTER TABLE users ADD COLUMN instance_limit INTEGER")
            .await?;
        conn.execute_unprepared("ALTER TABLE users ADD COLUMN max_cpu_cores INTEGER")
            .await?;
        conn.execute_unprepared("ALTER TABLE users ADD COLUMN max_ram_bytes BIGINT")
            .await?;

        conn.execute_unprepared(
            "ALTER TABLE workspace_templates ADD COLUMN allocation_mode VARCHAR(20) NOT NULL DEFAULT 'shared'",
        )
        .await?;

        conn.execute_unprepared(
            "ALTER TABLE system_settings ADD COLUMN max_cpu_cores INTEGER NOT NULL DEFAULT 8",
        )
        .await?;
        conn.execute_unprepared(
            "ALTER TABLE system_settings ADD COLUMN max_ram_bytes BIGINT NOT NULL DEFAULT 17179869184",
        )
        .await?;
        conn.execute_unprepared(
            "ALTER TABLE system_settings ADD COLUMN shared_max_cpu INTEGER NOT NULL DEFAULT 0",
        )
        .await?;
        conn.execute_unprepared(
            "ALTER TABLE system_settings ADD COLUMN shared_max_ram BIGINT NOT NULL DEFAULT 0",
        )
        .await?;

        Ok(())
    }
}
