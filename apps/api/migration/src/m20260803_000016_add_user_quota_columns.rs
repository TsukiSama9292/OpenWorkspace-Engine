use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        // NULL means "inherit the role default" — the per-user value is a
        // personal override, never an absolute (spec Decision 6).
        conn.execute_unprepared("ALTER TABLE users ADD COLUMN instance_limit INTEGER")
            .await?;

        conn.execute_unprepared("ALTER TABLE users ADD COLUMN max_cpu_cores INTEGER")
            .await?;

        conn.execute_unprepared("ALTER TABLE users ADD COLUMN max_ram_bytes BIGINT")
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared("ALTER TABLE users DROP COLUMN max_ram_bytes")
            .await?;

        conn.execute_unprepared("ALTER TABLE users DROP COLUMN max_cpu_cores")
            .await?;

        conn.execute_unprepared("ALTER TABLE users DROP COLUMN instance_limit")
            .await?;

        Ok(())
    }
}
