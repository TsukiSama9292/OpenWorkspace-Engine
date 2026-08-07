use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// The template-level container runtime value `docker` was renamed to `runc`
/// (both mean "use Docker's default OCI runtime", but `docker` is no longer
/// accepted by the API). Rewrite any rows that still hold the old value and
/// reset the column default; the original `DEFAULT 'docker'` from migration
/// `m20260723_000009` is history and left untouched.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(
            "UPDATE workspace_templates SET container_runtime = 'runc' \
             WHERE container_runtime = 'docker'",
        )
        .await?;
        conn.execute_unprepared(
            "ALTER TABLE workspace_templates \
             ALTER COLUMN container_runtime SET DEFAULT 'runc'",
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(
            "ALTER TABLE workspace_templates \
             ALTER COLUMN container_runtime SET DEFAULT 'docker'",
        )
        .await?;

        Ok(())
    }
}
