use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(
            "ALTER TABLE workspace_templates ADD COLUMN max_run_seconds BIGINT",
        )
        .await?;

        conn.execute_unprepared(
            "ALTER TABLE workspace_templates ADD COLUMN timeout_action VARCHAR(20) NOT NULL DEFAULT 'remove'",
        )
        .await?;

        conn.execute_unprepared(
            "ALTER TABLE workspace_templates ADD CONSTRAINT workspace_templates_timeout_action_check CHECK (timeout_action IN ('remove', 'stop', 'pause'))",
        )
        .await?;

        conn.execute_unprepared(
            "ALTER TABLE workspace_instances ADD COLUMN started_at TIMESTAMPTZ",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(
            "ALTER TABLE workspace_instances DROP COLUMN started_at",
        )
        .await?;

        conn.execute_unprepared(
            "ALTER TABLE workspace_templates DROP CONSTRAINT workspace_templates_timeout_action_check",
        )
        .await?;

        conn.execute_unprepared(
            "ALTER TABLE workspace_templates DROP COLUMN timeout_action",
        )
        .await?;

        conn.execute_unprepared(
            "ALTER TABLE workspace_templates DROP COLUMN max_run_seconds",
        )
        .await?;

        Ok(())
    }
}
