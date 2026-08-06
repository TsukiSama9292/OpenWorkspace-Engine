# API Reference

Rust/Axum API serving the SvelteKit dashboard. Base URL: `/api` (Traefik `api-router` → service on `:3000`).

## Conventions

- **Auth:** all endpoints (except login) require the `ow_token` JWT cookie. Unauthenticated → `401`; authenticated but insufficient permission → `403`.
- **Authorization:** there is no per-user role column. Permissions come from group memberships and are resolved fresh from the database on every request into an **effective context** (five permission flags, a template whitelist, an instance ceiling, and a derived tier). The JWT carries only identity — nothing role-bearing is decoded from it. See [RBAC](rbac.md).
- **Responses:** JSON. Error bodies are either a bare status code (e.g. `404`) or `{ "error": "..." }` for instance/template routes.
- **Deletes** return `204 No Content` on success.

## Auth (`apps/api/src/routes/auth/`)

### POST `/api/auth/login`

Body:
```json
{ "username": "admin", "password": "admin" }
```
Sets `Set-Cookie: ow_token=<JWT>; Path=/; HttpOnly; Secure; SameSite=Lax; Max-Age=604800` (7 days). Returns the caller's effective context:
```json
{
  "context": {
    "user_id": "…",
    "username": "admin",
    "is_admin": true,
    "tier": 2,
    "can_create_template": true,
    "can_manage_users": true,
    "can_manage_group_instances": true,
    "can_manage_docker": true,
    "can_manage_registry": true,
    "effective_max_instances": 0,
    "allowed_template_ids": ["…"],
    "group_ids": ["…"],
    "direct_max_instances": null
  }
}
```
`401` on unknown user or bad password.

### GET `/api/auth/me`

Returns the current user's effective context, **recomputed from the DB on every call** (a permission change takes effect on the very next request):
```json
{ "context": { … same shape as login … } }
```

### GET `/api/auth/validate`

Cheap identity check (the JWT decode + DB context resolution):
```json
{ "user_id": "…", "username": "admin", "is_admin": true, "tier": 2 }
```

### POST `/api/auth/logout`

Clears the cookie (`Max-Age=0`) and returns `{ "status": "ok" }`.

### POST `/api/auth/change-password`

Body `{ "current_password", "new_password" }` → `{ "status": "ok" }`. Any user may rotate their own password; `400` on an empty `new_password` or a wrong `current_password`.

> There is **no** `/api/auth/register` — user creation happens only through `POST /api/users` (`can_manage_users`).

## Users (`apps/api/src/routes/users.rs`)

| Endpoint | Requirement | Description |
|----------|-------------|-------------|
| `GET /api/users` | `can_manage_users` | List all users with their policy |
| `POST /api/users` | `can_manage_users` | Create a user |
| `GET /api/users/{id}` | `can_manage_users`, or self | Get one user |
| `PUT /api/users/{id}` | `can_manage_users` (self: password only) | Update a user |
| `DELETE /api/users/{id}` | `can_manage_users` + tier guardrail | Delete a user |

`GET /api/users` → `{ "users": [{ "id", "username", "created_at", "direct_max_instances", "group_ids", "is_admin", "tier" }] }`

`POST /api/users` body: `{ "username", "password", "group_ids"? }`. When `group_ids` is absent or empty the new account is placed in the **User** system group. The actor may assign the target only into groups whose tier is strictly below their own (an admin cannot place anyone into the Admin group; a manager cannot place anyone into Manager/Admin). Returns `{ "user": { … } }`. Password is bcrypt-hashed (cost 10).

`PUT /api/users/{id}` body: `{ "username"?, "password"?, "group_ids"?, "direct_max_instances"? }` — all fields optional; policy fields left absent are untouched. A non-admin may update only their own **password**. Policy writes (memberships / personal ceiling) to a target require the actor's tier to be **strictly greater** than the target's (admins exempt) — a non-admin can never write their own policy. An Admin-group member's membership list is **immutable**: any `group_ids` payload targeting an Admin member → `403` (the Admin group id can't be assigned and dropping it is forbidden), so no one can demote the root account; identity/password and `direct_max_instances` edits on an Admin member remain allowed. `direct_max_instances` accepts an integer (set), `null` (clear the personal ceiling), or absent (leave untouched); it can only *raise* the effective ceiling, never lower it. Returns the updated user.

`DELETE /api/users/{id}` → `204`. Deleting a target whose tier is not strictly below the actor's → `403`; deleting an **Admin-group member** → `403` for every actor (self, another admin, or a non-admin) — the root account can never be deleted through the API.

## Groups (`apps/api/src/routes/groups.rs`)

| Endpoint | Requirement | Description |
|----------|-------------|-------------|
| `GET /api/groups` | `can_manage_users` | List all groups with their template whitelist |
| `POST /api/groups` | admin only | Create a group |
| `PUT /api/groups/{id}` | admin only | Update a group |
| `DELETE /api/groups/{id}` | admin only | Delete a custom group (204) |

`GET /api/groups` → `{ "groups": [{ "id", "name", "description", "kind", "can_create_template", "can_manage_users", "can_manage_group_instances", "can_manage_docker", "can_manage_registry", "max_instances", "template_ids" }] }`

`POST`/`PUT` body (`GroupInput`): `{ "name", "description"?, "can_create_template"?, "can_manage_users"?, "can_manage_group_instances"?, "can_manage_docker"?, "can_manage_registry"?, "max_instances"? (default 2, 0 = unlimited), "template_ids"? }`. Flags and `max_instances` default to the schema defaults; the template whitelist defaults to empty (Admin-whitelist backfill only happens on template creation). Duplicate name → `409`.

Rules: system groups (`kind` `admin`/`manager`/`user`) cannot be renamed or deleted; Admin flags are fixed all-on and User flags fixed all-off (both ceilings stay editable); custom groups (`kind` = `null`) take the payload verbatim.

## Templates (`apps/api/src/routes/workspace/templates.rs`)

| Endpoint | Requirement | Description |
|----------|-------------|-------------|
| `GET /api/templates` | any authenticated | List templates (global browsable catalog, hidden included) |
| `POST /api/templates` | `can_create_template` | Create a template |
| `GET /api/templates/{id}` | any authenticated | Get a template |
| `PUT /api/templates/{id}` | own template + `can_create_template`, or admin | Update |
| `DELETE /api/templates/{id}` | own template + `can_create_template`, or admin | Delete (204) |

List/get return the full catalog to every authenticated user — hidden templates included, so the management UI can display and restore them. Update/delete require `can_create_template` **and** ownership, or admin.

Launch is gated separately by the group whitelist + template visibility (see [RBAC](rbac.md#launch-authorization-pre-flight)); a template can be visible in the catalog but not launchable by a given user.

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
| `visibility` | string | `private` (`public`/`private`/`hidden`) |

Validation: `max_run_seconds`/`keep_time_seconds` ≥ 60; `timeout_action`/`keep_time_action` ∈ `{remove, stop, pause}`; bandwidth ≥ 0; `visibility` ∈ `{public, private, hidden}`. Invalid → `400`.

Template JSON additionally includes `container_runtime` normalized to `"docker"` when empty, `visibility`, plus `instance_count` (running+stopped instances using it) and `created_at`/`updated_at`.

A new template whitelists the **Admin** group by default (no other group), so it is immediately admin-usable; the creator gets **no automatic access** — access is granted by whitelisting one of the user's groups via the group-management API.

## Instances (`apps/api/src/routes/workspace/instances.rs`)

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/instances` | GET | List instances (own, plus group-visible for `can_manage_group_instances`; all for admin) |
| `/api/instances` | POST | Launch an instance from a template |
| `/api/instances/{id}` | GET | Get one instance |
| `/api/instances/{id}` | DELETE | Delete (204), keeps persistent data |
| `/api/instances/{id}/start` | POST | Start / restart |
| `/api/instances/{id}/stop` | POST | Stop (route kept, port reserved) |
| `/api/instances/{id}/pause` | POST | Pause (must be running) |
| `/api/instances/{id}/unpause` | POST | Resume (must be paused) |
| `/api/instances/{id}/heartbeat` | POST | Bump `last_seen_at` (keep-time refresh) |

All instance mutations require the caller to be the owner, an admin, or a group-instance holder whose target owner shares a group and is of a strictly lower tier (`can_manage_group_instances` + tier guardrail). `GET /api/instances` scoping: admins see all; everyone else sees their own, plus — for a `can_manage_group_instances` holder — instances owned by users sharing at least one group and of a strictly lower tier.

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
  "owner_group_ids": ["uuid", "…"],
  "owner_tier": 0,
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

#### Launch pre-flight

A launch attempt runs an ordered pre-flight before any reservation is written:

1. **Template visibility** — `hidden` → `403` for everyone, admins included.
2. **Template whitelist** — a `private` template must be in the user's effective whitelist; `public` skips this check. No tier is exempt. `403`.
3. **Per-user effective ceiling** — the user's active count must stay below `effective_max_instances`. `409`.
4. **Global host ceiling** — the global active count must stay below `host_instance_limit` (0 = unlimited), for every tier. `409`.

Active = `running`/`starting`/`paused`; `stopped`/`error` never count. The per-user ceiling is exact (single-user-row `FOR UPDATE`); the host ceiling is best-effort.

A rejection body carries the reason:
```json
{
  "error": "Per-user instance limit reached (active: 2, limit: 2)",
  "rejection": { "scope": "user_instance", "current": 2, "limit": 2, "requested": 1 }
}
```
Scopes: `template_not_allowed`, `template_hidden` (403), `user_instance`, `host_instance` (409).

### Lifecycle endpoints

- `POST /{id}/start` → `{ "status": "starting", "container_id": … }`. Reuses the persisted host port and `/30` network; recreates the container only if missing or the port was stolen. Returns `409` if already running.
- `POST /{id}/stop` → `{ "status": "stopped" }`. **Keeps the Traefik route and host port** (stable bookmarked URL, zero churn on restart). `409` if already stopped.
- `POST /{id}/pause` → `{ "status": "paused" }` (`409` unless running).
- `POST /{id}/unpause` → `{ "status": "running" }` (`409` unless paused).
- `POST /{id}/heartbeat` → `{ "status": "ok" }`.

All error bodies use `{ "error": "…" }`.

## Registry (`apps/api/src/routes/workspace/registry.rs`)

Registry data is fetched from a configurable URL and cached in the `registry_cache` table. All four endpoints require `can_manage_registry` (admin included).

| Endpoint | Method | Requirement | Description |
|----------|--------|-------------|-------------|
| `/api/registry` | GET | `can_manage_registry` | Return the cached registry JSON |
| `/api/registry/sync` | POST | `can_manage_registry` | Re-fetch from the configured URL and refresh the cache |
| `/api/registry/url` | GET | `can_manage_registry` | `{ "url": "…" }` |
| `/api/registry/url` | PUT | `can_manage_registry` | `{ "url": "…" }` |

`GET /api/registry` → `404` if the cache is empty. Sync errors → `502` (upstream fetch/parse failure) or `400` (no URL configured).

## Docker (`apps/api/src/routes/workspace/docker_raw.rs`)

Raw Docker passthrough for the admin console. Both endpoints require `can_manage_docker` (admin included).

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/docker/containers` | GET | `{ "containers": [{ "id", "names", "image", "status", "state" }] }` (all containers, incl. stopped) |
| `/api/docker/containers/create` | POST | Body `{ "name", "image" }` → `{ "container_id" }` |

## Persistent volumes (`apps/api/src/routes/workspace/persistent_volumes.rs`)

Both endpoints require `can_manage_users` (admin included) and are never scoped by group.

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/persistent-volumes` | GET | `{ "volumes": [{ "id", "host_path", "owner_id", "owner_username", "status", "created_at" }] }` — only `orphaned` rows |
| `/api/persistent-volumes/{id}/cleanup` | POST | Double-confirmed "thorough cleanup" → `204`; an `active` (still-referenced) volume → `409` |

## System settings (`apps/api/src/routes/admin_settings.rs`)

Both endpoints are admin-only.

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/admin/settings` | GET | `{ "settings": { "host_instance_limit": 0 } }` |
| `/api/admin/settings` | PUT | Body `{ "host_instance_limit": 0 }` (≥ 0, 0 = unlimited) → the updated settings |

`host_instance_limit` is the only global knob; it caps the total number of **active** instances across all users and applies to every tier, admins included.

## Proxy verify (`apps/api/src/routes/proxy/vnc.rs`)

### GET `/api/vnc/verify`

Traefik **ForwardAuth** endpoint (declared as the `vnc-auth` middleware in `static-routers.yml`, currently **not attached to any router** — the active per-instance gate is Traefik's Basic header injection). Reads the `ow_token` cookie manually (not via Axum's cookie jar), decodes the JWT, and validates the instance:

- Extracts the token from `X-Forwarded-Uri` (`/kasmvnc/{token}/websockify`)
- Checks the `VncCache` first (`{ status }`); on miss, falls back to `find_by_access_token` and populates the cache
- Returns `404` if the instance isn't `running`; otherwise sets `X-Forwarded-User` (the JWT's `sub`) — no role header, matching the identity-only JWT

## Health

### GET `/api/health`

`{ "status": "ok" }` — unauthenticated liveness probe (also mounted at `/health`).

## Errors

| Code | Meaning |
|------|---------|
| `400` | Bad request (validation, empty fields, invalid group ids) |
| `401` | Missing/invalid `ow_token` |
| `403` | Authenticated but insufficient permission (flag gate, tier guardrail, template whitelist/`hidden`) |
| `404` | Resource not found |
| `409` | Conflict (already running/stopped, persistent instance exists, pre-flight `user_instance`/`host_instance` ceiling, cleaning an active volume, duplicate group name) |
| `500` | Internal error |
| `502` | Upstream registry fetch failed |
| `204` | Successful delete (no body) |
