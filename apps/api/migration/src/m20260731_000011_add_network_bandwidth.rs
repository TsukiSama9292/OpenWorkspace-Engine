use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(
            "ALTER TABLE workspace_templates ADD COLUMN network_bandwidth_up_mbps INTEGER NOT NULL DEFAULT 0",
        )
        .await?;

        conn.execute_unprepared(
            "ALTER TABLE workspace_templates ADD COLUMN network_bandwidth_down_mbps INTEGER NOT NULL DEFAULT 0",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(
            "ALTER TABLE workspace_templates DROP COLUMN network_bandwidth_down_mbps",
        )
        .await?;

        conn.execute_unprepared(
            "ALTER TABLE workspace_templates DROP COLUMN network_bandwidth_up_mbps",
        )
        .await?;

        Ok(())
    }
}
