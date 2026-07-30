use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(
            "ALTER TABLE instances RENAME TO workspaces",
        )
        .await?;

        for col in &[
            ("image", "VARCHAR(512)", "'tsukisama9292/ow-kasmvnc-ubuntu:jammy'"),
            ("cores", "INTEGER", "2"),
            ("memory", "BIGINT", "4294967296"),
            ("gpu_count", "INTEGER", "0"),
            ("persistent_storage", "BOOLEAN", "true"),
            ("volume_host_path", "VARCHAR(1024)", "NULL"),
            ("volume_container_path", "VARCHAR(1024)", "'/home/kasm_user'"),
        ] {
            conn.execute_unprepared(&format!(
                "ALTER TABLE workspaces ADD COLUMN IF NOT EXISTS {} {} DEFAULT {}",
                col.0, col.1, col.2,
            ))
            .await?;
        }

        conn.execute_unprepared(
            r#"
            CREATE TABLE IF NOT EXISTS registry_config (
                id INTEGER PRIMARY KEY DEFAULT 1,
                registry_url VARCHAR(2048) NOT NULL,
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                CONSTRAINT single_row CHECK (id = 1)
            )
            "#,
        )
        .await?;

        conn.execute_unprepared(
            r#"
            CREATE TABLE IF NOT EXISTS registry_cache (
                id INTEGER PRIMARY KEY DEFAULT 1,
                registry_json JSONB NOT NULL,
                synced_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                CONSTRAINT single_row CHECK (id = 1)
            )
            "#,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared("DROP TABLE IF EXISTS registry_cache").await?;
        conn.execute_unprepared("DROP TABLE IF EXISTS registry_config").await?;

        for col in &[
            "volume_container_path",
            "volume_host_path",
            "persistent_storage",
            "gpu_count",
            "memory",
            "cores",
            "image",
        ] {
            conn.execute_unprepared(&format!(
                "ALTER TABLE workspaces DROP COLUMN IF EXISTS {}",
                col,
            ))
            .await?;
        }

        conn.execute_unprepared(
            "ALTER TABLE workspaces RENAME TO instances",
        )
        .await?;

        Ok(())
    }
}
