use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Add the per-template launch visibility (template-visibility spec Decision
/// 1): `public` (everyone may launch), `private` (default — the group whitelist
/// governs), `hidden` (nobody may launch). Existing rows land at `private`, so
/// the upgrade changes no authorization. Values are validated in Rust; the
/// column is plain text, no DB CHECK.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE workspace_templates \
                 ADD COLUMN visibility VARCHAR(16) NOT NULL DEFAULT 'private'",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE workspace_templates DROP COLUMN visibility")
            .await?;
        Ok(())
    }
}
