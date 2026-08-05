use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(
            "ALTER TABLE users ADD COLUMN is_system_admin BOOLEAN NOT NULL DEFAULT FALSE",
        )
        .await?;

        conn.execute_unprepared(
            "ALTER TABLE users ADD COLUMN direct_max_instances INTEGER",
        )
        .await?;

        conn.execute_unprepared(
            r#"
            CREATE TABLE groups (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                name VARCHAR(255) NOT NULL UNIQUE,
                description TEXT,
                can_create_template BOOLEAN NOT NULL DEFAULT FALSE,
                can_manage_users BOOLEAN NOT NULL DEFAULT FALSE,
                can_manage_group_instances BOOLEAN NOT NULL DEFAULT FALSE,
                can_manage_docker BOOLEAN NOT NULL DEFAULT FALSE,
                can_manage_registry BOOLEAN NOT NULL DEFAULT FALSE,
                max_instances INTEGER NOT NULL DEFAULT 2,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#,
        )
        .await?;

        conn.execute_unprepared(
            r#"
            CREATE TABLE user_groups (
                user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                group_id UUID NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
                PRIMARY KEY (user_id, group_id)
            )
            "#,
        )
        .await?;

        conn.execute_unprepared(
            r#"
            CREATE TABLE group_templates (
                group_id UUID NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
                template_id UUID NOT NULL REFERENCES workspace_templates(id) ON DELETE CASCADE,
                PRIMARY KEY (group_id, template_id)
            )
            "#,
        )
        .await?;

        conn.execute_unprepared(
            r#"
            CREATE TABLE user_templates (
                user_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                template_id UUID NOT NULL REFERENCES workspace_templates(id) ON DELETE CASCADE,
                PRIMARY KEY (user_id, template_id)
            )
            "#,
        )
        .await?;

        conn.execute_unprepared(
            r#"
            CREATE TABLE persistent_volumes (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                owner_id UUID REFERENCES users(id) ON DELETE SET NULL,
                host_path VARCHAR(512) NOT NULL,
                status VARCHAR(32) NOT NULL DEFAULT 'active',
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#,
        )
        .await?;

        // Seed the Managers group with all five flags enabled and backfill the
        // legacy role tree into the flat model: admins become system admins,
        // managers join the Managers group, and any personal instance_limit is
        // copied into direct_max_instances. The legacy role and quota columns
        // stay until a later migration drops them.
        conn.execute_unprepared(
            r#"
            INSERT INTO groups (name, description, can_create_template, can_manage_users,
                                can_manage_group_instances, can_manage_docker, can_manage_registry,
                                max_instances)
            VALUES ('Managers', NULL, TRUE, TRUE, TRUE, TRUE, TRUE, 5)
            "#,
        )
        .await?;

        conn.execute_unprepared(
            "UPDATE users SET is_system_admin = TRUE WHERE role = 'admin'",
        )
        .await?;

        conn.execute_unprepared(
            "UPDATE users SET direct_max_instances = instance_limit WHERE instance_limit IS NOT NULL",
        )
        .await?;

        conn.execute_unprepared(
            r#"
            INSERT INTO user_groups (user_id, group_id)
            SELECT u.id, g.id
            FROM users u
            JOIN groups g ON g.name = 'Managers'
            WHERE u.role = 'manager'
            "#,
        )
        .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared("DROP TABLE IF EXISTS persistent_volumes")
            .await?;
        conn.execute_unprepared("DROP TABLE IF EXISTS user_templates").await?;
        conn.execute_unprepared("DROP TABLE IF EXISTS group_templates").await?;
        conn.execute_unprepared("DROP TABLE IF EXISTS user_groups").await?;
        conn.execute_unprepared("DROP TABLE IF EXISTS groups").await?;
        conn.execute_unprepared("ALTER TABLE users DROP COLUMN IF EXISTS direct_max_instances")
            .await?;
        conn.execute_unprepared("ALTER TABLE users DROP COLUMN IF EXISTS is_system_admin")
            .await?;

        Ok(())
    }
}
