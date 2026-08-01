# API Reference

Base URL: `http://localhost:3000`

All endpoints return JSON. Protected endpoints require a valid `ow_token` cookie (set by `/api/auth/login`).

## Authentication

### Login

```
POST /api/auth/login
```

**Request:**
```json
{
  "username": "admin",
  "password": "admin"
}
```

**Response:**
```json
{
  "token": "eyJ0eXAiOiJKV1QiLCJhbGci..."
}
```

Sets `Set-Cookie: ow_token=...; Path=/; HttpOnly; SameSite=Lax; Max-Age=86400`

---

### Register

```
POST /api/auth/register
```

**Request:**
```json
{
  "username": "user1",
  "password": "pass123"
}
```

**Response:**
```json
{
  "user": {
    "id": "uuid",
    "username": "user1",
    "role": "user"
  }
}
```

---

### Validate Token

```
GET /api/auth/validate
```

**Auth:** Required (cookie)

**Response:**
```json
{
  "user_id": "uuid",
  "role": "admin"
}
```

---

### Get Current User

```
GET /api/auth/me
```

**Auth:** Required

**Response:**
```json
{
  "id": "uuid",
  "username": "admin",
  "role": "admin"
}
```

---

### Logout

```
POST /api/auth/logout
```

Clears the `ow_token` cookie.

---

## Users (Admin)

### List Users

```
GET /api/users
```

**Auth:** Required (admin)

**Response:**
```json
{
  "users": [
    {
      "id": "uuid",
      "username": "admin",
      "role": "admin",
      "created_at": "2026-07-23T00:00:00Z"
    }
  ]
}
```

---

### Get User

```
GET /api/users/{id}
```

**Auth:** Required (admin)

---

### Delete User

```
DELETE /api/users/{id}
```

**Auth:** Required (admin)

---

## Instances

### List Instances

```
GET /api/instances
```

**Auth:** Required

Non-admin users see only their own instances. Admin users see all.

**Response:**
```json
{
  "instances": [
    {
      "id": "uuid",
      "name": "dev-1",
      "instance_number": 1,
      "container_id": "a300dff243bf",
      "status": "running",
      "owner_id": "uuid",
      "vnc_token": "44ce0bc9e30a47a29713331fffc849fd",
      "created_at": "2026-07-23T00:00:00Z"
    }
  ]
}
```

---

### Create Instance

```
POST /api/instances
```

**Auth:** Required

**Request:**
```json
{
  "template_id": "uuid",
  "persistence": "use_persistent"
}
```

`persistence` is optional (`use_persistent` / `no_persistent` / `reset_persistent`, default `no_persistent`). A client-supplied host path is ignored. See [Persistent Storage](persistent-storage.md).

**Response:**
```json
{
  "instance": {
    "id": "uuid",
    "name": "dev-1",
    "instance_number": 6,
    "container_id": "a300dff243bf",
    "status": "running",
    "vnc_token": "44ce0bc9e30a47a29713331fffc849fd"
  }
}
```

**What happens:**
1. DB record created (UUID, instance_number, vnc_token)
2. KasmVNC container pulled and started on `ow-network` network
3. `kasmvnc.yaml` injected into container
4. Traefik route YAML files written to `traefik/dynamic/`

---

### Get Instance

```
GET /api/instances/{id}
```

**Auth:** Required (owner or admin)

---

### Delete Instance

```
DELETE /api/instances/{id}
```

**Auth:** Required (owner or admin)

**What happens:**
1. Traefik route files deleted
2. Docker container stopped and removed
3. DB record deleted

Persistent data (host dir + volume) is **preserved** for reuse; only a reset wipes it. See [Persistent Storage](persistent-storage.md).

---

### Start Instance

```
POST /api/instances/{id}/start
```

**Auth:** Required (owner or admin)

**What happens:**
1. Checks if container exists (inspect state)
2. If missing, creates a new container
3. If stopped, starts existing container
4. Writes Traefik route files

---

### Stop Instance

```
POST /api/instances/{id}/stop
```

**Auth:** Required (owner or admin)

**What happens:**
1. Docker container stopped
2. Traefik route files deleted
3. DB status updated to `stopped`

---

## Docker (Admin)

### List Containers

```
GET /api/docker/containers
```

**Auth:** Required (admin)

Returns all Docker containers on the host.

---

### Create Container

```
POST /api/docker/containers/create
```

**Auth:** Required (admin)

Low-level container creation. Prefer `/api/instances` for VNC instances.

---

## VNC (Internal)

### ForwardAuth Verify

```
GET /api/vnc/verify
```

**Auth:** None (called by Traefik, not browsers)

This endpoint is called by Traefik's ForwardAuth middleware before proxying WebSocket requests to KasmVNC. It validates the JWT cookie and instance ownership.

**Headers received from Traefik:**
- `Cookie: ow_token=...`
- `X-Forwarded-Uri: /vnc/{token}/websockify`

**Success (200):**
```
X-Forwarded-User: {user_id}
X-Forwarded-Role: {role}
```

**Errors:** 401 (invalid/missing JWT), 403 (instance not owned), 404 (instance not found or not running)

---

## Health Check

```
GET /health
```

**Response:** `{"status": "ok"}`
