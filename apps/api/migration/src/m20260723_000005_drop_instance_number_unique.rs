use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        // instance_number UNIQUE constraint is global, but it should be per-config.
        // Drop the UNIQUE constraint — SERIAL still provides auto-increment.
        conn.execute_unprepared(
            r#"
            ALTER TABLE workspace_instances
            DROP CONSTRAINT IF EXISTS workspace_instances_instance_number_key
            "#,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(
            r#"
            DO $$ BEGIN
                ALTER TABLE workspace_instances
                ADD CONSTRAINT workspace_instances_instance_number_key UNIQUE (instance_number);
            EXCEPTION WHEN duplicate_object THEN NULL;
            END $$;
            "#,
        )
        .await?;

        Ok(())
    }
}
