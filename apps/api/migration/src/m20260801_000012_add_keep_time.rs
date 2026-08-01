use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(
            "ALTER TABLE workspace_templates ADD COLUMN keep_time_seconds BIGINT",
        )
        .await?;

        conn.execute_unprepared(
            "ALTER TABLE workspace_templates ADD COLUMN keep_time_action VARCHAR(20) NOT NULL DEFAULT 'pause'",
        )
        .await?;

        conn.execute_unprepared(
            "ALTER TABLE workspace_templates ADD CONSTRAINT workspace_templates_keep_time_action_check CHECK (keep_time_action IN ('remove', 'stop', 'pause'))",
        )
        .await?;

        conn.execute_unprepared(
            "ALTER TABLE workspace_instances ADD COLUMN last_seen_at TIMESTAMPTZ",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(
            "ALTER TABLE workspace_instances DROP COLUMN last_seen_at",
        )
        .await?;

        conn.execute_unprepared(
            "ALTER TABLE workspace_templates DROP CONSTRAINT workspace_templates_keep_time_action_check",
        )
        .await?;

        conn.execute_unprepared(
            "ALTER TABLE workspace_templates DROP COLUMN keep_time_action",
        )
        .await?;

        conn.execute_unprepared(
            "ALTER TABLE workspace_templates DROP COLUMN keep_time_seconds",
        )
        .await?;

        Ok(())
    }
}
