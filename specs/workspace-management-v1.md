# Workspace Management Platform — v1 Spec

## Problem Statement

Schools, SMBs, labs, and institutions face rising hardware costs. Each staff member or student typically requires their own desktop machine, but purchasing new hardware for every person is expensive and wasteful. The organization needs a way to let multiple users share a single server's resources through thin clients or old computers, accessing virtual desktops via a web browser — with zero new hardware purchases, low learning cost, and efficient resource utilization.

The current codebase has a partially built system: a Rust API with container lifecycle (create, delete, start, stop), a SvelteKit management UI, KasmVNC container orchestration via bollard, Traefik dynamic routing, and JWT authentication. However, the system uses "instance" terminology, lacks pause/unpause, has no workspace registry, and the UI is a basic prototype. v1 needs to complete the gaps, rename to "workspace" terminology, add a registry-driven workspace catalog, and ship a functional management platform.

## Solution

Complete the workspace management platform v1 by:

1. **Renaming "Instance" → "Workspace"** throughout API, database, and UI.
2. **Adding Docker pause/unpause** to the container lifecycle — instant freeze/resume with RAM preserved in host memory.
3. **Introducing a Workspace Registry** — a remote JSON file (`workspace_registry.json`) that defines available workspace types (Ubuntu Desktop, Firefox, Chromium, etc.) with resource defaults. The API syncs on-demand; the UI presents a catalog-style dashboard.
4. **Redesigning the dashboard** as a workspace catalog — workspace cards (icon, name, status, owner) like an app store. Click a card → detail page with lifecycle controls (Start/Pause/Resume/Stop/Delete) and VNC Connect.
5. **Adding workspace configuration** — each workspace captures: docker image, CPU cores, RAM (bytes), GPU count, persistent volume mapping, with defaults from the registry.

## User Stories

### Workspace Lifecycle

1. As a user, I want to create a new workspace by picking a workspace type from the registry, so that I get a pre-configured virtual desktop without manual Docker setup.
2. As a user, I want to give my workspace a name, so that I can identify it on the dashboard.
3. As a user, I want to override resource defaults (CPU, RAM, GPU) when creating a workspace, so that I can tune performance for my use case.
4. As a user, I want to enable or disable persistent storage when creating a workspace, so that my files survive container restarts.
5. As a user, I want to start a stopped workspace, so that I can resume working.
6. As a user, I want to pause a running workspace, so that I can free CPU while keeping my session state in memory.
7. As a user, I want to resume a paused workspace, so that I can continue exactly where I left off instantly.
8. As a user, I want to stop a running workspace, so that I can free all resources (CPU + RAM).
9. As a user, I want to delete a workspace I no longer need, so that I can clean up resources.
10. As a user, I want to connect to a running workspace via VNC in my browser, so that I can use the virtual desktop.
11. As a user, I want the system to prevent invalid state transitions (e.g., pause a workspace that is already paused), so that I get clear error feedback.

### Workspace Registry

12. As an admin, I want to configure a remote URL for the workspace registry, so that the system can fetch available workspace types.
13. As an admin, I want to manually sync the registry from the remote URL, so that I can control when new workspace types appear.
14. As an admin, I want the API to cache the last-synced registry, so that users can still create workspaces even if the remote registry is temporarily unreachable.
15. As a user, I want to see all available workspace types with their icons, names, and descriptions, so that I can choose the right one.
16. As a user, I want to see resource defaults (CPU, RAM) for each workspace type, so that I know what I'm getting.

### Dashboard and Navigation

17. As a user, I want a dashboard that shows all my workspaces as cards, so that I can see at a glance what I have.
18. As a user, I want each workspace card to show: icon (from registry), name, status (running/stopped/paused), and owner, so that I can quickly identify workspaces.
19. As a user, I want to click a workspace card to see its detail page, so that I can manage it.
20. As an admin, I want to see all workspaces across all users on the dashboard, so that I can manage the system.
21. As a user, I want to see only my own workspaces on the dashboard, so that I don't get overwhelmed by others' workspaces.
22. As a user, I want an empty state message when I have no workspaces, so that I know what to do next.

### Workspace Detail Page

23. As a user, I want to see my workspace's status, image, resource allocation, owner, and persistent storage path on the detail page, so that I have full visibility.
24. As a user, I want lifecycle control buttons (Start/Pause/Resume/Stop/Delete) on the detail page, so that I can manage my workspace.
25. As a user, I want a "Connect" button that opens VNC in a new tab, so that I can start using my desktop immediately.
26. As a user, I want to see the current status of my workspace update in real-time after performing an action, so that I know the action succeeded.

### Authentication and Authorization

27. As a user, I want to log in with my username and password, so that my workspaces are protected.
28. As a user, I want my login session to persist across browser tabs, so that I don't have to log in repeatedly.
29. As an admin, I want to create and delete users, so that I can manage team access.
30. As an admin, I want to see all workspaces across all users, so that I can monitor system usage.
31. As a non-admin user, I want to see only my own workspaces, so that I have privacy.
32. As a user, I want to be redirected to the login page if my session expires, so that I'm not shown errors.

### Persistent Storage

33. As a user, I want my workspace's home directory to persist across stop/start cycles, so that my files and settings are preserved.
34. As a user, I want the persistent storage path to follow the pattern `/mnt/ow/{workspace_name}/{user_id}`, so that data is organized and isolated.
35. As a user, I want to see the persistent storage path on the workspace detail page, so that I know where my data lives.

### System Robustness

36. As a user, I want the system to handle Docker daemon unavailability gracefully, so that I get a clear error message instead of a crash.
37. As a user, I want the system to handle registry fetch failures gracefully, so that I can still use existing workspaces.
38. As a user, I want workspace status to be consistent between the database and actual Docker container state, so that I don't see stale information.

## Implementation Decisions

### Terminology Rename

All references to "instance" in the API, database, and UI are renamed to "workspace". This includes:

- API paths: `/api/instances` → `/api/workspaces`
- Database table: `instances` → `workspaces` (via migration)
- Rust structs: `InstanceRepository` → `WorkspaceRepository`, `CreateInstanceRequest` → `CreateWorkspaceRequest`
- Frontend routes: `/instances/` → `/workspaces/`
- JSON response keys: `"instance"` → `"workspace"`, `"instances"` → `"workspaces"`

### Database Schema Changes

A new migration (`003_rename_instances_to_workspaces.sql`) performs:

```sql
ALTER TABLE instances RENAME TO workspaces;
-- Update any sequence names if needed
-- The existing columns remain: id, name, instance_number, container_id, status, owner_id, created_at, updated_at, vnc_token
```

No new columns are added in v1. Workspace configuration (image, CPU, RAM, GPU, volumes) is stored on the workspace record but the exact column additions are:

- `image VARCHAR(512)` — docker image from registry or user override
- `cores INTEGER DEFAULT 2` — CPU core allocation
- `memory BIGINT DEFAULT 4294967296` — RAM in bytes (default 4GB)
- `gpu_count INTEGER DEFAULT 0` — GPU count
- `persistent_storage BOOLEAN DEFAULT true` — whether to mount persistent volume
- `volume_host_path VARCHAR(1024)` — computed host path `/mnt/ow/{name}/{user_id}`
- `volume_container_path VARCHAR(1024) DEFAULT '/home/kasm_user'` — container mount point

### Docker Lifecycle — Pause/Unpause

Two new methods on `DockerClient`:

- `pause_container_by_id(container_id: &str)` — calls `bollard::Docker::pause_container()`
- `unpause_container_by_id(container_id: &str)` — calls `bollard::Docker::unpause_container()`

Two new API endpoints:

- `POST /api/workspaces/{id}/pause` — validates status is `running`, calls Docker pause, updates DB status to `paused`
- `POST /api/workspaces/{id}/unpause` — validates status is `paused`, calls Docker unpause, updates DB status to `running`

State machine for the workspace lifecycle:

```
create → start → running ⇄ pause ↔ paused → unpause → running
                                           ↓
                                      stop → stopped → delete
```

Invalid transitions return HTTP 409 Conflict.

### Workspace Registry

**Format**: A single `workspace_registry.json` file hosted at a remote URL. Structure:

```json
{
  "name": "Registry Name",
  "description": "Registry description",
  "icon_url": "https://...",
  "workspaces": [
    {
      "friendly_name": "Ubuntu Desktop",
      "description": "Full Ubuntu desktop environment",
      "image": "kasmweb/desktop:ubuntu",
      "icon_url": "https://...",
      "categories": ["Desktop"],
      "arch": ["amd64", "arm64"],
      "cores": 2,
      "memory": 4294967296,
      "gpu_count": 0,
      "cpu_allocation_method": "Inherit",
      "run_config": {},
      "exec_config": {},
      "volume_mappings": {}
    }
  ]
}
```

**Sync mechanism**: On-demand via `POST /api/registry/sync`. The API fetches the URL, parses the JSON, stores it in a `registry_cache` table (or in-memory). Periodic auto-sync is deferred to v2.

**API endpoints**:

- `GET /api/registry` — returns the cached registry (or 404 if never synced)
- `POST /api/registry/sync` — fetches from configured URL, updates cache, returns the registry
- `GET /api/registry/url` — returns the currently configured registry URL (admin only)
- `PUT /api/registry/url` — sets the registry URL (admin only)

### Workspace Creation Flow

1. User clicks a workspace card on the dashboard (or "New Workspace" button).
2. A modal opens showing: workspace name input, resource overrides (CPU, RAM, GPU, persistent storage toggle), pre-filled from registry defaults.
3. User submits → API creates workspace record + starts Docker container + writes Traefik routes.
4. Dashboard updates to show the new workspace card.

### API Endpoints (v1 complete list)

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/health` | No | Health check |
| POST | `/api/auth/login` | No | Login |
| POST | `/api/auth/register` | No | Register user |
| GET | `/api/auth/validate` | No | ForwardAuth endpoint |
| GET | `/api/auth/me` | Yes | Current user info |
| POST | `/api/auth/logout` | No | Logout |
| GET | `/api/users` | Yes | List users |
| GET | `/api/users/{id}` | Yes | Get user |
| DELETE | `/api/users/{id}` | Admin | Delete user |
| GET | `/api/workspaces` | Yes | List workspaces (admin: all, user: own) |
| POST | `/api/workspaces` | Yes | Create workspace |
| GET | `/api/workspaces/{id}` | Yes | Get workspace detail |
| DELETE | `/api/workspaces/{id}` | Yes | Delete workspace |
| POST | `/api/workspaces/{id}/start` | Yes | Start workspace |
| POST | `/api/workspaces/{id}/stop` | Yes | Stop workspace |
| POST | `/api/workspaces/{id}/pause` | Yes | Pause workspace |
| POST | `/api/workspaces/{id}/unpause` | Yes | Resume paused workspace |
| GET | `/api/registry` | Yes | Get cached registry |
| POST | `/api/registry/sync` | Admin | Sync registry from remote URL |
| GET | `/api/registry/url` | Admin | Get configured registry URL |
| PUT | `/api/registry/url` | Admin | Set registry URL |
| GET | `/api/docker/containers` | Yes | List Docker containers (raw) |
| POST | `/api/docker/containers/create` | Yes | Create arbitrary Docker container |
| GET | `/api/vnc/verify` | Cookie | Traefik ForwardAuth verification |

### Frontend Routes (v1)

| Route | Page | Description |
|-------|------|-------------|
| `/login/` | Login | Username/password form |
| `/` | Dashboard | Workspace card grid (app-store style) |
| `/workspaces/new/` | New Workspace | Form: pick type, configure, create |
| `/workspaces/[id]/` | Workspace Detail | Status, lifecycle controls, VNC Connect |
| `/admin/users/` | User Management | Admin-only user table |
| `/vnc/[token]/` | VNC Viewer | VNC connection page |

### Dashboard Card Design

Minimal: workspace icon (from registry), workspace name, status badge (running/stopped/paused), owner name. The entire card is a link to the workspace detail page. No action buttons on the card — all actions live on the detail page.

### Workspace Detail Page Design

Shows: workspace name, status, image, CPU/RAM/GPU allocation, owner, persistent storage path. Lifecycle buttons: Start / Pause / Resume / Stop / Delete. "Connect → VNC" button opens VNC in a new tab.

### VNC Integration

Unchanged from current implementation. The VNC viewer page, RFB connection, Traefik WebSocket proxy, and ForwardAuth verification all remain as-is. The only change is that workspace routes use the new `/workspaces/` path prefix instead of `/instances/`.

## Testing Decisions

### Testing Philosophy

Tests verify **external behavior** (API responses, database state, Docker container state) rather than internal implementation details. A good test answers: "If I call this endpoint with this input, do I get the expected output and side effects?"

### Seam 1: API Contract Tests

**What**: Integration tests that start the full Axum server, send HTTP requests, and assert on response status codes and JSON bodies. Uses `sqlx::test` for database isolation and a real Docker daemon for container operations.

**Modules tested**: All route handlers in `routes.rs`.

**Prior art**: None — the API has no tests yet. This establishes the pattern.

**Approach**:
- Each test creates a fresh database via `sqlx::test` (PostgreSQL test database).
- Tests exercise the full request lifecycle: auth → create workspace → start → pause → unpause → stop → delete.
- Docker operations are tested against the real daemon (integration test, not unit test).

**Key test cases**:
- Create workspace → 201 + workspace JSON with status `running`
- Pause running workspace → 200 + status `paused`
- Pause already-paused workspace → 409 Conflict
- Unpause paused workspace → 200 + status `running`
- Unpause running workspace → 409 Conflict
- Stop paused workspace → 200 + status `stopped` (Docker unpause then stop)
- Delete running workspace → 204 (stop + remove + DB delete)
- List workspaces as admin → all workspaces returned
- List workspaces as user → only own workspaces returned
- Get workspace by ID → workspace JSON
- Get workspace by ID (not found) → 404
- Registry sync → 200 + registry JSON cached
- Registry sync (unreachable URL) → 502 or cached version returned

### Seam 2: Database Tests

**What**: Unit tests for `WorkspaceRepository` methods. Uses `sqlx::test` for isolated PostgreSQL test databases.

**Modules tested**: `WorkspaceRepository` (renamed from `InstanceRepository`).

**Prior art**: None — establishes the pattern.

**Key test cases**:
- Create workspace → record exists with correct fields
- Find by ID → returns workspace or None
- Find by VNC token → returns workspace or None
- List by owner → returns only that owner's workspaces
- List all → returns all workspaces
- Update status → status column changes
- Update container ID → container_id column changes
- Delete → record removed, returns true
- Delete (not found) → returns false

### Seam 3: Docker Client Tests

**What**: Integration tests for `DockerClient` methods. Requires Docker daemon.

**Modules tested**: `DockerClient` (pause, unpause, inspect, start, stop, remove).

**Prior art**: None — establishes the pattern.

**Key test cases**:
- Pause container → container state becomes "Paused"
- Unpause container → container state becomes "Running"
- Inspect container state → returns correct state string
- Pause non-existent container → error
- Start stopped container → state becomes "Running"
- Stop running container → state becomes "Exited"

### Seam 4: Registry Sync Tests

**What**: Integration tests for registry sync endpoint.

**Modules tested**: Registry sync handler, HTTP fetch logic.

**Key test cases**:
- Sync with valid URL → registry cached and returned
- Sync with invalid URL → error response
- GET registry (never synced) → 404
- GET registry (after sync) → cached registry returned
- Set URL + sync → correct URL fetched

### Test Infrastructure

- Add `tokio-test`, `tower` (for `ServiceExt`), and `reqwest` (for test HTTP client) to `[dev-dependencies]` in `Cargo.toml`.
- Use `sqlx::test` for PostgreSQL test database provisioning.
- Tests are marked `#[tokio::test]` and run with `cargo test`.
- Docker integration tests are gated behind an env var or feature flag to avoid failing in CI without Docker.

## Out of Scope

- **Instance editing while stopped** — changing image, resizing RAM, or modifying volumes after creation. Deferred to v2.
- **Batch operations** — "pause all," "stop by user," "restart all." Deferred to v2.
- **Container logs viewer** — real-time log streaming from Docker. Deferred to v2.
- **Resource monitoring** — CPU/RAM usage graphs via Docker stats. Deferred to v2.
- **Periodic auto-sync** — background registry sync on a timer. Deferred to v2.
- **Non-VNC workloads** — general-purpose container support (Jupyter, web servers, headless). Deferred to v2.
- **GPU passthrough** — the config field exists but actual GPU allocation via Docker is not implemented in v1.
- **Persistent storage path enforcement** — the path is computed and stored, but directory creation on the host is not handled by the API in v1. The Docker volume mount relies on the host path existing.
- **Frontend tests** — no SvelteKit component or E2E tests in v1. The Playwright config exists but is not actively used.
- **HTTPS/TLS for API** — the API runs on HTTP behind Traefik TLS termination. No changes needed.

## Further Notes

### Migration Strategy

The rename from "instances" to "workspaces" is a breaking change for the API. Since this is v1 (pre-production), there is no backward compatibility concern. All frontend routes, API paths, and JSON keys change simultaneously.

### Registry as Single Source of Truth for Image Config

The workspace registry defines *what* Docker images are available and their default resource configurations. When a user creates a workspace, the API reads the registry entry for the selected workspace type and uses those defaults. User overrides (custom CPU/RAM) are stored in the database. This separation means:

- Registry = catalog of available workspace types (admin-managed, remote)
- Database = per-user workspace instances (user-created, with overrides)

### Traefik Dynamic Routing

No changes to the Traefik routing logic. The existing `write_vnc_route` / `delete_vnc_route` functions continue to work with workspace VNC tokens. The route files are written to `docker/openworkspace_dev/traefik/dynamic/` as before.

### KasmVNC Container Configuration

The existing `create_kasm_container` method is extended to accept resource limits (CPU, RAM) from the workspace config. The bollard `Config` struct's `host_config` field is populated with `nano_cpus` and `memory` parameters. The `kasmvnc.yaml` injection and network connection logic remain unchanged.
