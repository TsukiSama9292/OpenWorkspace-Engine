use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(
            "CREATE TABLE system_settings (
                id INTEGER PRIMARY KEY,
                max_cpu_cores INTEGER NOT NULL,
                max_ram_bytes BIGINT NOT NULL,
                host_instance_limit INTEGER NOT NULL,
                shared_max_cpu INTEGER NOT NULL,
                shared_max_ram BIGINT NOT NULL
            )",
        )
        .await?;

        // Seed the singleton with conservative defaults (8 cores / 16 GiB,
        // unlimited instance count, shared fuse off) so the global lock target
        // always exists; startup refresh-provisions the capacity values.
        conn.execute_unprepared(
            "INSERT INTO system_settings (id, max_cpu_cores, max_ram_bytes, host_instance_limit, shared_max_cpu, shared_max_ram)
             VALUES (1, 8, 17179869184, 0, 0, 0)",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared("DROP TABLE system_settings").await?;

        Ok(())
    }
}
