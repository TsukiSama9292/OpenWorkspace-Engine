-- Rename instances table to workspaces
ALTER TABLE instances RENAME TO workspaces;

-- Add workspace configuration columns
ALTER TABLE workspaces ADD COLUMN IF NOT EXISTS image VARCHAR(512) DEFAULT 'kasmweb/desktop:1.19.0-rolling-daily';
ALTER TABLE workspaces ADD COLUMN IF NOT EXISTS cores INTEGER DEFAULT 2;
ALTER TABLE workspaces ADD COLUMN IF NOT EXISTS memory BIGINT DEFAULT 4294967296;
ALTER TABLE workspaces ADD COLUMN IF NOT EXISTS gpu_count INTEGER DEFAULT 0;
ALTER TABLE workspaces ADD COLUMN IF NOT EXISTS persistent_storage BOOLEAN DEFAULT true;
ALTER TABLE workspaces ADD COLUMN IF NOT EXISTS volume_host_path VARCHAR(1024);
ALTER TABLE workspaces ADD COLUMN IF NOT EXISTS volume_container_path VARCHAR(1024) DEFAULT '/home/kasm_user';

-- Registry configuration: stores the remote URL
CREATE TABLE IF NOT EXISTS registry_config (
    id INTEGER PRIMARY KEY DEFAULT 1,
    registry_url VARCHAR(2048) NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT single_row CHECK (id = 1)
);

-- Registry cache: stores the last-synced workspace definitions
CREATE TABLE IF NOT EXISTS registry_cache (
    id INTEGER PRIMARY KEY DEFAULT 1,
    registry_json JSONB NOT NULL,
    synced_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT single_row CHECK (id = 1)
);
