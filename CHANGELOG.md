# Changelog

Chronological, user-visible changes. Append, don't rewrite history.

## [Unreleased]

### Log surfaces redesigned (Audit Logs page + Container Log modal)

Log UI Redesign (`.scratch/log-ui-redesign/`). Layout and interaction overhaul
of the two log surfaces — same dark glassmorphism + zinc/indigo look, no
backend change:

- **Audit Logs page**: the six filters now sit in an evenly aligned grid with
  the date-range fields paired; Apply / Clear and the entry count moved to
  their own right-aligned action row (the Sessions page filter bar inherits
  the same alignment). Times render compactly (`2026-08-08 15:14`) with the
  full time on hover, the caller IP column hides on narrow windows, and edit
  rows expand via a clear keyboard-operable chevron button instead of a
  hidden corner glyph.
- **Container Log modal**: follow now genuinely pins to the newest line —
  scrolling up pauses it (and the status text says so), scrolling to the
  bottom resumes it. A Wrap toggle switches between wrapped lines and
  alignment-faithful `white-space: pre` with horizontal scrolling. Lines are
  numbered with stdout (blue) / stderr (red) color edges replacing the old
  O/E letterboxes. The panel opens larger, gains a fullscreen toggle, an
  A−/A+ text-size control remembered across sessions, and truncated header
  titles.
- **Tests**: new pure-helper and component test suites for both surfaces.

### Audit trail (Logs page) and on-demand instance logs

Observability & Logs (`.scratch/archive/observability-logs/`). A queryable audit trail
of administrative and security events, plus an on-demand container-log viewer:

- **Audit trail**: every meaningful action — sign-ins and failed sign-ins,
  session lifecycle (create / start / stop / pause / resume / delete, including
  auto-sleep), template / group / user edits, registry and settings changes,
  and denied-access attempts by signed-in users — is recorded with the actor,
  action, target, outcome, caller IP, and time. Entries are kept 90 days and
  pruned automatically.
- **Logs page** (new): a new page in the UI for groups granted the new
  `can_view_audit_logs` flag (Admin and Manager on by default, User off). Filter
  by event type, actor, target, outcome, or date range; edit events expand to
  show exactly which fields changed and their before/after values, with
  sensitive values (passwords, secrets, tokens, keys, credentials) always shown
  as `[REDACTED]`.
- **On-demand instance logs**: every session card now has a Logs button (owner,
  admins, and group managers entitled to control the session) that opens the
  session's console output — last 200 lines by default, with an option to
  follow new output live. Stopped or paused sessions show the tail plus a
  clear "session ended" reason, so a dead session stays debuggable.
- **Log bounds**: instance container logs are capped (~15 MB per session),
  and control-plane logs are rotated by the compose stack, so log files cannot
  grow without limit.

### Instance containers now restart with `unless-stopped`

Instance (and raw admin-panel) containers are created with the `unless-stopped`
restart policy, so a host or Docker daemon restart brings running instances
back automatically. A deliberate stop — from the UI, the API, or auto-sleep —
still stays stopped; only a container that was running at daemon shutdown is
resumed.

### Monitor tab: interactive time-series charts and instance detail modal

Monitor Dashboard Optimization (`.scratch/archive/monitor-dashboard-optimization/`).
The 1h / 24h range toggle is gone, replaced by a single 24-hour interactive
time axis that picks the right resolution as you zoom (fine 15 s points for
the last hour, coarse 5 min averages beyond):

- **Interactive charts everywhere**: hover shows a crosshair with the exact
  value + timestamp, click pins the readout, dragging across a chart highlights
  the range with live average / max / min stats and zooms on release. The
  charts start with the right edge following "now"; zooming or panning
  disengages follow and a "back to now" button re-engages it.
- **Enlarged host cards**: CPU / RAM / Disk are now full interactive charts
  (~180 px tall, 3-across) instead of small sparklines.
- **Instance detail modal**: each row's new Detail button opens the instance's
  CPU and memory as two full interactive charts, reading from the snapshot the
  panel already fetched (no extra request). Close via the overlay, the ×
  button, or Esc.
- **Row sparklines** gain a light hover tooltip and click-to-pin.
- The snapshot now returns **timestamped two-tier points** per metric
  (`*_fine` / `*_coarse`) — endpoint and gating unchanged.

### Monitor tab: readable layout and meaningful used/max memory charts

Polish pass on the Monitor tab (`.scratch/archive/monitor-dashboard/`):

- **Layout**: instance-table data cells now align left like the Instance name
  column (no more right-aligned numbers), and the type is bumped to match the
  rest of the site — bigger host-card values, larger CPU/RAM values, badges,
  and status text.
- **Meaningful memory charts**: sparklines are now scaled to a real domain
  instead of each row's own local min/max — CPU against 0–100%, instance RAM
  against the template's memory cap, and host cards against their totals — so
  "used / max" reads as a genuine fraction. The instance Memory cell shows
  `used / cap (percent)`; templates configured without a memory cap show the
  used bytes with an `(unlimited)` hint instead of a misleading "max" (the
  container's cgroup would otherwise report the host's RAM as its limit).
- The snapshot now reports the **template-configured memory cap** as the
  instance limit (`0` = unlimited) rather than the raw container cgroup limit.
- Instance **used** memory now matches `docker stats`: the reclaimable page
  cache (`inactive_file`) is excluded from the raw cgroup usage, so a browser
  session that caches ~540 MB of file pages reads as ~260 MB, not ~800 MB.
- Instance **CPU** is now shown as used/max against the template's core
  budget, in the same per-core-% unit Docker uses (`200%` = 2 cores): the
  Memory-style cell reads `18% / 200% (9%)` and the sparkline is scaled to the
  core limit instead of a hard 100% cap (which clipped multi-core usage).
  Templates without a CPU cap show the used value with an `(unlimited)` hint,
  with the sparkline scaled to the host's core count.
- A container that vanishes mid-sample (instance stopped or deleted between
  the monitor pass's list and stats read) is now logged at debug instead of
  alarm — a `No such container` 404 is expected teardown, not a fault.

### Resource Monitoring Dashboard (Monitor tab) (`.scratch/archive/monitor-dashboard/`)

The Monitor tab is now a real operator view instead of an admin-only
placeholder: host and per-instance resource usage, sampled in the background
every 15 s and held in-memory for an hour of fine-grained history plus a full
day of five-minute averages (nothing persisted).

- **Three host cards** (CPU / RAM / Disk) with the current value and a 1-hour
  sparkline — headroom at a glance before launching another instance.
- **Active Instances table** (running / starting / paused) with owner,
  template, runtime badge, uptime, and live CPU % / RAM (value + sparkline).
  Paused (auto-slept) sessions are greyed with a `[paused]` badge; stopped and
  failed sessions are hidden; columns are sortable to find the worst offender.
- **1h / 24h range toggle**: 15-second detail for the last hour, or all-day
  five-minute mean+peak trends for spotting slow memory growth / sustained load.
- **New group permission `can_view_monitoring`** (the sixth RBAC flag): admins
  and managers see the tab by default, plain users don't, and admins can grant
  or revoke it per group in the group editor. Access to the underlying snapshot
  is gated server-side by the same flag.
- The sampler reuses the existing health-worker tick (a stats pass every 15 s),
  reads host metrics from `/proc` and instance metrics via one-shot `docker
  stats` — no new process, no chart library, no database writes.

### Container runtime value renamed from `docker` to `runc`

The template-level runtime value `docker` (which always meant "use Docker's
default OCI runtime, runC") was renamed to `runc` for clarity. The default
runtime is unchanged — runC, fastest with full GPU compatibility:

- The API now accepts `runc` as the runC runtime value and no longer accepts
  `docker`; `OW_CONTAINER_RUNTIME` and template create/update defaults are
  `runc`. A migration rewrites any existing rows still holding `docker` to
  `runc`, so existing templates keep launching under runC with no action.
- The template form's Runtime dropdown now offers `runc (OCI default)`
  (default; fast, GPU-compatible) and `runsc (gVisor)` (optional; sandboxed,
  slower).

### Default container runtime is now runC (Docker); gVisor is optional

Templates previously launched under `runsc` (gVisor) by default. The default
is now **runC (Docker)** — the fastest runtime with full GPU compatibility —
end to end:

- The API defaults `OW_CONTAINER_RUNTIME` to `docker` (previously `runsc`), and
  template create/update requests without a runtime now default to `docker`.
  Existing templates with an empty runtime field resolve to Docker (runC) on
  their next launch.
- The template form's Runtime dropdown now offers two explicit choices —
  `runC (Docker)` (default; fast, GPU-compatible) and `runsc (gVisor)`
  (optional; sandboxed, slower) — instead of a bare "Default" option that
  resolved to gVisor.

### Persistent storage cleanup now removes the host data folder itself

Resetting persistent storage and the admin "Thorough Cleanup" previously only
emptied the instance's host data directory — the folder (`{root}/{template}/{user}`)
was left behind, contradicting the UI's "permanently deletes the volume directory"
wording. Both paths now delete the directory itself from the host (the helper
container mounts the parent and `rm -rf`s the leaf, since the bind-mount point
cannot be unlinked from inside a container), matching the design in
`.scratch/archive/persistent_storage/spec.md` §5. Delete/stop still preserve the
data for reuse.

### Production benchmark: measure the compose stack's CPU/RAM (`.scratch/production-benchmark/`)

A pure-bash benchmark (`scripts/benchmark/benchmark-prod.sh`) that brings up the production compose stack and reports what it and six concurrent instances consume — the first reproducible resource-cost numbers for the platform (no product code changed).

- **Four-table report** (`report.md` + per-second CSVs under `scripts/benchmark/reports/`): platform container peaks, per-instance peaks (KasmVNC / ttyd / Jupyter × runC / runsc, with `docker_in_instance`), runC-vs-runsc aggregate per remote type, and host before→after delta — plus provenance (timestamp, Docker default runtime, compose commit, image digests).
- **Real API path**: logs in with the admin cookie, creates six templates, launches six `no_persistent` instances, and samples all six in one synchronized window only after every instance reports `running`.
- **Self-cleaning**: deletes instances/templates and `docker compose down` on success and on any failure (EXIT trap); preflight fails fast with fix instructions (runsc missing, port 80 busy, images missing — auto-built via the repo image script).
- **`--smoke` mode** + `scripts/benchmark/smoke_test.sh`: short-window end-to-end verification — platform healthy, six instances running, report produced, and the host (containers, networks, DB rows, compose stack) clean afterwards.
- **Unit-testable pure library** (`benchlib.sh`): `/proc` host sampling, `docker stats --format '{{json .}}'` parsing (both PascalCase and snake_case keys), CSV/aggregation/Markdown — fixture-tested without Docker.

### Admin protection: the admin account can no longer be deleted or demoted (`.scratch/archive/admin-protection/`)

Follow-up to the security-fuzzing incident where the fuzzer's unexpected-method probing executed `DELETE /api/users/{id}` with the admin session and permanently deleted the `admin` user.

- **`DELETE /api/users/{id}` returns 403** when the target is a member of the Admin system group (resolved by `kind = 'admin'`, not the literal username) — admin self-delete and admin-delete-another-admin are both rejected, on top of the pre-existing non-admin tier guardrail.
- **`PUT /api/users/{id}` returns 403** when the target is an Admin-group member and the payload carries `group_ids` — an Admin member's membership list is now immutable via the API (the pre-existing `can_assign_groups` rule already rejects any payload containing the Admin group id), so no one — including the admin themselves — can demote the root account. Username/password and personal-ceiling edits on the admin account keep working (verified live).
- **User-management UI**: the Delete button is hidden on rows whose user `is_admin` (Edit stays). Editing an admin row now shows an "Admin membership is protected" note instead of group toggles, and saves without `group_ids`, so the personal ceiling remains adjustable without hitting a 403.
- **`pnpm run security:api` hardening**: a pre-flight guard aborts the run unless an active `unexpected-methods = []` sits inside `[phases.coverage]` in `schemathesis.toml` (section-scoped, so the line can't be relocated to a dead section); a post-run integrity check snapshots the admin's existence/`is_admin` plus template/instance row counts and dies identifying the damaged resource on any mismatch — and it runs even when a fuzz pass fails hard. Both guards proven live by mutation tests (config regression → pre-flight dies; admin row deleted mid-run → post-run dies).

### Security fuzzing: utoipa OpenAPI spec + Schemathesis dual-pass (`.scratch/archive/security-fuzzing/`)

- **Code-first OpenAPI spec**: the 17 safe endpoints (`/health`, `GET /api/auth/*`, and the read-only `GET /api/templates`, `/api/instances`, `/api/vnc/verify`, `/api/users`, `/api/groups`, `/api/registry`, `/api/docker/containers`, `/api/persistent-volumes`, `/api/admin/settings`) are annotated with `#[utoipa::path]` and exported by a build-time `export_openapi` binary to the committed `apps/api/security/openapi.json` — nothing is served at runtime. A drift-check unit test fails the suite if the artifact diverges from the annotations.
- **`pnpm run security:api`**: fuzzes the running dev stack (`pnpm run dev:nosudo`) with Schemathesis in two passes — an admin session asserting schema-valid 200s / declared 4xx and never a 5xx, and a self-provisioned `fuzz-user` session asserting `admin-gated` endpoints never return 2xx (the RBAC/BOLA/IDOR boundary). Fixed `--seed` (20260101) keeps runs reproducible.
- **Custom RBAC check**: `admin_gated_boundary` (a Schemathesis hook mounted into the container) fails any 2xx from an `admin-gated` operation to the non-admin session; gated on `OW_ENFORCE_RBAC=1` so Pass 1 is unaffected. Proven non-no-op by a mutation test (weakening `list_users`'s guard makes Pass 2 go red).
- **Self-hosted fuzz image**: `ow-schemathesis` built from `apps/api/scripts/schemathesis.Dockerfile` — the official `schemathesis:stable` image bundles a broken `tracecov` plugin that crashes `run`; no host Python / pipx is introduced.
- **Runtime findings hardened in**: `POST /api/auth/login` gained the missing `400` declaration (JSON syntax errors — e.g. a `\x00` body — are rejected with 400, verified by the fuzzer); and Schemathesis's unexpected-method probing is disabled (`[phases.coverage] unexpected-methods = []`) after the fuzzer deleted the admin user by probing `DELETE /api/users/{id}`, keeping the exported spec the only fuzz surface.

### Quality gates: Rust Clippy gate, forbid unsafe, analysis reports, web lint, standalone E2E

- **Rust hard gate**: `cargo clippy --all-targets --all-features -- -D warnings` now runs inside `apps/api/scripts/check.sh` and the `apps/api` `check` script, so `bash scripts/check.sh` and `pnpm check` agree. All pre-existing Clippy warnings were fixed (no `#[allow]` suppressions).
- **Forbid unsafe**: `#![forbid(unsafe_code)]` in the API crate roots (`lib.rs`, `main.rs`); the two `std::env::set_var` test helpers in `core/settings.rs` were refactored to an injectable source so the forbid compiles cleanly.
- **Complexity soft report**: `pnpm analysis:rust` reports `too_many_lines` (100) / `cognitive_complexity` (25) via CLI-only flags; thresholds live in `apps/api/clippy.toml`. CLI-only so the `-D warnings` gate cannot promote them.
- **Dependency-unsafe report**: `pnpm analysis:unsafe` runs `cargo geiger 2>/dev/null || true`. Known upstream limitation (cargo-geiger 0.13.0 / krates 0.18.1) — geiger prints `Failed to match (ignoring source)` stderr noise for feature-gated crates cargo locks but no feature activates; the stderr redirect keeps the report clean and `|| true` keeps the soft-report exit-0 contract. See `.scratch/archive/quality-gates/spec.md`.
- **Code-bloat report**: `pnpm analysis:bloat` runs `cargo llvm-lines` against a release build.
- **Web lint**: `apps/web` gains ESLint (flat config, eslint-plugin-svelte + typescript-eslint + eslint-config-prettier); `lint` = `eslint .`, and `check` now runs svelte-check + eslint together. Complexity rules report softly via the web `analysis` script.
- **Standalone E2E**: new `e2e/` workspace package (Playwright) targeting the running dev stack at `http://localhost`; `pnpm run test:e2e` (smoke: login/dashboard/permission-gated tabs) and `pnpm run test:e2e:full` (launch a real instance → KasmVNC viewer → WebSocket → teardown). Old `apps/web` E2E scripts and `@playwright/test` devDependency removed. Scripts named `e2e`/`e2e:full` so `turbo run test` ignores them.
- **Root `pnpm test` fixed**: `turbo.json` declares `"test": { "cache": false }` — turbo 2.10.5 errored ("Missing tasks in project") on the undeclared `test` task, so `pnpm test` was broken at the root. Now runs web vitest + api nextest.
- **`pnpm run dev:nosudo`**: dev-stack variant that skips gVisor registration and `network:allow`, so no sudo is needed for the stack itself (bandwidth/tc shaping fails open). `pnpm run dev:stop:nosudo` stops it.
