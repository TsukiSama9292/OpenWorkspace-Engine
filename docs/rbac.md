# Role-Based Access Control (RBAC)

## Overview

OpenWorkspace uses a three-tier RBAC system: **Admin → Manager → User**. The admin account is seeded at first startup and cannot be modified. Managers can create users. Users can only manage their own instances.

## Roles

| Role | Permissions |
|------|-------------|
| **Admin** | Full access: manage all instances, all templates, all users, Docker registry, system settings |
| **Manager** | Create users, manage own instances + user instances, manage own templates + user templates |
| **User** | Manage own instances only, manage own templates only, change own password |

## Permission Matrix

### Instances

| Action | Admin | Manager | User |
|--------|-------|---------|------|
| Create | ✅ | ✅ | ✅ |
| View all | ✅ | ✅ (own + users) | ❌ |
| View own | ✅ | ✅ | ✅ |
| Stop/Start/Delete own | ✅ | ✅ | ✅ |
| Stop/Start/Delete user's | ✅ | ✅ | ❌ |
| Stop/Start/Delete manager's | ✅ | ❌ | ❌ |

### Users

| Action | Admin | Manager | User |
|--------|-------|---------|------|
| Create | ✅ | ✅ | ❌ |
| View all | ✅ | ❌ | ❌ |
| View self | ✅ | ✅ | ✅ |
| Edit user | ✅ | ✅ (users only) | ❌ |
| Delete user | ✅ | ✅ (users only) | ❌ |
| Edit self | ✅ | ✅ | ✅ (password only) |

### Docker Registry

| Action | Admin | Manager | User |
|--------|-------|---------|------|
| Push/Delete images | ✅ | ❌ | ❌ |
| List images | ✅ | ✅ | ✅ |

### Templates (Configs)

| Action | Admin | Manager | User |
|--------|-------|---------|------|
| Create | ✅ | ✅ | ✅ |
| Edit own | ✅ | ✅ | ✅ |
| Edit user's | ✅ | ✅ | ❌ |
| Delete own | ✅ | ✅ | ✅ |
| Delete user's | ✅ | ✅ | ❌ |

## Implementation

### Backend (`apps/api/src/auth.rs`)

```rust
pub enum Role {
    Admin,
    Manager,
    User,
}

impl Role {
    pub fn can_manage_instance(&self, owner_role: &Role) -> bool {
        match self {
            Role::Admin => true,
            Role::Manager => matches!(owner_role, Role::User),
            Role::User => false,
        }
    }

    pub fn can_manage_user(&self, target_role: &Role) -> bool {
        match self {
            Role::Admin => true,
            Role::Manager => matches!(target_role, Role::User),
            Role::User => false,
        }
    }
}
```

### Frontend (`apps/web/src/lib/stores/auth.ts`)

```typescript
export const isAdmin = derived(auth, ($auth) => $auth.user?.role === 'admin');
export const isManager = derived(auth, ($auth) => $auth.user?.role === 'manager');

export function canControlInstance(inst: Instance): boolean {
    if ($isAdmin) return true;
    if ($isManager && inst.owner_role === 'user') return true;
    return inst.owner_id === $auth.user?.id;
}
```

### Ownership Checks

Instance and template mutation endpoints verify ownership:

```rust
// Non-admin/manager can only modify their own resources
if auth.role != Role::Admin && auth.role != Role::Manager {
    if instance.owner_id != auth.user_id {
        return Err(StatusCode::FORBIDDEN);
    }
}
```

Manager restrictions:

```rust
// Manager can only manage user instances, not other managers/admins
if auth.role == Role::Manager {
    let owner_role = get_owner_role(&instance.owner_id);
    if !auth.role.can_manage_instance(&owner_role) {
        return Err(StatusCode::FORBIDDEN);
    }
}
```

## Admin Account

- Seeded at first startup from `ADMIN_USERNAME` / `ADMIN_PASSWORD` env vars
- Cannot be deleted via API
- Cannot have role changed
- Cannot be edited by managers

## Migration Path

Adding a new role requires:
1. Update `Role` enum in `auth.rs`
2. Add permission methods
3. Update frontend `canControl*` helpers
4. Update route guard middleware
