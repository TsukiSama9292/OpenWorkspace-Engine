use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(
            "ALTER TABLE workspace_templates ADD COLUMN remote_type VARCHAR(32) NOT NULL DEFAULT 'kasmvnc'",
        )
        .await?;

        conn.execute_unprepared(
            "ALTER TABLE workspace_instances RENAME COLUMN vnc_token TO access_token",
        )
        .await?;

        conn.execute_unprepared(
            "ALTER TABLE workspace_instances RENAME COLUMN vnc_password TO access_password",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(
            "ALTER TABLE workspace_instances RENAME COLUMN access_password TO vnc_password",
        )
        .await?;

        conn.execute_unprepared(
            "ALTER TABLE workspace_instances RENAME COLUMN access_token TO vnc_token",
        )
        .await?;

        conn.execute_unprepared(
            "ALTER TABLE workspace_templates DROP COLUMN remote_type",
        )
        .await?;

        Ok(())
    }
}
