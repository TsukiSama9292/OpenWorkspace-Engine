use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Stage-5 observability (observability-logs spec Decision 1).
///
/// 1. New `audit_logs` table: one row per recorded administrative / security
///    event. `actor_name` snapshots the actor's name (or `"system"` /
///    `"anonymous"`); `actor_user_id` is NULL when no authenticated user
///    exists (failed logins). `detail` is a redacted JSONB changed-field
///    before/after diff.
/// 2. New `groups.can_view_audit_logs` flag: custom groups default off; the
///    Admin and Manager system groups are backfilled on (mirrors `000022`),
///    the User system group stays off.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE TABLE audit_logs (
                    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                    created_at    TIMESTAMPTZ NOT NULL,
                    actor_user_id UUID NULL,
                    actor_name    TEXT NOT NULL,
                    action        TEXT NOT NULL,
                    target_type   TEXT NULL,
                    target_id     TEXT NULL,
                    target_name   TEXT NULL,
                    outcome       TEXT NOT NULL,
                    client_ip     TEXT NULL,
                    detail        JSONB NULL
                )",
            )
            .await?;
        // Keyset cursor support: ORDER BY created_at DESC, id DESC, cursor
        // `(created_at, id) < ($1, $2)`. Plus a filter index on `action`.
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX idx_audit_logs_created_id \
                 ON audit_logs (created_at DESC, id DESC)",
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "CREATE INDEX idx_audit_logs_action ON audit_logs (action)",
            )
            .await?;

        manager
            .get_connection()
            .execute_unprepared(
                "ALTER TABLE groups \
                 ADD COLUMN can_view_audit_logs BOOLEAN NOT NULL DEFAULT FALSE",
            )
            .await?;
        manager
            .get_connection()
            .execute_unprepared(
                "UPDATE groups SET can_view_audit_logs = TRUE \
                 WHERE kind IN ('admin', 'manager')",
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("ALTER TABLE groups DROP COLUMN can_view_audit_logs")
            .await?;
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE audit_logs")
            .await?;
        Ok(())
    }
}
