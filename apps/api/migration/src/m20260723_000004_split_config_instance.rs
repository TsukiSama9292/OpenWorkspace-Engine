use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        conn.execute_unprepared(
            r#"
            CREATE TABLE workspace_templates (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                name VARCHAR(255) NOT NULL,
                description TEXT,
                owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                image VARCHAR(512) NOT NULL DEFAULT 'tsukisama9292/ow-kasmvnc-ubuntu:jammy',
                cores INTEGER NOT NULL DEFAULT 2,
                memory BIGINT NOT NULL DEFAULT 4294967296,
                gpu_count INTEGER NOT NULL DEFAULT 0,
                docker_registry VARCHAR(2048),
                run_config JSONB NOT NULL DEFAULT '{}',
                exec_config JSONB NOT NULL DEFAULT '{}',
                volume_mappings JSONB NOT NULL DEFAULT '{}',
                persistent_storage_path VARCHAR(1024),
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#,
        )
        .await?;

        conn.execute_unprepared(
            r#"
            CREATE TABLE workspace_instances (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                template_id UUID NOT NULL REFERENCES workspace_templates(id) ON DELETE CASCADE,
                name VARCHAR(255) NOT NULL,
                instance_number SERIAL UNIQUE,
                owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                container_id VARCHAR(255),
                status VARCHAR(50) NOT NULL DEFAULT 'stopped',
                vnc_token VARCHAR(64) UNIQUE NOT NULL DEFAULT gen_random_uuid()::text,
                mount_persistent BOOLEAN NOT NULL DEFAULT false,
                resolved_volume_host_path VARCHAR(1024),
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#,
        )
        .await?;

        conn.execute_unprepared(
            r#"
            CREATE TEMPORARY TABLE _migration_templates AS
            SELECT DISTINCT ON (w.name, w.image)
                gen_random_uuid() AS template_id,
                w.name,
                NULL::text AS description,
                w.owner_id,
                COALESCE(w.image, 'tsukisama9292/ow-kasmvnc-ubuntu:jammy') AS image,
                COALESCE(w.cores, 2) AS cores,
                COALESCE(w.memory, 4294967296) AS memory,
                COALESCE(w.gpu_count, 0) AS gpu_count,
                NULL::varchar AS docker_registry,
                '{}'::jsonb AS run_config,
                '{}'::jsonb AS exec_config,
                '{}'::jsonb AS volume_mappings,
                w.volume_host_path AS persistent_storage_path
            FROM workspaces w
            ORDER BY w.name, w.image, w.created_at
            "#,
        )
        .await?;

        conn.execute_unprepared(
            r#"
            INSERT INTO workspace_templates (id, name, description, owner_id, image, cores, memory, gpu_count, docker_registry, run_config, exec_config, volume_mappings, persistent_storage_path)
            SELECT template_id, name, description, owner_id, image, cores, memory, gpu_count, docker_registry, run_config, exec_config, volume_mappings, persistent_storage_path
            FROM _migration_templates
            "#,
        )
        .await?;

        conn.execute_unprepared(
            r#"
            INSERT INTO workspace_instances (id, template_id, name, instance_number, owner_id, container_id, status, vnc_token, mount_persistent, resolved_volume_host_path, created_at)
            SELECT
                w.id,
                (SELECT mc.template_id FROM _migration_templates mc WHERE mc.name = w.name AND mc.image = COALESCE(w.image, 'tsukisama9292/ow-kasmvnc-ubuntu:jammy') LIMIT 1) AS template_id,
                w.name,
                w.instance_number,
                w.owner_id,
                w.container_id,
                COALESCE(w.status, 'stopped'),
                w.vnc_token,
                COALESCE(w.persistent_storage, false),
                w.volume_host_path,
                w.created_at
            FROM workspaces w
            "#,
        )
        .await?;

        conn.execute_unprepared("DROP TABLE IF EXISTS workspaces").await?;
        conn.execute_unprepared("DROP TABLE IF EXISTS _migration_templates").await?;

        for idx in &[
            ("idx_workspace_instances_owner", "workspace_instances", "owner_id"),
            ("idx_workspace_instances_template", "workspace_instances", "template_id"),
            ("idx_workspace_instances_status", "workspace_instances", "status"),
            ("idx_workspace_instances_vnc_token", "workspace_instances", "vnc_token"),
            ("idx_workspace_templates_owner", "workspace_templates", "owner_id"),
        ] {
            manager
                .create_index(
                    Index::create()
                        .name(idx.0)
                        .table(idx.1)
                        .col(idx.2)
                        .to_owned(),
                )
                .await?;
        }

        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let conn = manager.get_connection();

        for idx in &[
            "idx_workspace_templates_owner",
            "idx_workspace_instances_vnc_token",
            "idx_workspace_instances_status",
            "idx_workspace_instances_template",
            "idx_workspace_instances_owner",
        ] {
            manager
                .drop_index(Index::drop().name(*idx).to_owned())
                .await?;
        }

        conn.execute_unprepared(
            r#"
            CREATE TABLE workspaces (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                name VARCHAR(255) NOT NULL,
                instance_number SERIAL UNIQUE,
                container_id VARCHAR(255),
                status VARCHAR(50) NOT NULL DEFAULT 'stopped',
                owner_id UUID NOT NULL REFERENCES users(id),
                vnc_token VARCHAR(64) UNIQUE,
                image VARCHAR(512) DEFAULT 'tsukisama9292/ow-kasmvnc-ubuntu:jammy',
                cores INTEGER DEFAULT 2,
                memory BIGINT DEFAULT 4294967296,
                gpu_count INTEGER DEFAULT 0,
                persistent_storage BOOLEAN DEFAULT true,
                volume_host_path VARCHAR(1024),
                volume_container_path VARCHAR(1024) DEFAULT '/home/kasm_user',
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
            "#,
        )
        .await?;

        conn.execute_unprepared(
            r#"
            INSERT INTO workspaces (id, name, instance_number, container_id, status, owner_id, vnc_token, image, cores, memory, gpu_count, persistent_storage, volume_host_path, created_at)
            SELECT
                wi.id,
                wi.name,
                wi.instance_number,
                wi.container_id,
                wi.status,
                wi.owner_id,
                wi.vnc_token,
                wc.image,
                wc.cores,
                wc.memory,
                wc.gpu_count,
                wi.mount_persistent,
                wi.resolved_volume_host_path,
                wi.created_at
            FROM workspace_instances wi
            JOIN workspace_templates wc ON wi.template_id = wc.id
            "#,
        )
        .await?;

        conn.execute_unprepared("DROP TABLE IF EXISTS workspace_instances").await?;
        conn.execute_unprepared("DROP TABLE IF EXISTS workspace_templates").await?;

        Ok(())
    }
}
