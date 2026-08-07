use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Add the Monitor-dashboard group permission flag `can_view_monitoring`
/// (monitor-dashboard spec Decision 8). New groups default to off; the Admin
/// and Manager system groups are backfilled on (their members should see the
/// Monitor tab), the User system group and all custom groups stay off. Values
/// are validated in Rust; the column is plain boolean, no DB CHECK.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE groups \
                 ADD COLUMN can_view_monitoring BOOLEAN NOT NULL DEFAULT FALSE",
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE groups SET can_view_monitoring = TRUE \
                 WHERE kind IN ('admin', 'manager')",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE groups DROP COLUMN can_view_monitoring")
            .await?;
        Ok(())
    }
}
