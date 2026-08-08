Status: completed

# Observability & Logs (Audit trail + On-demand container logs + Control-plane log rotation)

## Problem Statement

OpenWorkspace Engine shares one Linux box among many developers, governed by
group-based RBAC, instance ceilings, auto-sleep, and bandwidth caps. Three
observability gaps stand in the way of operating it:

1. **No audit trail.** There is no historical record of *who did what*. When a
   container is deleted, a template is edited, or a group permission changes,
   nothing is written down. If a user's instance vanishes or a setting is
   altered, the operator cannot reconstruct how or by whom. The roadmap's
   "Audit logging" (High) and "Operations Logs page" (Medium) items are unmet;
   the `#logs` sidebar tab is an `is_admin`-gated placeholder showing only
   "Server logs."
2. **No way to see what a session printed.** A user whose container crashes, or
   a group manager helping debug a teammate's instance, cannot read the
   container's stdout/stderr. Docker already buffers the last lines on disk; we
   just never expose them. Without this, "why did my session die" is answered
   by guesswork.
3. **Unbounded control-plane logs.** The API container logs to stdout with the
   Docker default `json-file` driver — which is *unbounded*. On a long-lived
   box this grows without limit until the disk fills, violating the project's
   "every megabyte has a purpose" principle.

This is roadmap Stage 5: the "Audit logging", "Operations Logs page", and
"On-demand container logs" items.

## Solution

Three coupled, deliberately separate subsystems — the storage medium follows
the log's purpose (audit → relational DB, runtime → Docker's on-disk buffer
streamed on demand, control-plane → docker log-driver rotation):

### 1. Audit trail (persisted, queryable)

Every meaningful administrative action — auth events, instance lifecycle,
template / group / user / registry edits, admin-settings changes — is appended
to a new `audit_logs` table. Writes are **asynchronous, best-effort, and
non-blocking**: a bounded Tokio MPSC channel feeds a single writer task that
batches inserts; if the channel is full or the DB write fails, the event
degrades to a `tracing` warning instead of failing the user's action. Entries
carry actor, action, target, client IP, outcome, and (for edits) a redacted
JSONB `before → after` diff.

The `#logs` tab becomes a paginated, filterable audit viewer, gated by a new
group flag `can_view_audit_logs` (Admin/Manager on by default, User off).

### 2. On-demand container logs (streamed, never persisted to the DB)

Each instance row gains a **Logs** button that opens a panel showing the
container's stdout/stderr: the last 200 lines by default, with a "follow new
output" live toggle. The API proxies Docker's `logs` endpoint (one request,
`tail` + `follow` params): snapshot without follow, Server-Sent Events while
following. Authorization matches instance *control* (`mayControlInstance`):
owner, admin, or a group-instance holder whose target is a lower-tier member of
a shared group. Logs stay bounded on disk via per-container log-opt limits set
at creation (`max-size=5m`, `max-file=3` ≈ 15 MB per instance). Nothing is
written to the database.

### 3. Control-plane log rotation (docker log driver, zero code)

The API container's stdout logging stays untouched (12-factor). Both compose
files (prod + dev) declare `logging: { driver: json-file, options: { max-size:
10m, max-file: 3 } }` on the `api` service so Docker rotates the control-plane
logs. Log format stays human-readable; level stays `RUST_LOG`. Control-plane
logs are **not** surfaced in the UI.

## User Stories

### Audit trail — recording

1. As a **user**, I want my successful and failed logins recorded, so that I have a record of who accessed the platform and when.
2. As a **user**, I want my logout recorded, so that the session timeline is complete.
3. As an **operator**, I want instance create / start / stop / delete / restart and auto-sleep events recorded, so that I can reconstruct the full lifecycle of any session.
4. As an **operator**, I want template create / edit / delete events recorded, so that I know who changed which workload profile and when.
5. As an **operator**, I want group create / edit / delete and membership-change events recorded, so that permission changes are traceable.
6. As an **operator**, I want user create / edit / delete and password-change events recorded, so that account lifecycles are auditable.
7. As an **operator**, I want admin-settings changes recorded, so that a box-level configuration change is attributable.
8. As an **operator**, I want registry URL changes recorded, so that image-registry changes are attributable.
9. As an **operator**, I want an authenticated user's forbidden-access attempt (403) recorded, so that privilege-escalation attempts leave evidence.
10. As an **operator**, I do NOT want anonymous scanner traffic recorded, so that bot noise does not drown out real evidence or bloat the table.
11. As an **operator**, I want system-triggered events (e.g. auto-sleep) recorded with a system actor, so that unattended actions are still on the record.
12. As an **operator**, I want the audit write to never block the operation that triggered it, so that a slow DB never breaks "stop container" or "delete template".
13. As an **operator**, I want the audit write to degrade to a server-side log when the queue is full or the DB write fails, so that an audit outage never takes down the main flow.

### Audit trail — viewing

14. As a **manager**, I want to open the Logs tab without being an admin, so that I can operate the box with my existing manager role (gated by the new flag).
15. As an **admin**, I want the Logs tab hidden from users without the flag, so that audit data (who did what, IPs, permission changes) is never leaked to tenants.
16. As a **viewer**, I want to filter entries by event type, actor, target, outcome, and date range, so that I can narrow a 90-day history to the question I'm asking.
17. As a **viewer**, I want cursor-based pagination (newest first, 50 per page), so that a long history can be walked without dropped or duplicated rows even when entries share a timestamp.
18. As a **viewer**, I want each entry to show actor (name), action, target, outcome, timestamp, and client IP, so that the who/what/when/where of an event is visible at a glance.
19. As a **viewer**, I want edit events to show which fields changed and their before/after values, so that I can see exactly what a template or settings edit altered.
20. As a **viewer**, I want sensitive fields (passwords, secrets, tokens, keys, credentials) shown as `[REDACTED]`, so that the audit table never becomes a credential leak.
21. As an **operator**, I want the client IP recorded as the real caller, not the reverse proxy, so that the IP column has audit value.

### On-demand container logs

22. As an **instance owner**, I want a Logs button on my instance that shows the last 200 lines of its stdout/stderr, so that I can debug a crash or a hung process.
23. As an **instance owner**, I want a follow toggle that streams new output live, so that I can watch a build or a script progress.
24. As an **admin**, I want to view any instance's logs, so that I can debug any session on the box.
25. As a **group-instance manager**, I want to view logs of a lower-tier member's instance in my group, so that I can debug a session I am entitled to control.
26. As a **user**, I want the logs panel to render ANSI color codes as colors instead of escape-code garbage, so that tool output is readable.
27. As a **user**, I want the logs panel to close cleanly (stop streaming) when I close it or leave the page, so that I do not leave dangling connections.
28. As a **user**, I want the panel to show a clear "session ended" state with the reason (stopped / paused / deleted / end of output) instead of spinning forever, so that I am not left hanging.
29. As a **user**, I want to open logs on a stopped instance and immediately see its last output plus the ended state, so that a dead session is still debuggable.
30. As an **operator**, I want instance logs bounded on disk (~15 MB max), so that "log viewing on demand" can never fill the box.

### Control-plane log rotation

31. As an **operator**, I want the API container's logs rotated by Docker (10 MB × 3), so that control-plane logging never fills the disk.
32. As an **operator**, I want log level still controlled by `RUST_LOG`, so that verbosity stays a deployment-time knob.
33. As an **operator**, I want control-plane logs readable via `docker logs`, so that ops needs no new tooling.

## Implementation Decisions

### 1. `audit_logs` table + flag column (migration `000024`)
- New table `audit_logs`: `id UUID PK`, `created_at TIMESTAMPTZ NOT NULL`,
  `actor_user_id UUID NULL`, `actor_name TEXT NOT NULL` (snapshot; `"system"`
  for system-triggered events), `action TEXT NOT NULL`, `target_type TEXT NULL`,
  `target_id TEXT NULL` (string form of the UUID where applicable),
  `target_name TEXT NULL` (snapshot), `outcome TEXT NOT NULL` (`success` |
  `failure`), `client_ip TEXT NULL`, `detail JSONB NULL` (changed-field
  before/after). Index on `(created_at, id)` for the keyset cursor and on
  `action` for filtering.
- Add `groups.can_view_audit_logs BOOLEAN NOT NULL DEFAULT FALSE`, backfilled
  TRUE for the Admin and Manager system groups by `kind` (mirrors migration
  `000022`). Admin groups carry it (the "admin carries every flag" rule);
  custom groups default FALSE.

### 2. RBAC plumbing (mirror the `can_view_monitoring` precedent end to end)
- `EffectiveContext` (API) and the web `EffectiveContext` / `Group` types gain
  `can_view_audit_logs`; the API's serialized `/auth/me` envelope is part of
  the public contract, so the new field flows through automatically.
- `AuthUser::can_view_audit_logs()` mirrors `can_view_monitoring()`.
- Group create/update persists the flag; the group editor UI gains a checkbox;
  Admin pinned on, User pinned off, Manager editable (defaulted on).
- Web `permissions.ts` gains `mayViewAuditLogs(ctx)`; the Logs sidebar item and
  tab swap `$isAdmin` for `mayViewAuditLogs`.

### 3. Audit event model
- Action vocabulary (stable string values, documented in the OpenAPI spec):
  `auth.login`, `auth.logout`, `auth.login_failure`, `auth.forbidden` (an
  authenticated request rejected 403), `instance.create`, `instance.start`,
  `instance.stop`, `instance.delete`, `instance.restart`,
  `instance.auto_sleep`, `instance.pause`, `instance.unpause`,
  `template.create`, `template.update`,
  `template.delete`, `group.create`, `group.update`, `group.delete`,
  `group.membership_change`, `user.create`, `user.update`, `user.delete`,
  `user.password_change`, `settings.update`, `registry.update`.
- Target types: `instance` | `template` | `group` | `user` | `registry` |
  `settings` | `none`.
- The audit hook lives at the **route-handler boundary** (highest common point):
  a small helper takes the request context + event description and enqueues it.
  It is called explicitly by handlers for the events above — not a blanket
  middleware — so read traffic never audits.
- `auth.forbidden` is emitted only for **authenticated** 403s; anonymous 401/403
  scanner noise is not audited (best-effort guard: the helper is invoked from
  the handful of authenticated routes that can reject 403, and from a thin
  handler wrapper that checks `AuthUser` presence before recording).
- Password-change and registry credential fields are still recorded as events,
  but their `detail` values are redacted (below).

### 4. Redacted before/after diff (pure function)
- Edit events record `detail` as a JSON object of `field: { before, after }`
  for the changed fields only — never a whole-row snapshot.
- A pure `redact_detail` function walks the diff and replaces any value whose
  field name matches `password` | `secret` | `token` | `key` | `credential`
  (case-insensitive, substring) with `"[REDACTED]"`. Redaction is built into
  the diff helper, so no caller can forget it. A second pure helper,
  `redact_url_userinfo`, strips embedded `user:pass@` userinfo from URL-valued
  fields (`registry_url`, template `docker_registry`) that a field-name keyword
  would miss.

### 5. Client IP extraction (pure function)
- A pure `client_ip(headers) -> Option<String>` reads `X-Forwarded-For` and
  returns the **rightmost** non-empty address (the last hop Traefik appended),
  falling back to `X-Real-IP`, then `null`. Rightmost (not leftmost) is chosen
  so the value stays the true client even if Traefik were ever configured to
  pass through a client-supplied header. Absent headers → `null` plus a single
  `tracing::debug!`.

### 6. Async audit channel (non-blocking, best-effort)
- `AppState` gains a `tokio::sync::mpsc::Sender<AuditEvent>` (bounded,
  capacity 1024). `main.rs` spawns one writer task that drains the channel and
  inserts via the repository in batches (50 events or 500 ms, whichever first).
- Channel full → `tracing::warn!` and drop the event (never await / block the
  handler). DB insert failure → bounded retry with backoff
  (`AUDIT_WRITER_MAX_RETRIES`, default 4) then `tracing::error!` and drop.
  MPSC single-consumer guarantees event ordering. Audit is explicitly
  best-effort under extreme load; the stderr fallback keeps evidence above
  zero.
- The writer also drains + flushes on a graceful-shutdown watch signal, so the
  final batch is not lost when the API stops.

### 7. `AuditLogRepository` (new repository, existing pattern)
- `insert_batch(events)`, `query(cursor, filters, limit) -> (entries, next_cursor)`,
  `prune_older_than(created_at_before) -> u64` (rows deleted). Follows the
  `WorkspaceInstanceRepository` style (sea-orm, method-per-query).
- Keyset pagination: `ORDER BY created_at DESC, id DESC`, cursor condition
  `(created_at, id) < ($1, $2)` (row-value comparison), so equal timestamps
  within a batch never split pages.
- Prune: daily, in the existing `health_worker` 3-second tick (a
  `due_for_prune(last_prune_at, now)` pure gate, like the monitor's
  every-5th-tick trigger). Retention from `AUDIT_RETENTION_DAYS` (default 90);
  no UI setting.

### 8. Audit query endpoint + Logs panel
- New endpoint family under the existing authenticated route set (single source
  of truth for payloads = the generated OpenAPI spec): a query endpoint gated
  by `can_view_audit_logs()` (403 otherwise) accepting optional filters
  (`action`, actor name substring, target name substring, `outcome`, date
  range) plus `cursor` / `limit` (default 50), returning entries + `next_cursor`.
- Frontend `LogsPanel` (replaces the placeholder): filter bar (event-type
  select, actor / target text inputs, outcome select, date range), newest-first
  list, "load more" using the cursor. Rows render actor, action, target,
  outcome, IP, time; edit events expand to show the redacted diff.

### 9. `DockerService::container_logs()` (the one new backend seam)
- New trait method: `async fn container_logs(&self, container_id: &str, tail:
  u64, follow: bool) -> Result<ContainerLogStream, ContainerLogsError>`.
  `ContainerLogStream` abstracts Docker's stdout/stderr log line stream (with
  stderr lines tagged) so the endpoint can render both. Real `DockerClient`
  impl: bollard `logs` with `tail` and `follow` (`stdout` + `stderr`); errors
  distinguish `ContainerNotFound` (deleted since last pass) from other failures.
- The existing `#[mockall::automock]` seam gains the method; mocks return canned
  line sequences, so the whole endpoint + SSE is exercised through the HTTP
  stack without Docker (extends `instances_mock_test.rs`).

### 10. Container logs endpoint (status-aware SSE)
- New endpoint `GET /api/instances/{id}/logs?tail=200&follow=true` (default
  `tail=200`, `follow=true`), authorization = `mayControlInstance` scope
  (owner / admin / group-instance holder on a lower-tier same-group target).
- Behavior by instance status:
  - `running` → emit the last `tail` lines, then if `follow` stream new output.
  - `stopped` / `paused` → emit the last `tail` lines, then immediately send an
    `end` event (`reason: stopped | paused`) and close.
  - `starting` → emit tail then follow (output is coming).
  - Container gone (logs 404) → `end` event (`reason: deleted`) and close.
- Streaming semantics: response is text/event-stream; when the client
  disconnects, Axum drops the response future and the underlying Docker stream
  is dropped with it (no lingering connections). The API also actively breaks
  the follow when it observes the instance leave `running` (stop / auto-sleep /
  delete) — translating the end of the Docker stream into an `end` event with
  the reason. No heartbeat or idle timer is needed: stream end == release.

### 11. Instance container log bounds (config-level)
- `ContainerConfig` gains log options; instance containers are created with
  `--log-driver json-file --log-opt max-size=5m --log-opt max-file=3` (≈15 MB
  per instance). The config builder is pure and unit-tested; an integration
  test asserts a created container actually carries the log options.

### 12. Control-plane log rotation (compose-only)
- Prod (`docker/openworkspace/docker-compose.yml`) adds `logging: { driver:
  json-file, options: { max-size: 10m, max-file: 3 } }` to the `api` service.
  The dev stack runs the API on the host (cargo), not in a container, so
  rotation there is the operator's terminal / journal's concern — no compose
  `logging` block applies. No Rust code change.
- Log format and level untouched (text, `RUST_LOG`); control-plane logs are not
  surfaced in the UI.

### 13. Frontend logs module + panel (no new dependencies)
- New pure module `src/lib/logs/ansi.ts`: a small SGR parser converting common
  ANSI escape sequences (foreground/background color, bold, reset, line
  controls) into HTML `span` markup; unknown sequences stripped. DOM-free and
  unit-tested, mirroring the `src/lib/chart/` style. No `ansi-to-html`, no
  `xterm.js`.
- Per-instance `Logs` button opens a `ContainerLogPanel` (modal/inline) that
  fetches the tail and follows via the SSE endpoint using the native `fetch`
  streaming reader; follow toggle; closing the panel aborts the reader (which
  cancels the SSE upstream). ANSI output rendered through `ansi.ts`.
- The `Logs` button visibility mirrors the endpoint's authorization
  (`mayControlInstance`-equivalent helper).

## Testing Decisions

A good test asserts externally observable behavior — the returned rows, the
pagination cursor, the gating decision, the SSE event sequence, the rendered
panel — never internal storage or channel internals.

- **`redact_detail` + `client_ip` (pure unit tests)**: redaction by field-name
  keyword (password/secret/token/key/credential, case-insensitive), untouched
  plain fields, nested/compound names; IP returns the rightmost
  `X-Forwarded-For` entry, falls back to `X-Real-IP`, and yields `None` for
  absent headers. Mirrors the pure unit tests of `host_port.rs` /
  `network_qos.rs`.
- **`AuditLogRepository` (DB-backed integration tests)**: batch insert persists
  entries in order; keyset pagination returns 50 per page with no overlap and
  no gaps across a same-timestamp batch; filters (action / actor / target /
  outcome / date range) narrow correctly; `prune_older_than` removes only old
  rows. Uses the existing test Postgres harness (`tests/common/pg.rs`).
- **Audit channel (unit)**: enqueue on a full channel does not block and the
  event is dropped (or logged); the writer task batches by count and by time;
  ordering is preserved.
- **Audit endpoints (integration, mock docker not needed)**: query returns 200
  for admin and for a manager whose group carries the flag, 403 for a user
  without it, 401 unauthenticated; filters and cursor round-trip. Follows the
  existing auth / `instances_mock_test.rs` integration patterns.
- **RBAC**: effective-context OR across groups includes the new flag; admin
  carries it; `AuthUser::can_view_audit_logs` mirrors the existing
  `can_view_monitoring` tests; migration backfills Admin/Manager on, User off;
  group editor round-trips the flag.
- **`DockerService::container_logs` (mock seam)**: the endpoint returns the
  tail snapshot; with `follow` it streams the mocked lines and terminates with
  an `end` event; stopped/paused instances end immediately with the right
  reason; a gone container ends with `deleted`; authorization matches
  `mayControlInstance` (owner yes, stranger no, group-instance holder yes on a
  lower-tier same-group target); SSE cancels on disconnect. Real Docker
  behavior is exercised by the existing docker integration tests plus a
  `docker_lifecycle_test`-style assertion that a created container carries the
  5m×3 log options.
- **Web (vitest)**: `ansi.ts` converts known sequences and strips unknown ones;
  `LogsPanel` renders rows, applies filters, paginates via cursor; the
  container log panel renders tail lines, shows the ended state with reason,
  and aborts the stream on close; `mayViewAuditLogs` in the permissions /
  rbac-actions tests; group editor checkbox round-trips the flag.
- **Gates**: `bash scripts/check.sh` silent (both feature sets), web `pnpm
  check`, full `run_tests.sh` (docker feature) and web `pnpm test` green; the
  security fuzzer (`pnpm run security:api`) re-run against a dev stack once the
  audit + logs endpoints land, with the new endpoints in the safe (RBAC-gated)
  surface.

## Out of Scope

- Surfacing control-plane logs in the UI, or switching them to structured JSON.
- `logrotate` / systemd-journal handling outside Docker's json-file rotation.
- Clearing / resetting container logs when an instance changes owner.
- Login-failure lockout and rate limiting (separate roadmap item — this feature
  only *records* failures and authenticated 403s).
- Audit export, webhook/notification, or anomaly alerts.
- Container log viewing for anonymous or unauthenticated users.
- Multi-host / distributed audit aggregation.
- Per-group/user resource quotas (separate roadmap item).

## Further Notes

- **Endpoints**: the query and container-log endpoints join the fuzz surface as
  RBAC-gated safe endpoints (18 fuzzable paths → 20); the OpenAPI spec is
  regenerated and remains the single source of truth for payload shapes. The
  audit action vocabulary is enumerated in the spec's `AuditAction` schema.
- **Two new docs touchpoints on delivery**: `docs/user-guide/rbac.md` (new
  flag), `docs/user-guide/frontend.md` (Logs tab, per-instance log panel);
  `roadmap.md` (Stage 5 items → completed); `CHANGELOG.md` (user-visible
  change). The `docs/user-guide` API-ban applies — point readers at the OpenAPI
  spec rather than enumerating endpoints.
- **Memory/disk budget**: audit inserts are batched (bounded channel), the
  audit table is pruned daily, instance logs are bounded at ~15 MB each by
  log-opt at creation, and control-plane logs are bounded by compose log-opt —
  all within the project's lightweight stance.
- **Best-effort honesty**: extreme-concurrency audit loss is accepted by
  design (stderr fallback keeps it near zero); this is a deliberate trade-off
  against ever blocking a main-path operation.
