# API Reference

Rust/Axum API serving the SvelteKit dashboard. Base URL: `/api` (Traefik `api-router` → service on `:3000`).

## Conventions

- **Auth:** all endpoints (except login) require the `ow_token` JWT cookie. Unauthenticated → `401`; unauthorized role → `403`.
- **Roles:** `admin`, `manager`, `user`. Authorization is enforced per-handler via `auth.rs` (see [RBAC](rbac.md)).
- **Responses:** JSON. Error bodies are either a bare status code (e.g. `404`) or `{ "error": "..." }` for instance/template routes.
- **Deletes** return `204 No Content` on success.

## Auth (`apps/api/src/routes/auth/`)

### POST `/api/auth/login`

Body:
```json
{ "username": "admin", "password": "admin" }
```
Sets `Set-Cookie: ow_token=<JWT>; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=604800` (7 days). Returns the user (no `password_hash`):
```json
{ "user": { "id": "…", "username": "admin", "role": "admin" } }
```
`401` on unknown user or bad password.

### GET `/api/auth/me`

Returns the current user from the DB (fresh role):
```json
{ "user": { "id": "…", "username": "admin", "role": "admin", "created_at": "…" } }
```

### GET `/api/auth/validate`

Returns the role decoded straight from the JWT (no DB hit):
```json
{ "user_id": "…", "role": "admin" }
```

### POST `/api/auth/logout`

Clears the cookie (`Max-Age=0`) and returns `{ "status": "ok" }`.

> There is **no** `/api/auth/register` — user creation happens only through `POST /api/users` (Admin/Manager).

## Users (`apps/api/src/routes/users.rs`)

| Endpoint | Roles | Description |
|----------|-------|-------------|
| `GET /api/users` | Admin, Manager (`can_manage_users`) | List all users |
| `POST /api/users` | Admin, Manager | Create a user |
| `GET /api/users/{id}` | Admin, self | Get one user |
| `PUT /api/users/{id}` | Admin, Manager, self (password only) | Update a user |
| `DELETE /api/users/{id}` | Admin, Manager | Delete a user (Admin targets are forbidden) |

`GET /api/users` → `{ "users": [{ "id", "username", "role", "created_at" }] }`

`POST /api/users` body: `{ "username", "password", "role": "user"|"manager"|"admin" }` (role optional, defaults `user`). Returns `{ "user": { "id", "username", "role" } }`. Password is bcrypt-hashed (cost 10).

`PUT /api/users/{id}` body: `{ "username"?, "password"?, "role"? }` — all fields optional. A non-admin may only update their own **password** (not username/role); editing an admin requires the caller to be admin. Returns the updated user.

`DELETE /api/users/{id}` → `204`. Deleting an admin → `403`.

## Templates (`apps/api/src/routes/workspace/templates.rs`)

| Endpoint | Roles | Description |
|----------|-------|-------------|
| `GET /api/templates` | any (scoped) | List templates |
| `POST /api/templates` | Admin, Manager | Create a template |
| `GET /api/templates/{id}` | any | Get a template |
| `PUT /api/templates/{id}` | Admin, Manager, owner | Update |
| `DELETE /api/templates/{id}` | Admin, Manager, owner | Delete (204) |

List is scoped by `can_view_all_instances()`: Admin/Manager see all, regular users see their own templates. Update/delete require `can_manage_templates()` **or** template ownership.

`POST /api/templates` body (all fields except `name` have defaults):

| Field | Type | Default |
|-------|------|---------|
| `name` | string | *(required)* |
| `description` | string | `null` |
| `image` | string | `tsukisama9292/ow-kasmvnc-ubuntu-dini:jammy` |
| `cores` | int | `2` |
| `memory` | int (bytes) | `4294967296` (4 GiB) |
| `gpu_count` | int | `0` |
| `docker_registry` | string? | `null` |
| `remote_type` | string | `kasmvnc` |
| `run_config` | object | `{}` |
| `exec_config` | object | `{}` |
| `volume_mappings` | object | `{}` |
| `persistent_storage_path` | string? | `null` |
| `container_runtime` | string | `docker` |
| `max_run_seconds` | int? | `null` |
| `timeout_action` | string | `remove` |
| `keep_time_seconds` | int? | `null` |
| `keep_time_action` | string | `pause` |
| `network_bandwidth_up_mbps` | int | `0` (unlimited) |
| `network_bandwidth_down_mbps` | int | `0` (unlimited) |
| `docker_in_instance` | bool | `false` |

Validation: `max_run_seconds`/`keep_time_seconds` ≥ 60; `timeout_action`/`keep_time_action` ∈ `{remove, stop, pause}`; bandwidth ≥ 0. Invalid → `400`.

Template JSON additionally includes `container_runtime` normalized to `"docker"` when empty, plus `instance_count` (running+stopped instances using it) and `created_at`/`updated_at`.

## Instances (`apps/api/src/routes/workspace/instances.rs`)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/instances` | GET | List instances (scoped by `can_view_all_instances`) |
| `/api/instances` | POST | Launch an instance from a template |
| `/api/instances/{id}` | GET | Get one instance |
| `/api/instances/{id}` | DELETE | Delete (204), keeps persistent data |
| `/api/instances/{id}/start` | POST | Start / restart |
| `/api/instances/{id}/stop` | POST | Stop (route kept, port reserved) |
| `/api/instances/{id}/pause` | POST | Pause (must be running) |
| `/api/instances/{id}/unpause` | POST | Resume (must be paused) |
| `/api/instances/{id}/heartbeat` | POST | Bump `last_seen_at` (keep-time refresh) |

All instance mutations require the caller to be the owner or have manager-over-owner rights (`can_manage_instance`). `GET /api/instances` scoping: Admin/Manager → all, User → own.

### `GET /api/instances`

```json
{ "instances": [ { "instance": { … } } ] }
```
Each instance (from `instance_to_json`):

```json
{
  "id": "uuid",
  "template_id": "uuid",
  "name": "…",
  "instance_number": 3,
  "owner_id": "uuid",
  "owner_username": "bob",
  "owner_role": "user",
  "container_id": "sha256:…",
  "host_port": 10042,
  "network_name": "ow-<instance-id>",
  "status": "running",
  "access_token": "…",
  "access_password": "…",
  "mount_persistent": true,
  "resolved_volume_host_path": "/…",
  "started_at": "…",
  "template_name": "…",
  "remote_type": "kasmvnc",
  "auto_sleeps_at": "… | null",
  "timeout_action": "remove",
  "keep_time_deadline": "… | null",
  "keep_time_seconds": 3600,
  "keep_time_action": "pause",
  "created_at": "…",
  "updated_at": "…"
}
```

Deadline semantics: `auto_sleeps_at` = `started_at + max_run_seconds`, `keep_time_deadline` = `last_seen_at + keep_time_seconds` — both only populated while `running`. `access_token` / `access_password` are the per-instance credentials the session pages and Traefik routes use.

### `POST /api/instances` (launch)

Body:
```json
{
  "template_id": "uuid",
  "persistence": "use_persistent" | "no_persistent" | "reset_persistent",
  "mount_persistent": true
}
```
`persistence` (optional) takes precedence over the legacy `mount_persistent` bool. Semantics:

| Mode | Behavior |
|------|----------|
| `no_persistent` | No volume mounted |
| `use_persistent` | Mount the owner's persistent volume for the template, reusing data |
| `reset_persistent` | Wipe existing data, then mount a fresh volume |

One persistent instance per (template, owner) — a second `use_persistent` launch with an existing non-`error` instance → `409`. A prior `error` record is replaced (dropped + wiped).

Returns `{ "instance": { … } }`. On a failure after the DB record was created, the instance is left in `error` status and the body includes `{ "instance": …, "docker_error": "…" }` (never a silent failure).

### Lifecycle endpoints

- `POST /{id}/start` → `{ "status": "starting", "container_id": … }`. Reuses the persisted host port and `/30` network; recreates the container only if missing or the port was stolen. Returns `409` if already running.
- `POST /{id}/stop` → `{ "status": "stopped" }`. **Keeps the Traefik route and host port** (stable bookmarked URL, zero churn on restart). `409` if already stopped.
- `POST /{id}/pause` → `{ "status": "paused" }` (`409` unless running).
- `POST /{id}/unpause` → `{ "status": "running" }` (`409` unless paused).
- `POST /{id}/heartbeat` → `{ "status": "ok" }`.

All error bodies use `{ "error": "…" }`.

## Registry (`apps/api/src/routes/workspace/registry.rs`)

Registry data is fetched from a configurable URL and cached in the `registry_cache` table.

| Endpoint | Method | Roles | Description |
|----------|--------|-------|-------------|
| `/api/registry` | GET | any | Return the cached registry JSON |
| `/api/registry/sync` | POST | Admin, Manager | Re-fetch from the configured URL and refresh the cache |
| `/api/registry/url` | GET | Admin, Manager | `{ "url": "…" }` |
| `/api/registry/url` | PUT | Admin, Manager | `{ "url": "…" }` |

`GET /api/registry` → `404` if the cache is empty. Sync errors → `502` (upstream fetch/parse failure) or `400` (no URL configured).

## Docker (`apps/api/src/routes/workspace/docker_raw.rs`)

Raw Docker passthrough for the admin console. Both endpoints require `can_manage_docker` (Admin, Manager).

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/docker/containers` | GET | `{ "containers": [{ "id", "names", "image", "status", "state" }] }` (all containers, incl. stopped) |
| `/api/docker/containers/create` | POST | Body `{ "name", "image" }` → `{ "container_id" }` |

## Proxy verify (`apps/api/src/routes/proxy/vnc.rs`)

### GET `/api/vnc/verify`

Traefik **ForwardAuth** endpoint (declared as the `vnc-auth` middleware in `static-routers.yml`, currently **not attached to any router** — the active per-instance gate is Traefik's Basic header injection). Reads the `ow_token` cookie manually (not via Axum's cookie jar), decodes the JWT, and validates the instance:

- Extracts the token from `X-Forwarded-Uri` (`/kasmvnc/{token}/websockify`)
- Checks the `VncCache` first (`{ status }`); on miss, falls back to `find_by_access_token` and populates the cache
- Returns `404` if the instance isn't `running`; otherwise sets `X-Forwarded-User` / `X-Forwarded-Role` headers

## Health

### GET `/api/health`

`{ "status": "ok" }` — unauthenticated liveness probe (also mounted at `/health`).

## Errors

| Code | Meaning |
|------|---------|
| `400` | Bad request (validation, empty fields, invalid role) |
| `401` | Missing/invalid `ow_token` |
| `403` | Authenticated but insufficient role/ownership |
| `404` | Resource not found |
| `409` | Conflict (already running/stopped, persistent instance exists) |
| `500` | Internal error |
| `502` | Upstream registry fetch failed |
| `204` | Successful delete (no body) |
