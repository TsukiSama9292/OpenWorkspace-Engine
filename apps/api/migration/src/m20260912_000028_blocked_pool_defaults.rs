use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Blocked-by-default group pools (resource-quotas v2 spec Decision 1 /
/// Story 16).
///
/// Migration 000026 set the pool columns' schema default to `-1` (unlimited)
/// while backfilling existing rows — which also made every *new* group
/// unlimited by default, contradicting the spec: a newly created group must
/// default to `0` (blocked) on every resource so no instance launches into an
/// ungoverned pool by accident. Existing rows are untouched (they were
/// deliberately backfilled to `-1`); only the column default changes, so this
/// migration alters fresh-row behavior going forward. The API contract
/// (`GroupInput` serde defaults) and the web create form default to `0` to
/// match.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared("ALTER TABLE groups ALTER COLUMN pool_cpu_cores SET DEFAULT 0")
            .await?;
        conn.execute_unprepared("ALTER TABLE groups ALTER COLUMN pool_memory_mb SET DEFAULT 0")
            .await?;
        conn.execute_unprepared("ALTER TABLE groups ALTER COLUMN pool_gpu_count SET DEFAULT 0")
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        // Restore the 000026 defaults.
        conn.execute_unprepared("ALTER TABLE groups ALTER COLUMN pool_cpu_cores SET DEFAULT -1")
            .await?;
        conn.execute_unprepared("ALTER TABLE groups ALTER COLUMN pool_memory_mb SET DEFAULT -1")
            .await?;
        conn.execute_unprepared("ALTER TABLE groups ALTER COLUMN pool_gpu_count SET DEFAULT -1")
            .await?;

        Ok(())
    }
}
