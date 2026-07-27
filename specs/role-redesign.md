# Role System Redesign — Three-Tier RBAC

## Problem Statement

The current role system is a binary admin/user model with critical security gaps: open registration allows anyone to self-register as admin, any authenticated user can act on any instance or config regardless of ownership, and role checks are scattered string comparisons with no validation. The user needs a proper three-tier role hierarchy (admin → manager → user) with clear permission boundaries, a single immutable admin account, and enforced ownership checks throughout the system.

## Solution

Replace the existing two-role system with a three-tier role hierarchy (admin, manager, user). Admin is a single seeded immutable account. Managers inherit admin permissions except the ability to create other managers or admins. Users can only manage their own workspaces and view their own profile. All authorization gaps (open registration, missing ownership checks, unprotected Docker routes) are closed.

## User Stories

1. As an admin, I want my account to be seeded automatically at startup so that I always have access to the system.
2. As an admin, I want my account to be immutable (cannot be edited or deleted by anyone) so that the root account is always protected.
3. As an admin, I want to create manager accounts so that I can delegate administrative tasks.
4. As an admin, I want to create user accounts so that team members can access their own workspaces.
5. As an admin, I want to create user accounts with role "user" only (not admin or manager) when I am acting as manager, so that the role hierarchy is enforced.
6. As an admin, I want to edit any user's username, password, and role so that I can manage the system.
7. As an admin, I want to delete any non-admin user so that I can remove inactive or unauthorized accounts.
8. As an admin, I want to see all users in the system so that I have full visibility.
9. As an admin, I want to see all instances across all users so that I can monitor system usage.
10. As an admin, I want to manage all templates (create, edit, delete) so that I can control available workspace configurations.
11. As an admin, I want to start, stop, pause, and resume any instance so that I can manage system resources.
12. As an admin, I want to access the Docker raw routes so that I can perform advanced system operations.
13. As an admin, I want to manage the container registry so that I can control available images.
14. As a manager, I want to create user accounts so that I can onboard team members.
15. As a manager, I want to create user accounts with role "user" only (not admin or manager) so that the role hierarchy is enforced.
16. As a manager, I want to edit any user's password and role so that I can manage accounts.
17. As a manager, I want to delete any non-admin user so that I can remove inactive accounts.
18. As a manager, I want to see all users in the system so that I have visibility into the user base.
19. As a manager, I want to see all instances across all users so that I can monitor system usage.
20. As a manager, I want to manage all templates so that I can control available workspace configurations.
21. As a manager, I want to start, stop, pause, and resume any instance so that I can manage system resources.
22. As a manager, I want to access the Docker raw routes so that I can perform advanced system operations.
23. As a manager, I want to manage the container registry so that I can control available images.
24. As a user, I want to see only my own workspaces so that my view is focused and clean.
25. As a user, I want to launch new instances from available templates so that I can start working.
26. As a user, I want to start, stop, pause, and resume my own instances so that I can manage my work.
27. As a user, I want to remove my own instances so that I can clean up when done.
28. As a user, I want to VNC into my running instances so that I can access my workspace.
29. As a user, I want to view my own profile (username, role) so that I know my account details.
30. As a user, I want to change my own password so that I can maintain account security.
31. As a user, I want to see available templates in the Quick Launch section so that I can create new workspaces.
32. As a user, I want to be unable to create, edit, or delete templates so that system configurations are controlled by administrators.
33. As a user, I want to be unable to see other users' instances or accounts so that privacy is maintained.
34. As a manager, I want to be unable to create admin or manager accounts so that the role hierarchy is preserved.
35. As a manager, I want to be unable to delete the admin account so that the root account is always protected.
36. As an admin, I want role validation on all user creation and update endpoints so that arbitrary role strings cannot be injected.
37. As an admin, I want registration to be disabled so that only authorized accounts can be created through the user management interface.
38. As a user, I want instance ownership enforcement so that no one else can start, stop, or delete my instances.
39. As a user, I want config ownership enforcement so that no one else can modify or delete my configurations.
40. As an admin, I want the sidebar to show only the tabs relevant to my role so that the UI is not cluttered with unavailable features.

## Implementation Decisions

### Role Definition

Three roles as a formal Rust enum, replacing scattered string literals:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    Admin,
    Manager,
    User,
}
```

Role hierarchy for permission checks: `Admin > Manager > User`. The hierarchy is expressed as methods on the `Role` enum (e.g., `role.can_manage_users()`, `role.can_manage_templates()`).

### Admin Immutability

The admin account (seeded at startup) is protected by its role: any API endpoint that edits or deletes a user must reject operations where `target.role == "admin"` OR `target.id == admin_id`. The admin ID is stored in app state at startup for reference.

### Single Admin Enforcement

The `POST /api/users` and `PUT /api/users/{id}` endpoints validate that:
- Creating a user with `role = "admin"` is forbidden for all roles.
- Creating a user with `role = "manager"` is forbidden unless the caller is `admin`.
- The seeded admin cannot be deleted or have its role changed.

### Registration Disabled

Remove or gate the `POST /api/auth/register` endpoint. User creation is only possible through `POST /api/users` (admin/manager only).

### Ownership Checks

All instance and config mutation endpoints (start, stop, pause, unpause, delete, update) enforce that the caller is either admin/manager OR the owner of the resource. This applies to:
- `POST /api/instances/{id}/start|stop|pause|unpause`
- `DELETE /api/instances/{id}`
- `PUT /api/configs/{id}`
- `DELETE /api/configs/{id}`

### Frontend Tab Visibility

| Tab | admin | manager | user |
|-----|-------|---------|------|
| Workspaces | ✓ | ✓ | ✓ |
| Instances | ✓ | ✓ | ✗ |
| Users | ✓ | ✓ | ✗ |
| Templates | ✓ | ✓ | ✗ |
| Settings | ✓ | ✓ | ✓ |

The `isAdmin` derived store is replaced with `isManager` (admin OR manager) for tab visibility. The Users tab role dropdown offers "user" and "manager" for admin, "user" only for manager.

### User Profile for Regular Users

Regular users can view their profile (username, role) and change their own password via `PUT /api/users/{id}` where the target ID matches the caller's ID. Username and role changes are rejected for self-updates by non-admin users.

### Backend Permission Middleware

Replace scattered `auth.role != "admin"` checks with a reusable helper or extractor that encodes the role hierarchy:
- `require_admin(auth)` — only admin
- `require_manager(auth)` — admin or manager
- `require_user(auth)` — any authenticated user

### Database

No schema migration needed for the role column — it's already a free-form string. The enum validation happens at the application layer.

## Testing Decisions

### Test Philosophy

Test external behavior (API responses, status codes, DB state) rather than implementation details. Role checks are tested by asserting that endpoints return 403 for unauthorized roles and 200/201/204 for authorized roles.

### Backend Tests

- **Unit tests for Role enum**: Verify `can_manage_users()`, `can_manage_templates()`, etc. return correct booleans.
- **Integration tests for user CRUD**: Test each endpoint with admin, manager, and user tokens. Assert 403 for forbidden operations, 200 for allowed.
- **Integration tests for ownership**: Test that non-admin users cannot start/stop/delete instances they don't own.
- **Integration tests for admin immutability**: Test that admin cannot be edited or deleted by anyone.

### Frontend Tests

- **Auth store tests**: Update existing tests to cover the three-role model.
- **Tab visibility tests**: Verify sidebar shows correct tabs based on role (manual or E2E).

### Prior Art

Existing test patterns in `apps/api/tests/` and `apps/web/src/tests/` — follow the same structure and conventions.

## Out of Scope

- Role-based access control for VNC connections (ForwardAuth) — already handled by ownership via `vnc_token`.
- Promoting/demoting existing users between roles (covered by PUT endpoint, but UI flow for self-service role changes is not in scope).
- Audit logging for role changes.
- Invite-based user registration flow.
- Multi-admin support.

## Further Notes

### Critical Security Fixes Required (Pre-existing)

These gaps exist in the current codebase and should be addressed as part of this redesign:

1. **Open registration** (`POST /api/auth/register`) — must be disabled or removed.
2. **Missing ownership checks** on instance mutations — any user can start/stop/delete any instance by ID.
3. **Missing ownership checks** on config mutations — any user can update/delete any config by ID.
4. **Docker raw routes** (`/api/docker/containers/*`) — completely unprotected, should require admin/manager.
5. **User list endpoint** (`GET /api/users`) — returns all users to any authenticated user, should be admin/manager only.
6. **No role validation** — arbitrary role strings can be created via the API.

### Migration Path

- The seeded admin account remains unchanged.
- Existing "user" accounts are unaffected — they stay as role "user".
- No database migration required — the role column is already a string.
- Frontend TypeScript union type updates from `'admin' | 'user'` to `'admin' | 'manager' | 'user'`.
