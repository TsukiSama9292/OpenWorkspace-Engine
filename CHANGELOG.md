# Changelog

Chronological, user-visible changes. Append, don't rewrite history.

## [Unreleased]

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
