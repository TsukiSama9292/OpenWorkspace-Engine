use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(
            "ALTER TABLE workspace_instances ADD COLUMN host_port INTEGER",
        )
        .await?;

        // The database is the concurrency arbiter for the host port pool: at
        // most one instance may own a given port. Postgres UNIQUE treats NULLs
        // as distinct, so pre-existing (legacy) rows with no port stay valid.
        conn.execute_unprepared(
            "CREATE UNIQUE INDEX idx_workspace_instances_host_port ON workspace_instances (host_port)",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(
            "DROP INDEX IF EXISTS idx_workspace_instances_host_port",
        )
        .await?;

        conn.execute_unprepared(
            "ALTER TABLE workspace_instances DROP COLUMN host_port",
        )
        .await?;

        Ok(())
    }
}
