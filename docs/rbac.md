# Role-Based Access Control (RBAC)

## Overview

OpenWorkspace uses a three-tier RBAC system: **Admin → Manager → User**. The admin account is seeded at first startup (`ADMIN_PASSWORD`, username `admin`) and cannot be deleted via the API. Role checks are centralized in `apps/api/src/auth.rs` (`Role` + `AuthUser`), enforced per-handler.

## Roles & Permission Matrix

| Area | Action | Admin | Manager | User |
|------|--------|:-----:|:-------:|:----:|
| **Instances** | Create / launch | ✅ | ✅ | ✅ |
| | View all | ✅ | ✅ | ❌ (own only) |
| | Manage (start/stop/pause/delete) any | ✅ | ❌ | ❌ |
| | Manage own | ✅ | ✅ | ✅ |
| | Manage other **users'** instances | ✅ | ✅ | ❌ |
| | Manage **managers'/admins'** instances | ✅ | ❌ | ❌ |
| **Templates** | Create | ✅ | ✅ | ✅ |
| | View all | ✅ | ✅ | ❌ (own only) |
| | Edit / delete own | ✅ | ✅ | ✅ |
| | Edit / delete others' | ✅ | ✅ | ❌ |
| **Users** | List all (`GET /api/users`) | ✅ | ✅ | ❌ |
| | Get `{id}` | ✅ (any) | ❌ (self only) | self only |
| | Create / edit / delete `user`-role accounts | ✅ | ✅ | ❌ |
| | Create / edit / delete `manager`/`admin` accounts | ✅ | ❌ | ❌ |
| | Edit self | ✅ | ✅ | password only |
| **Registry** | View cached (`GET /api/registry`) | ✅ | ✅ | ✅ |
| | Sync / set URL | ✅ | ✅ | ❌ |
| **Docker** | List containers / create raw container | ✅ | ✅ | ❌ |

Key point: **Manager ≈ Admin for everything except**: managing Manager/Admin-owned instances, and creating/editing/deleting Manager/Admin accounts.

## Implementation — `apps/api/src/auth.rs`

```rust
pub enum Role { Admin, Manager, User }

impl Role {
    pub fn can_manage_users(&self) -> bool        // Admin, Manager
    pub fn can_create_role(&self, target: &Role) -> bool
        // Admin → any; Manager → User only; User → false
    pub fn can_manage_templates(&self) -> bool    // Admin, Manager
    pub fn can_view_all_instances(&self) -> bool  // Admin, Manager
    pub fn can_manage_all_instances(&self) -> bool // Admin only
    pub fn can_manage_instance(&self, owner_role: &Role) -> bool
        // Admin → true; Manager → owner is User; User → false
    pub fn can_manage_docker(&self) -> bool       // Admin, Manager
    pub fn can_manage_registry(&self) -> bool     // Admin, Manager
}
```

The test suite pins this matrix (e.g. `test_role_can_manage_docker` asserts Admin **and** Manager both pass; `test_role_can_manage_all_instances` asserts Manager fails).

### Ownership rule for instances

Instance mutation endpoints (start/stop/pause/unpause/delete/heartbeat) use `can_manage_instance`, but ownership always wins:

```rust
async fn can_manage_instance(state, auth, instance) -> Result<bool, StatusCode> {
    if instance.owner_id == auth.user_id {
        return Ok(true);                     // owner can always control own instance
    }
    let owner_role = get_owner_role(state, instance.owner_id);
    Ok(auth.role.can_manage_instance(&owner_role))  // Manager-over-User, Admin-over-any
}
```

### Template ownership rule

Update/delete require `can_manage_templates()` **or** `existing.owner_id == auth.user_id` (users can edit/delete templates they created). Create has no role check (any authenticated user). Listing is scoped by `can_view_all_instances()`.

### User-account rules (`apps/api/src/routes/users.rs`)

- `list_users` / `create_user` / `delete_user`: require `can_manage_users` (Admin, Manager).
- `delete_user`: additionally `can_create_role(target_role)` — a Manager cannot delete a Manager, and **no one can delete an Admin** (`target_role == Admin → 403`).
- `update_user`: editing an Admin requires the caller to be Admin; a non-admin may only change their **own password** (username/role changes → 403); changing a user's role requires `can_create_role`.
- `get_user`: **admin or self** — a Manager cannot fetch another user's record via `GET /api/users/{id}` (list still works).

## Frontend (`apps/web/src/lib/stores/auth.ts`)

```typescript
export const isAdmin = derived(auth, ($auth) => $auth?.role === 'admin');
export const isManager = derived(auth, ($auth) => $auth?.role === 'admin' || $auth?.role === 'manager');
```

The dashboard hides/shows UI with these. `canControlInstance` (in `+page.svelte`) mirrors the server rule:

```typescript
function canControlInstance(inst: Instance): boolean {
  if (inst.owner_id === $auth?.id) return true;
  if ($auth?.role === 'admin') return true;
  if ($auth?.role === 'manager' && inst.owner_role === 'user') return true;
  return false;
}
```

`canEditUser`/`canDeleteUser` additionally block Managers from editing/deleting other Managers (and everyone from Admins).

## Admin Account

- Seeded at first startup: username `admin`, password from `ADMIN_PASSWORD` (default `admin`).
- Cannot be deleted via the API (`delete_user` rejects Admin targets).
- Cannot be edited by Managers (non-admin editing an Admin → 403).

## Migration Path

Adding a new role requires:
1. Extend `Role` enum in `auth.rs`
2. Add permission methods + their unit tests (the zero-warning gate won't accept a method with no test coverage of its matrix)
3. Update frontend `canControl*` / `isX` helpers
4. Add route-guard checks in the handlers that need it
