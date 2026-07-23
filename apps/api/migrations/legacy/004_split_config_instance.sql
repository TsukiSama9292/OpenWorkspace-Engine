-- Migration 004: Split workspaces into workspace_configs + workspace_instances

-- 1. Create workspace_configs table (template/reusable config)
CREATE TABLE workspace_configs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(255) NOT NULL,
    description TEXT,
    owner_id UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    image VARCHAR(512) NOT NULL DEFAULT 'kasmweb/desktop:1.19.0-rolling-daily',
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
);

-- 2. Create workspace_instances table (running containers)
CREATE TABLE workspace_instances (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    config_id UUID NOT NULL REFERENCES workspace_configs(id) ON DELETE CASCADE,
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
);

-- 3. Stash unique configs in a temp table (CTEs are statement-scoped)
CREATE TEMPORARY TABLE _migration_configs AS
SELECT DISTINCT ON (w.name, w.image)
    gen_random_uuid() AS config_id,
    w.name,
    NULL::text AS description,
    w.owner_id,
    COALESCE(w.image, 'kasmweb/desktop:1.19.0-rolling-daily') AS image,
    COALESCE(w.cores, 2) AS cores,
    COALESCE(w.memory, 4294967296) AS memory,
    COALESCE(w.gpu_count, 0) AS gpu_count,
    NULL::varchar AS docker_registry,
    '{}'::jsonb AS run_config,
    '{}'::jsonb AS exec_config,
    '{}'::jsonb AS volume_mappings,
    w.volume_host_path AS persistent_storage_path
FROM workspaces w
ORDER BY w.name, w.image, w.created_at;

-- 4. Insert configs from temp table
INSERT INTO workspace_configs (id, name, description, owner_id, image, cores, memory, gpu_count, docker_registry, run_config, exec_config, volume_mappings, persistent_storage_path)
SELECT config_id, name, description, owner_id, image, cores, memory, gpu_count, docker_registry, run_config, exec_config, volume_mappings, persistent_storage_path
FROM _migration_configs;

-- 5. Insert instances linked to migrated configs
INSERT INTO workspace_instances (id, config_id, name, instance_number, owner_id, container_id, status, vnc_token, mount_persistent, resolved_volume_host_path, created_at)
SELECT
    w.id,
    (SELECT mc.config_id FROM _migration_configs mc WHERE mc.name = w.name AND mc.image = COALESCE(w.image, 'kasmweb/desktop:1.19.0-rolling-daily') LIMIT 1) AS config_id,
    w.name,
    w.instance_number,
    w.owner_id,
    w.container_id,
    COALESCE(w.status, 'stopped'),
    w.vnc_token,
    COALESCE(w.persistent_storage, false),
    w.volume_host_path,
    w.created_at
FROM workspaces w;

-- 6. Cleanup
DROP TABLE IF EXISTS workspaces;
DROP TABLE IF EXISTS _migration_configs;

-- 7. Add indexes for common queries
CREATE INDEX idx_workspace_instances_owner ON workspace_instances(owner_id);
CREATE INDEX idx_workspace_instances_config ON workspace_instances(config_id);
CREATE INDEX idx_workspace_instances_status ON workspace_instances(status);
CREATE INDEX idx_workspace_instances_vnc_token ON workspace_instances(vnc_token);
CREATE INDEX idx_workspace_configs_owner ON workspace_configs(owner_id);
