use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

/// Expand → contract (flat-rbac_2 spec ID-1): seed the three system groups
/// (`groups.kind` = `admin`/`manager`/`user`), rename the legacy `Managers`
/// group to `Manager`, move former `is_system_admin` users into the Admin
/// group, backfill the Admin group onto every existing template's whitelist,
/// and only then drop the per-user whitelist (`user_templates`) and the admin
/// boolean (`users.is_system_admin`). The two seed groups, the rename, and the
/// backfill all happen *before* the drops so nothing is lost on upgrade.
///
/// Name collisions with pre-existing custom groups are legal (spec Decision 9
/// keeps `Admin`/`Manager`/`User` usable as custom-group names), so the seeding
/// first renames any custom group holding a canonical system name out of the
/// way (names are cosmetic; identity is by `kind`), then inserts idempotently
/// keyed on `kind`.
#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        // Expand: system-group identity and a nullable max_instances so the
        // Admin group can mean "unlimited" (NULL), matching the effective
        // context's 0/NULL-is-unlimited rule.
        conn.execute_unprepared("ALTER TABLE groups ADD COLUMN kind VARCHAR(32)")
            .await?;
        conn.execute_unprepared("ALTER TABLE groups ALTER COLUMN max_instances DROP NOT NULL")
            .await?;

        // Free the always-seeded canonical names from custom groups (the
        // `(custom <id>)` suffix keeps the rename collision-free even if two
        // groups share a name — impossible today, but free of cost to guard).
        conn.execute_unprepared(
            r#"
            UPDATE groups
            SET name = name || ' (custom ' || SUBSTRING(id::text, 1, 8) || ')'
            WHERE kind IS NULL AND name IN ('Admin', 'User')
            "#,
        )
        .await?;

        // The 'Manager' name is only needed if a legacy Managers group exists
        // to promote, so free it only in that case (a custom 'Manager' group
        // otherwise keeps its name per spec Decision 9).
        conn.execute_unprepared(
            r#"
            UPDATE groups
            SET name = name || ' (custom ' || SUBSTRING(id::text, 1, 8) || ')'
            WHERE kind IS NULL AND name = 'Manager'
              AND EXISTS (SELECT 1 FROM groups WHERE name = 'Managers')
            "#,
        )
        .await?;

        // Rename the legacy Managers group to Manager (kind='manager'),
        // preserving its members, and cap it at the spec's default of 2.
        // Idempotent: skipped once a manager-kind group already exists.
        conn.execute_unprepared(
            r#"
            UPDATE groups
            SET name = 'Manager', kind = 'manager', max_instances = 2
            WHERE name = 'Managers'
              AND NOT EXISTS (SELECT 1 FROM groups WHERE kind = 'manager')
            "#,
        )
        .await?;

        // Seed the Admin group (all five flags fixed TRUE, unlimited ceiling).
        conn.execute_unprepared(
            r#"
            INSERT INTO groups (name, kind, can_create_template, can_manage_users,
                                can_manage_group_instances, can_manage_docker, can_manage_registry,
                                max_instances)
            SELECT 'Admin', 'admin', TRUE, TRUE, TRUE, TRUE, TRUE, NULL
            WHERE NOT EXISTS (SELECT 1 FROM groups WHERE kind = 'admin')
            "#,
        )
        .await?;

        // Seed the User group (all five flags FALSE, cap 1).
        conn.execute_unprepared(
            r#"
            INSERT INTO groups (name, kind, can_create_template, can_manage_users,
                                can_manage_group_instances, can_manage_docker, can_manage_registry,
                                max_instances)
            SELECT 'User', 'user', FALSE, FALSE, FALSE, FALSE, FALSE, 1
            WHERE NOT EXISTS (SELECT 1 FROM groups WHERE kind = 'user')
            "#,
        )
        .await?;

        // Move every is_system_admin user into the Admin group, removing them
        // from Manager if present (Admin and Manager are mutually exclusive).
        conn.execute_unprepared(
            r#"
            INSERT INTO user_groups (user_id, group_id)
            SELECT u.id, g.id
            FROM users u
            JOIN groups g ON g.kind = 'admin'
            WHERE u.is_system_admin = TRUE
            ON CONFLICT DO NOTHING
            "#,
        )
        .await?;
        conn.execute_unprepared(
            r#"
            DELETE FROM user_groups ug
            USING groups g, users u
            WHERE ug.group_id = g.id AND ug.user_id = u.id
              AND g.kind = 'manager' AND u.is_system_admin = TRUE
            "#,
        )
        .await?;

        // Backfill the Admin group onto every existing template so removing the
        // admin bypass does not cut off existing admin access (spec ID-1).
        conn.execute_unprepared(
            r#"
            INSERT INTO group_templates (group_id, template_id)
            SELECT g.id, t.id
            FROM groups g
            CROSS JOIN workspace_templates t
            WHERE g.kind = 'admin'
            ON CONFLICT DO NOTHING
            "#,
        )
        .await?;

        // Contract: drop the per-user template whitelist and the admin boolean.
        conn.execute_unprepared("DROP TABLE user_templates").await?;
        conn.execute_unprepared("ALTER TABLE users DROP COLUMN is_system_admin")
            .await?;

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        // Restore the admin boolean from Admin-group membership.
        conn.execute_unprepared(
            "ALTER TABLE users ADD COLUMN is_system_admin BOOLEAN NOT NULL DEFAULT FALSE",
        )
        .await?;
        conn.execute_unprepared(
            r#"
            UPDATE users SET is_system_admin = TRUE
            WHERE id IN (
                SELECT ug.user_id FROM user_groups ug
                JOIN groups g ON g.id = ug.group_id WHERE g.kind = 'admin'
            )
            "#,
        )
        .await?;

        // Restore the per-user whitelist table (empty; personal rows were
        // dropped by `up`, nothing to reconstruct).
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

        // Undo the Admin backfill and the admin memberships.
        conn.execute_unprepared(
            "DELETE FROM group_templates WHERE group_id IN (SELECT id FROM groups WHERE kind = 'admin')",
        )
        .await?;
        conn.execute_unprepared(
            "DELETE FROM user_groups WHERE group_id IN (SELECT id FROM groups WHERE kind = 'admin')",
        )
        .await?;

        // Rename Manager back to Managers (restoring its legacy ceiling),
        // drop the Admin/User system groups, and re-assert the legacy NOT NULL.
        conn.execute_unprepared(
            "UPDATE groups SET name = 'Managers', kind = NULL, max_instances = 5 WHERE kind = 'manager'",
        )
        .await?;
        conn.execute_unprepared("DELETE FROM groups WHERE kind IN ('admin', 'user')")
            .await?;
        conn.execute_unprepared("UPDATE groups SET max_instances = 2 WHERE max_instances IS NULL")
            .await?;
        conn.execute_unprepared("ALTER TABLE groups ALTER COLUMN max_instances SET NOT NULL")
            .await?;
        conn.execute_unprepared("ALTER TABLE groups DROP COLUMN kind").await?;

        Ok(())
    }
}
