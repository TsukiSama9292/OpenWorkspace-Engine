# Workspace Management Platform — v2 Spec (Config + Instance Split)

## Problem Statement

The v1 implementation conflated "workspace configuration" and "running instance" into a single `workspaces` table. A workspace config (image, CPU, RAM, GPU, persistent storage path, Docker settings) should be a reusable template that users create once and launch multiple container instances from. The current design also lacks:

- A proper persistent storage path template (currently just a boolean toggle)
- Docker resource limits actually being applied (cores/memory stored but never passed to Docker)
- Full Docker configuration passthrough (environment variables, DNS, network mode, SHM size, volume mappings, exec commands)
- The ability to launch multiple instances from one config

The user needs a clean separation between **Workspace Config** (template) and **Workspace Instance** (running container), where one config can spawn many instances.

## Solution

Split the current `workspaces` table into two:

1. **Workspace Config** — a reusable template stored in `workspace_configs`. Users create these via the UI (from scratch or from registry templates). Contains all Docker configuration fields.

2. **Workspace Instance** — a running container spawned from a config, stored in `workspace_instances`. Each instance gets its own VNC token, container ID, status, and resolved volume mounts. Users can launch multiple instances from the same config.

Additionally:
- Pass all Docker configuration fields from the config to bollard when creating containers (resource limits, environment, DNS, SHM, network mode, volumes, exec commands)
- Persistent storage path is a template set at config time (e.g., `/data/persistent/{workspace_name}/{user_id}`), resolved at instance launch time
- When launching an instance, user can choose whether to mount the persistent volume

## User Stories

### Workspace Config CRUD

1. As a user, I want to create a workspace config by filling in a form (name, image, CPU, RAM, etc.), so that I have a reusable template for launching desktops.
2. As a user, I want to give my workspace config a custom name, so that I can identify it easily (e.g., "AI Lab", "Web Dev").
3. As a user, I want to set the Docker image for my config, so that I can choose which desktop environment to use.
4. As a user, I want to set CPU cores and memory limits for my config, so that I can control resource allocation.
5. As a user, I want to optionally set GPU count for my config, so that I can allocate GPU resources when needed.
6. As a user, I want to set a Docker registry URL for my config, so that I can pull images from private registries.
7. As a user, I want to configure run_config (hostname, DNS, SHM size, environment variables, network mode) for my config, so that I can customize the container runtime environment.
8. As a user, I want to configure exec_config (post-start commands) for my config, so that I can run custom setup scripts when a container starts.
9. As a user, I want to configure volume_mappings (host→container path pairs) for my config, so that I can mount additional directories into containers.
10. As a user, I want to set a persistent storage path template for my config (format: `/path/{workspace_name}/{user_id}`), so that user data persists across container restarts.
11. As a user, I want to see all my workspace configs on a dashboard page, so that I can manage my templates.
12. As a user, I want to edit an existing workspace config, so that I can update settings without recreating it.
13. As a user, I want to delete a workspace config, so that I can remove templates I no longer need.
14. As a user, I want to see how many instances are running from each config, so that I can monitor usage.
15. As an admin, I want to see all workspace configs across all users, so that I can manage the system.
16. As a user, I want to see the config details (all fields) on a detail page, so that I can review settings before launching.

### Workspace Instance Lifecycle

17. As a user, I want to launch a new instance from a workspace config, so that I can start a virtual desktop.
18. As a user, I want to choose whether to mount the persistent volume when launching an instance, so that I can decide if I need persistent data.
19. As a user, I want the system to resolve the persistent storage path template (replace `{workspace_name}` and `{user_id}`) when launching, so that each user gets their own storage directory.
20. As a user, I want to see all my running instances on the dashboard, so that I can monitor my active desktops.
21. As a user, I want to pause an instance (Docker pause — RAM preserved in host memory), so that I can temporarily free CPU without losing state.
22. As a user, I want to resume a paused instance, so that I can continue where I left off instantly.
23. As a user, I want to stop an instance, so that I can shut down a desktop I no longer need.
24. As a user, I want to delete an instance, so that I can remove it completely (container + DB record).
25. As a user, I want to connect to a running instance via VNC in my browser, so that I can use the virtual desktop.
26. As a user, I want to see instance details (status, container ID, config it was launched from, resolved volume mounts), so that I can debug issues.
27. As an admin, I want to see all instances across all users, so that I can monitor system usage.
28. As a user, I want instance names to be auto-generated (e.g., config name + number), so that I don't have to name each instance.

### Docker Integration

29. As a system, I want to apply CPU limits (nano_cpus) to containers based on the config's `cores` field, so that resource allocation is enforced.
30. As a system, I want to apply memory limits to containers based on the config's `memory` field, so that resource allocation is enforced.
31. As a system, I want to set environment variables on containers from the config's `run_config.environment`, so that container behavior is configurable.
32. As a system, I want to configure DNS servers on containers from the config's `run_config.dns`, so that network resolution is customizable.
33. As a system, I want to set SHM size on containers from the config's `run_config.shm_size`, so that shared memory is properly allocated.
34. As a system, I want to set network mode on containers from the config's `run_config.network_mode`, so that network connectivity is configurable.
35. As a system, I want to mount volumes from the config's `volume_mappings` into containers, so that host directories are accessible.
36. As a system, I want to mount the persistent storage volume (resolved path) into containers when the user opts in, so that user data persists.
37. As a system, I want to execute post-start commands from the config's `exec_config` after a container starts, so that custom setup runs automatically.

### Registry Integration

38. As a user, I want to browse workspace templates from the synced registry, so that I can quickly create a config from a predefined template.
39. As a user, I want to select a registry template and customize its settings before saving as my own config, so that I don't have to start from scratch.
40. As an admin, I want to sync the registry from a remote URL, so that the available templates are up to date.

### Dashboard & UI

41. As a user, I want the dashboard to show two sections: "Configs" and "Instances", so that I can distinguish templates from running containers.
42. As a user, I want config cards to show: name, image, resource summary (CPU/RAM), instance count, so that I can quickly identify configs.
43. As a user, I want instance cards to show: name (auto-generated), status (running/paused/stopped), config name, owner, so that I can identify running containers.
44. As a user, I want a "New Config" button that opens a form to create a workspace config from scratch.
45. As a user, I want a "Launch Instance" button on a config detail page, so that I can spawn a new instance from that config.
46. As a user, I want a config detail page showing all fields (image, CPU, RAM, GPU, registry, run_config, exec_config, volume_mappings, persistent storage path), so that I can review settings.
47. As a user, I want an instance detail page showing status, container ID, config name, resolved volumes, and lifecycle controls (Pause/Resume/Stop/Delete/Connect), so that I can manage the running container.

### Authentication & Authorization

48. As a user, I want to log in with username/password, so that my actions are authenticated.
49. As a user, I want only admins to delete configs created by other users, so that configs are protected.
50. As a user, I want only admins to see all configs/instances, so that regular users only see their own.
51. As a user, I want to be able to register a new account, so that I can join the system.

### Persistent Storage

52. As a user, I want to set a persistent storage path template at config time, so that the path is consistent across instances.
53. As a user, I want to choose whether to mount persistent storage when launching an instance, so that I can opt out if I don't need persistence.
54. As a system, I want to resolve `{workspace_name}` and `{user_id}` in the path template when launching, so that each user gets a unique directory.
55. As a user, I want my data in the persistent storage directory to survive container restarts and reconnections, so that I don't lose work.

### System Robustness

56. As a system, I want to handle Docker daemon unavailability gracefully, so that the API returns proper error responses.
57. As a system, I want to prevent invalid lifecycle transitions (e.g., pause a stopped container), so that the state machine is enforced.
58. As a system, I want to clean up Traefik routes when instances are stopped or deleted, so that stale routes don't persist.

## Implementation Decisions

### Database Schema

**New table: `workspace_configs`** (replaces the Docker-related columns in `workspaces`)

| Column | Type | Notes |
|--------|------|-------|
| `id` | UUID PK | Auto-generated |
| `name` | VARCHAR(255) NOT NULL | User-defined config name |
| `description` | TEXT | Optional description |
| `owner_id` | UUID FK→users | Creator |
| `image` | VARCHAR(512) NOT NULL | Docker image |
| `cores` | INTEGER NOT NULL DEFAULT 2 | CPU cores |
| `memory` | BIGINT NOT NULL DEFAULT 4294967296 | Memory in bytes |
| `gpu_count` | INTEGER DEFAULT 0 | GPU count |
| `docker_registry` | VARCHAR(2048) | Private registry URL |
| `run_config` | JSONB DEFAULT '{}' | { hostname, dns, shm_size, environment, network_mode } |
| `exec_config` | JSONB DEFAULT '{}' | Post-start commands |
| `volume_mappings` | JSONB DEFAULT '{}' | Host→container path pairs |
| `persistent_storage_path` | VARCHAR(1024) | Template: `/path/{workspace_name}/{user_id}` |
| `created_at` | TIMESTAMPTZ | Auto |
| `updated_at` | TIMESTAMPTZ | Auto |

**New table: `workspace_instances`** (replaces `workspaces`)

| Column | Type | Notes |
|--------|------|-------|
| `id` | UUID PK | Auto-generated |
| `config_id` | UUID FK→workspace_configs | Source config |
| `name` | VARCHAR(255) NOT NULL | Auto-generated: `{config_name}-{number}` |
| `instance_number` | SERIAL UNIQUE | Auto-increment |
| `owner_id` | UUID FK→users | Instance creator |
| `container_id` | VARCHAR(255) | Docker container ID (nullable) |
| `status` | VARCHAR(50) DEFAULT 'stopped' | stopped/running/paused/error |
| `vnc_token` | VARCHAR(64) UNIQUE | For Traefik routing |
| `mount_persistent` | BOOLEAN DEFAULT false | Whether to mount persistent volume |
| `resolved_volume_host_path` | VARCHAR(1024) | Resolved path (nullable) |
| `created_at` | TIMESTAMPTZ | Auto |
| `updated_at` | TIMESTAMPTZ | Auto |

**Tables to drop**: `workspaces` (replaced by the two new tables), `registry_config`, `registry_cache` (keep as-is).

### API Endpoints

**Workspace Config endpoints** (new):
- `GET /api/configs` — list configs (admin: all, user: own)
- `POST /api/configs` — create config
- `GET /api/configs/{id}` — get config detail
- `PUT /api/configs/{id}` — update config
- `DELETE /api/configs/{id}` — delete config (must have no running instances)

**Workspace Instance endpoints** (modified):
- `GET /api/instances` — list instances (admin: all, user: own)
- `POST /api/instances` — launch instance from config (body: `{ config_id, mount_persistent? }`)
- `GET /api/instances/{id}` — get instance detail
- `DELETE /api/instances/{id}` — delete instance
- `POST /api/instances/{id}/start` — start instance
- `POST /api/instances/{id}/stop` — stop instance
- `POST /api/instances/{id}/pause` — pause instance
- `POST /api/instances/{id}/unpause` — unpause instance

**Registry endpoints** (unchanged):
- `GET /api/registry` — get cached registry
- `POST /api/registry/sync` — sync from remote URL
- `GET /api/registry/url` — get registry URL
- `PUT /api/registry/url` — set registry URL

**Auth & User endpoints** (unchanged).

### Docker Client Changes

The `create_kasm_container` method must be refactored to accept a config struct and apply all Docker settings:

- **Resource limits**: `HostConfig.NanoCpus` (cores × 1e9), `HostConfig.Memory` (bytes), `HostConfig.Devices` (GPU)
- **Environment**: `Config.Env` from `run_config.environment`
- **DNS**: `HostConfig.Dns` from `run_config.dns`
- **SHM size**: `HostConfig.ShmSize` from `run_config.shm_size`
- **Network mode**: `HostConfig.NetworkMode` from `run_config.network_mode`
- **Volume mounts**: `HostConfig.Binds` from `volume_mappings` + resolved persistent storage path
- **Post-start exec**: After container starts, run commands from `exec_config` via `docker exec`

A new method `create_container_from_config` will replace `create_kasm_container`, taking the full config and instance parameters.

### Frontend Changes

**Dashboard** (`/`):
- Two tabbed sections: "Configs" and "Instances"
- Config cards: name, image, CPU/RAM summary, instance count
- Instance cards: auto-generated name, status badge, config name, owner
- "New Config" button
- Empty state for each section

**New Config form** (`/configs/new/`):
- Fields: name, description, image, cores, RAM (GB), GPU count, docker_registry
- Collapsible sections for: run_config, exec_config, volume_mappings, persistent storage
- run_config: hostname, DNS (comma-separated), SHM size, environment variables (key-value pairs), network mode
- exec_config: command input
- volume_mappings: dynamic key-value pair inputs (add/remove rows)
- persistent_storage_path: text input with template variable hints

**Config detail page** (`/configs/[id]/`):
- Read-only view of all config fields
- "Launch Instance" button → opens modal to choose mount_persistent
- List of instances launched from this config
- Edit/Delete buttons (admin or owner)

**Instance detail page** (`/instances/[id]/`):
- Status, container ID, config name, resolved volumes
- Lifecycle controls (Start/Pause/Resume/Stop/Delete)
- VNC Connect button

**Route changes**:
- `/configs/` — config list (could also be a tab on dashboard)
- `/configs/new/` — create config
- `/configs/[id]/` — config detail
- `/instances/[id]/` — instance detail
- Remove `/workspaces/` routes

### Migration Strategy

A new migration `004_split_config_instance.sql` will:
1. Create `workspace_configs` table
2. Create `workspace_instances` table
3. Migrate existing `workspaces` data: each row becomes a config + an instance
4. Drop the old `workspaces` table
5. Update `registry_config` and `registry_cache` remain unchanged

### Persistent Storage Path Resolution

Template: `/data/persistent/{workspace_name}/{user_id}`

At instance launch time, if `mount_persistent` is true:
1. Read `persistent_storage_path` from the config
2. Replace `{workspace_name}` with the config name
3. Replace `{user_id}` with the instance owner's UUID
4. Create the directory on the host if it doesn't exist
5. Mount it into the container at a fixed path (e.g., `/home/kasm_user/persistent`)

### Instance Naming

Auto-generated: `{config_name}-{instance_number}` where `instance_number` is a per-config sequential counter. Example: "AI Lab-1", "AI Lab-2".

## Testing Decisions

### Test approach
- **API contract tests**: Integration tests with full Axum server + sqlx::test PostgreSQL. Test all config and instance CRUD endpoints, lifecycle transitions, and error cases.
- **Database tests**: Repository unit tests with sqlx::test. Test config and instance queries, foreign key constraints, cascade behavior.
- **Docker client tests**: Integration tests against real Docker daemon (or mock). Test that `create_container_from_config` passes all fields correctly.
- **Frontend tests**: Deferred (requires live VNC containers for E2E).

### Key test seams
1. **Config CRUD** — create, read, update, delete configs via API
2. **Instance lifecycle** — launch, start, pause, resume, stop, delete via API
3. **Config→Docker passthrough** — verify all config fields appear in Docker container config
4. **Persistent storage resolution** — verify template variables are replaced correctly
5. **Authorization** — verify users can only see/modify their own configs/instances
6. **State machine** — verify invalid transitions return 409

## Out of Scope

- Registry-based config creation (option 1 from grilling — deferred to v3)
- Periodic auto-sync of registry (v2)
- Container logs viewer
- Resource monitoring (CPU/RAM usage graphs)
- GPU passthrough (actual GPU allocation, only field stored)
- Batch operations (start/stop multiple instances)
- Non-VNC workloads (arbitrary container management)
- Frontend unit tests
- HTTPS/TLS configuration
- Persistent storage path enforcement (existence check on host)

## Further Notes

- The current `workspaces` table has data that needs to be migrated. The migration should create configs from existing workspace settings and instances from existing running containers.
- The `vnc_token` and Traefik routing logic remains the same — each instance gets its own token and dynamic route.
- The `exec_config` commands run AFTER the container starts and the VNC service is ready. Timing may need a health check or retry loop.
- The `volume_mappings` field stores additional mounts beyond the persistent storage path. Both are combined when creating the container.
