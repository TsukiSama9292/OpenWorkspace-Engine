Status: completed

# Resource & Instance Quota Management

## Problem Statement

Workspace instances run untrusted developer workloads on a shared host. Today there is no upper bound on how many instances can run or how much of the host's CPU/RAM a single user (or all users together) can claim. A user can launch an unbounded number of containers and drive the host into OOM or CPU starvation, taking down every other workspace and the control plane itself. Templates already carry a per-container CPU (`cores`) and RAM (`memory`) limit, but nothing accounts for those limits in aggregate.

Two allocation philosophies are needed: heavy workloads (GPU training, big compiles) want a **hard reservation** that is guaranteed and locked while active, while light dev environments (web terminals, Jupyter) want the elasticity of **overcommit**, where per-container cgroups limits are the safety net and no static host budget is deducted.

## Solution

Add a multi-layer quota control plane to the Rust API. A new **pre-flight validation pipeline** runs before any instance is activated (new launch or restart of a stopped instance) and enforces, in order: per-user instance count, global host instance count, per-user CPU/RAM, host **dedicated** resource pool (dedicated-mode templates only), and an optional host **shared** resource fuse (shared-mode templates only). Rejections are fail-fast — they happen **before any database record is created** and return `409 Conflict` with a machine-readable quota body.

Templates gain an `allocation_mode` label (`shared` / `dedicated`). **Dedicated** instances statically deduct their CPU/RAM from the host's total allocatable capacity and hold that reservation while active (running, starting, or paused); **shared** instances deduct nothing at the host level and rely on cgroups limits, enabling overcommit. Per-user quotas (instance count, CPU, RAM) apply to both modes, with per-role defaults and optional per-user overrides. Host capacity and global limits are configured in a new single-row `system_settings` table — auto-seeded from the host via Docker when absent, overridable by environment variables, and editable by Admins through a new admin settings API.

Checks and reservations are made atomic with SeaORM transactions plus row locks (global singleton row, then the user row), so concurrent launches cannot overshoot a limit. A `shared` template can never hold a host reservation, and changing a template's mode while it has active instances is forbidden, so accounting stays consistent.

## User Stories

1. As an **instance user**, I want to be told when I have reached my personal instance limit, so that I know why my launch was refused instead of seeing a generic error.
2. As an **instance user**, I want my personal CPU quota enforced across all my active instances (dedicated and shared combined), so that one user cannot monopolize the host through many small containers.
3. As an **instance user**, I want my personal RAM quota enforced across all my active instances, so that my total requested memory can never exceed my allowance.
4. As an **instance user**, I want my quota rejection to be accompanied by the current usage and the limit, so that I can act on it (stop an instance, or ask an admin for a raise).
5. As an **instance user**, I want to launch a shared-mode template freely (it does not consume host dedicated capacity), so that lightweight dev environments keep their overcommit elasticity.
6. As an **instance user**, I want the same quota checks to run when I restart a stopped instance, so that a restart cannot bypass the limits that a fresh launch would hit.
7. As an **instance user**, I want unpausing to require no new quota check, so that pausing and resuming my session is instant.
8. As an **instance user**, I want a failed launch (infrastructure error after the quota check passed) to leave a visible `error` instance, so that I can see what happened and retry.
9. As a **workspace manager**, I want per-role default quotas (instance count, CPU, RAM), so that I get sane enforcement without configuring every user.
10. As a **workspace manager**, I want to override a specific user's quota while keeping `NULL` as "use the role default", so that I can give a trusted user a higher allowance without a special role.
11. As a **workspace manager**, I want the role defaults to be: user = 2 instances / 4 cores / 8 GiB, manager = 5 instances / 12 cores / 32 GiB, admin = exempt, so that the initial deployment is safe out of the box.
12. As a **workspace manager**, I want an Admin to be exempt from *personal* limits but still counted against *global* host limits, so that the physical host capacity cannot be bypassed by role.
13. As a **workspace manager**, I want a Manager to get no exemption — just higher default quotas — so that enforcement semantics stay predictable.
14. As a **workspace manager**, I want to set an optional global host instance limit, with `0` meaning unlimited, so that I can cap the number of live containers on the box.
15. As a **workspace manager**, I want the global instance limit (when `0`) to simply skip its check, so that a fresh install behaves exactly as it does today.
16. As a **workspace manager**, I want a template's `allocation_mode` to be visible and editable, so that I can mark heavy workloads as dedicated and light ones as shared.
17. As a **workspace manager**, I want only Admins to be able to set a template to `dedicated`, so that hard host reservations are a high-privilege action.
18. As a **workspace manager**, I want a Manager creating/editing templates to be constrained to `shared`, so that non-admin managers cannot lock host capacity.
19. As a **workspace manager**, I want to be blocked (`409`) from changing a template's `allocation_mode` while it has active instances, so that per-mode accounting can never be inconsistent.
20. As a **workspace manager**, I want existing templates to migrate to `shared` by default, so that upgrading the platform does not suddenly lock host capacity.
21. As a **workspace manager**, I want to set an optional host shared-CPU / shared-RAM fuse (default `0` = off), so that I can stop the sum of shared-mode limits from exceeding physical RAM on a single box.
22. As an **infrastructure admin**, I want host capacity (total CPU cores, total RAM) auto-detected from the host via Docker at first startup, so that I don't have to enter numbers manually.
23. As an **infrastructure admin**, I want environment variables to override the auto-detected capacity, so that I can reserve headroom for the OS and control-plane services without changing code.
24. As an **infrastructure admin**, I want to edit host capacity and global limits later through an admin settings API, so that I can tune the box after hardware changes.
25. As an **infrastructure admin**, I want a failed capacity auto-detect to log a warning and fall back to env values or conservative defaults, so that the API still boots.
26. As an **API consumer**, I want a consistent `409 Conflict` for every quota or count rejection, so that I need only one error branch on the client.
27. As an **API consumer**, I want each quota rejection body to carry a structured `quota` object (`scope`, `current`, `limit`, `requested`), so that the UI can render the reason without parsing prose.
28. As an **API consumer**, I want the admin settings endpoint to expose `max_cpu_cores`, `max_ram_bytes`, `host_instance_limit`, `shared_max_cpu`, and `shared_max_ram`, so that I can audit and change host policy programmatically.
29. As an **API consumer**, I want template create/update/list/single responses to include `allocation_mode`, so that clients know each template's reservation behavior.
30. As an **API consumer**, I want the user update API to accept per-user quota overrides (and `NULL` to restore role defaults), so that quotas can be managed programmatically.
31. As an **instance user**, I want a paused instance to keep holding my quota and the host dedicated reservation, so that resuming never fails due to a race with another launcher.
32. As an **instance user**, I want a stopped or deleted instance to release its instance-count slot and my personal resource quota, so that I can reuse my allowance.
33. As an **instance user**, I want a stopped or deleted dedicated instance to release the host dedicated reservation immediately, so that the capacity is available to others.
34. As a **platform developer**, I want the pre-flight decision logic to be pure functions, so that it is unit-testable without a database or Docker.
35. As a **platform developer**, I want the check-and-reserve step to run inside a transaction that locks the global settings row then the user row, so that concurrent launches cannot overshoot any limit.
36. As a **platform developer**, I want the quota-resolution logic to be a single function so that a future Group tier can slot in as one more fallback level.
37. As a **platform developer**, I want the `system_settings` singleton row to double as both the host-config store and the global lock target, so that global accounting reads are consistent with the lock.
38. As a **platform developer**, I want the reservation written as `starting` status inside the transaction before the slow Docker build runs, so that in-flight activations are visible to subsequent checks.
39. As an **instance user**, I want my launch to proceed normally through the existing one-click "create and start" experience, so that the new checks do not add a step.
40. As a **workspace manager**, I want GPU count to remain entirely unaccounted by quotas in this phase, so that the CPU/RAM accounting stays correct without a half-built GPU scheme.

## Implementation Decisions

### 1. Activation scope and fail-fast semantics

- The pre-flight pipeline runs on **both** activation paths: launching a new instance (`POST /api/instances`) and restarting a stopped one (`POST /api/instances/{id}/start`).
- It does **not** run on unpause: a paused instance already holds its count and resource reservations, so unpausing consumes nothing new.
- Quota/count rejection happens **before** any DB record is created (fail-fast): the request returns `409 Conflict` and leaves no row behind.
- Only infrastructure failures *after* a successful check and DB reservation leave an `error` record: on launch failure the instance is marked `error` (record kept); on restart failure it is rolled back to `stopped` so the user can retry.

### 2. Active / inactive status sets

- **Active** (counted toward instance counts and resource sums): `running`, `starting`, `paused`.
  - `starting` is included even though the health probe (up to 120s) has not confirmed it, because the container is already alive and consuming resources on the host. Including it removes the overcommit window and makes "reserve on `starting` write" exact.
- **Inactive** (never counted): `stopped`, `error`.

### 3. Concurrency: transactional check-and-reserve with row locks

- Both activation paths run an atomic sequence in a SeaORM transaction:
  1. `begin`
  2. `select_for_update` the global singleton `system_settings` row (id = 1)
  3. `select_for_update` the user's `users` row
  4. Gather counters and sums (inside the same transaction)
  5. Run the full pre-flight pipeline
  6. On violation: `rollback`, return `409` with a quota body
  7. Otherwise reserve by creating/updating the instance row with status `starting`, then `commit`
  8. Only after commit does the handler call the (slow) Docker API to build/start the container
- **Lock hierarchy (deadlock prevention):** locks are always acquired in ownership-chain order — global row first, then the user row. The reverse order is forbidden. This ordering is abstracted as "ancestors before descendants along the ownership chain", so a future Group tier inserts between global and user without changing the rule.

### 4. Global singleton `system_settings` row

- New single-row table `system_settings` (id = 1) stores host capacity and global policy:
  - `max_cpu_cores` (host total CPU)
  - `max_ram_bytes` (host total RAM)
  - `host_instance_limit` (global active-instance cap; `0` = unlimited/skip)
  - `shared_max_cpu`, `shared_max_ram` (host shared-mode fuse; `0` = off)
- The same row is the global lock target from Decision 3, so global counters are read under the lock and are always consistent.

### 5. Host capacity provisioning

- At startup, if the `system_settings` row does not exist: auto-detect host capacity via `docker info` (host `NCPU` / `MemTotal` — the host, not the API container), then seed it into the row.
- Environment variables `OW_HOST_CPU_CORES` / `OW_HOST_RAM_BYTES` override the detected values.
- If detection fails: log a `WARN`, fall back to env values, then to conservative defaults (8 cores / 16 GiB). The API must still boot.
- At runtime, an Admin can edit all values through the admin settings API; edits persist in the row. No hardcoded OS-reserve magic numbers — an admin who wants headroom sets the capacity conservatively.

### 6. Per-user quotas (User columns + Role defaults, Group-ready)

- `users` gains three nullable columns: `instance_limit`, `max_cpu_cores`, `max_ram_bytes`.
  - `NULL` = inherit the role default (the per-user value is a *personal override*, never an absolute).
- Role default quota table is a code constant:
  - `user`: 2 instances / 4 cores / 8 GiB
  - `manager`: 5 instances / 12 cores / 32 GiB
  - `admin`: exempt from all personal-level checks
- Admin edits a user's overrides via the existing user update API; passing `NULL` restores the role default.
- **Group extensibility seams (phase 2):** (a) effective-quota resolution is one function (`resolve_effective_quota(override, role) → quota`), so a Group tier is one more fallback level; (b) the nullable-override semantics survive adding `group_id`; (c) the lock ordering is expressed as an ownership chain.

### 7. Admin exemption boundary

- **Personal level (Admin exempt):** per-user instance limit, per-user CPU quota, per-user RAM quota — skipped entirely for Admin.
- **Global level (no exemption, including Admin):** host instance limit, host dedicated pool, host shared fuse. All roles' instances — Admin's included — count toward global active totals and the dedicated/shared sums.

### 8. Pre-flight pipeline (executed in order; `409` on first failure)

1. **Per-user instance count** (skipped for Admin): `(active instances owned by user) + 1 ≤ effective instance_limit`.
2. **Global host instance count** (skipped when `host_instance_limit = 0`): `(all active instances) + 1 ≤ host_instance_limit`.
3. **Per-user resources** (skipped for Admin): `(user's active CPU sum + template cores) ≤ effective cpu quota` and the same for RAM. Counts both dedicated and shared instances.
4. **Host dedicated pool** (only when the template is `dedicated`): `(all active dedicated CPU + template cores) ≤ max_cpu_cores` and the same for RAM, where "all active dedicated" is the sum over active instances whose template is `dedicated`.
5. **Host shared fuse** (only when the template is `shared` and the fuse is enabled, i.e. `shared_max_* > 0`): `(all active shared CPU + template cores) ≤ shared_max_cpu` and the same for RAM.

- Every check reports a distinct `QuotaScope` on failure (see Decision 10), and the checks run in this fixed order so the first hit is deterministic.

### 9. Template `allocation_mode`

- `workspace_templates` gains `allocation_mode` (`shared` | `dedicated`); the migration backfills existing templates with `shared`.
- Only **Admin** may create or update a template with `allocation_mode = dedicated`; Managers are restricted to `shared` (`403 Forbidden` otherwise). Existing `can_manage_templates` (admin + manager) is narrowed for the dedicated value only.
- Editing a template's `allocation_mode` is rejected with `409` while that template has any active instance (running / starting / paused). The admin must stop/remove active instances or clone a new template first.
- The instance row does not snapshot the mode; because the mode cannot change while instances are active, reading the template's current mode at check time is always consistent. A restart re-runs the pipeline against the current mode.

### 10. Rejection contract

- All quota / count rejections return `409 Conflict`; permission violations (e.g. Manager setting `dedicated`) return `403 Forbidden`.
- Rejection body:
  ```json
  {
    "error": "Per-user instance limit reached (active: 2, limit: 2)",
    "quota": {
      "scope": "user_instance",
      "current": 2,
      "limit": 2,
      "requested": 1
    }
  }
  ```
- `scope` enum: `user_instance`, `user_cpu`, `user_ram`, `host_instance`, `host_dedicated_cpu`, `host_dedicated_ram`, `host_shared_cpu`, `host_shared_ram`.
- The `quota` field is present only on quota rejections; all other errors keep the existing `{ "error": ... }` shape.

### 11. Admin settings API

- New admin-only endpoints `GET /api/admin/settings` and `PUT /api/admin/settings` exposing `max_cpu_cores`, `max_ram_bytes`, `host_instance_limit`, `shared_max_cpu`, `shared_max_ram`. Reads and writes the `system_settings` row (id = 1, upserted on demand).

### 12. Web UI (phase 1)

- Template form: `allocation_mode` selector (`shared` / `dedicated`). The `dedicated` option is rendered only for Admins; Managers see only `shared`.
- Admin user management: quota override fields (`instance_limit`, `max_cpu_cores`, `max_ram_bytes`); leaving a field empty sends `NULL` (restore role default).
- New Admin system settings page wired to the admin settings API.
- 409 handling: the client catches the quota rejection body and shows a structured error toast/modal. No quota-usage dashboard is built in this phase.

### 13. Module shape (seams)

- A new **quota policy module** holds the pure decision logic: `AllocationMode`, `Quota`, `QuotaScope`, `QuotaViolation` types; `resolve_effective_quota(override, role)`; and a single `check(...)` function that takes the counters, the template resources, the effective quota, the host capacity, the allocation mode, and the fuse, and returns `Result<(), QuotaViolation>`. No DB or Docker access. This is the deep module: all comparison and ordering behavior sits behind a small interface.
- A **DB layer** (repositories) provides the counter/sum queries (active per user, active total, active dedicated sum, active shared sum) and the `system_settings` single-row read/upsert, used inside the transaction.
- A **transactional activation helper** shared by both activation routes wraps the begin → lock global → lock user → check → reserve-as-`starting` → commit sequence; the routes then continue with the existing Docker build path.
- The `DockerService` trait is the existing seam used after the reservation; the check itself is independent of it.

## Testing Decisions

- **Only external behavior is tested**: a rejected request must return `409` with the right `scope` and numbers and must leave **no** instance row behind; an accepted launch must reserve and then build. Tests do not assert on internal helper call counts.
- **Quota policy module — unit tests, no DB/Docker** (prior art: the pure logic in `network_qos.rs`). Cover every branch of the pipeline: each scope's threshold boundary (at-limit vs over-limit), Admin personal-level bypass, `host_instance_limit = 0` skip, dedicated-only pool check, shared fuse on/off, and the fixed check ordering.
- **Quota resolution — unit tests**: role defaults for user/manager, admin exemption, per-user override precedence, and `NULL` = inherit.
- **`allocation_mode` template rules — API integration tests** (prior art: `templates_test.rs`): migration backfill default `shared`; Manager `dedicated` → `403`; mode edit blocked by an active instance → `409`; allowed when no active instances.
- **Transactional check-and-reserve — integration tests against the Postgres test container** (prior art: `tests/common/pg.rs` pattern and `instances_mock_test.rs` with a mocked `DockerService`): two concurrent launches from the same user at the instance limit → exactly one succeeds; two concurrent launches from different users at the global limit → exactly one succeeds; per-user serialization holds.
- **Route-level behavior — `instances_mock_test.rs`-style tests with a mocked DockerService**: rejection produces `409` + quota body and no DB row; launch infra-failure produces an `error` record; restart failure returns the instance to `stopped`.
- **Lifecycle accounting — integration tests**: pause keeps the reservation, stop/delete releases count and personal quota, deleting/stopping a dedicated instance releases the host pool (asserted via the counter queries).
- **Frontend — Vitest**: `template-form.test.ts` / `template-panel.test.ts` for `allocation_mode` round-tripping and the admin-only `dedicated` option; new tests for the user quota fields and the admin settings page; a test that a 409 quota body renders the structured toast.

## Out of Scope

- **GPU quotas**: no GPU capacity columns, no GPU pre-flight checks, no GPU UI. GPU accounting (VRAM isolation, NVIDIA container runtime integration) is a separate phase-2 ticket. `gpu_count > 0` templates are not forced into `dedicated` mode.
- **Group/team quotas**: a `groups` table, `users.group_id`, group-level quotas, and group management UI are phase 2. This spec only keeps the three seams (single resolution function, nullable-override semantics, ownership-chain lock ordering) that make Group a drop-in tier.
- **Quota usage dashboard**: no "remaining quota" display or dedicated usage endpoint in this phase; the client shows only the 409 rejection toast/modal.
- **Dynamic host-capacity reconciliation** (e.g. detecting a container that died outside the API and correcting a stale `running` status): the existing health/keep-time workers remain the only reclaim mechanisms.

## Further Notes

- Resource units follow the existing template model: `cores` is whole CPU cores (`i32`), `memory` is bytes (`i64`). Quota columns mirror those units (RAM quotas in bytes).
- The env `Settings` remain the source only for *seeding* `system_settings`; the row is the runtime source of truth, read under the global lock during checks.
- A `shared` template can never hold a host dedicated reservation; its CPU/RAM still count toward the user's personal quota and the shared fuse. This mirrors the PRD's intent that per-user quotas bound each account while the host-level dedicated pool guarantees heavy workloads.
- The feature is expected to ship behind the existing single-API-replica deployment; the DB row-lock mechanism was chosen specifically so that scaling to multiple replicas later requires no change to the check logic.
