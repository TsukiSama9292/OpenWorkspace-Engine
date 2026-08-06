# OpenWorkspace Engine — Agent Guide

## Project nature

Experimental. A Docker Compose stack that runs KasmVNC/Jupyter/ttyd containers behind a Traefik reverse proxy, with a custom SvelteKit browser UI. Goal: share one Linux box's resources among multiple devs via browser-based virtual desktops — with group-based RBAC, per-instance `/30` network isolation, resource governance (auto-sleep, keep-time, bandwidth caps, instance ceilings), and persistent user data.

## Reading material — start here

Read these before working on the project. They are the constitution + living docs:

- **[mission.md](mission.md)** — why the project exists, core features, design philosophy (Security · Stability · Performance), anti-goals.
- **[roadmap.md](roadmap.md)** — completed phases (with commit references) and planned phases, plus the Definition of Done for each stage.
- **[CHANGELOG.md](CHANGELOG.md)** — chronological change log (append to it when a user-visible change lands).
- **[docs/user-guide/](docs/user-guide/)** — user-facing guides (for the most basic user — see [Docs conventions](#docs-conventions)):
  - `architecture.md` — what the platform is, how sessions are created and connected, isolation and scaling
  - `rbac.md` — the group-based permission model (flags, template whitelist, instance ceiling, tier)
  - `frontend.md` — the browser UI: pages, tabs, session viewers
  - `persistent-storage.md` — persistent user data: what is kept across stop/start/delete
  - `remote-auth.md` — how sessions are secured (per-instance credentials, server-side injection)
- **[docs/developer-guide/](docs/developer-guide/)** — engineering docs for developers, AI coding agents, and operators (see [Docs conventions](#docs-conventions)):
  - `tech-stack.md` — technology decisions (ADR-style rationale), quality gates, dev + prod deploy flow, update flow, env vars
  - `development.md` — setup, commands, debugging, production
  - `gvison.md` — gVisor/runsc sandboxing, NVProxy GPU passthrough, driver setup
  - `caching-strategy.md` — when the in-process cache is enough, and when a shared cache is needed
  - `lock-registry.md` — how host ports and instance subnets are reserved without conflicts
  - `apps/api/security/openapi.json` — generated REST API spec (per-endpoint payloads and auth)
- **[docs/agents/](docs/agents/)** — the agent issue tracker, triage labels, and domain layout
- **[.scratch/*/spec.md](.scratch/)** — spec/issue files for **planned/in-progress** features (triage labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`). **Completed** features are archived under `.scratch/archive/` (Status: `completed`).

## Docs conventions

Docs live in two guides, with different audiences and rules:

- **`docs/user-guide/`** — written for the most basic user: someone who will use
  or operate the product, not read its source. When writing or updating these:
  - Explain **usage**, **feature modules**, and **abstracted logic**. Mention
    underlying mechanisms only briefly — never descend into code-level
    implementation (no structs, functions, source file paths, or container
    internals).
  - **Never write an API URL, an API name, or an API schema**: no `/api/...`
    paths, no `GET`/`POST ...` endpoint references, no payload / response JSON,
    no endpoint tables. The single source of endpoint truth is the generated
    OpenAPI spec at `apps/api/security/openapi.json` — point readers there once
    instead of enumerating endpoints.
- **`docs/developer-guide/`** — engineering docs for developers, AI coding
  agents, and operators: setup, commands, debugging, deploy flows, ADRs,
  operational mechanisms, and the agent tooling under `docs/agents/`. No
  API-ban — a developer guide may reference the OpenAPI spec and source paths
  freely, but must stay accurate and keep the hard quality gates in view.

Both guides share one rule: the technical history — the problems we hit and how
we solved them — lives in `.scratch/archive/` (closed specs and issues), not in
the guides. Link there when a reader would benefit.

## Monorepo layout

Two active apps: `apps/web/` (SvelteKit frontend) and `apps/api/` (Rust API). pnpm workspaces + Turborepo. `apps/vnc-ui/` was **removed** — do not reference it.

- `apps/api/` — Axum REST API. `src/routes/` (auth, users, groups, templates, instances, registry, proxy, admin_settings), `src/docker.rs` (DockerService mockable seam + DockerClient container/network/volume/bandwidth ops), `src/host_port.rs` + `src/instance_net.rs` (flock allocators), `src/route_writer.rs` (Traefik YAML), `src/network_qos.rs` (tc/HTB), `src/health_worker.rs` (3s lifecycle worker), `migration/` (sqlx migrations `000001`–`000021`).
- `apps/web/` — SvelteKit static SPA (`adapter-static`, `ssr=false`). `src/lib/api/` (client + action helpers), `src/lib/stores/auth.ts` (EffectiveContext store), `src/lib/permissions.ts` (mayControlInstance / mayLaunchTemplate), `src/lib/preflight.ts`, `src/lib/vnc/` (noVNC core), `src/lib/components/` (panels: templates, instances, groups, users, volumes, admin).

## Key commands

```bash
# Root (turbo)
pnpm run dev          # full dev stack: kill-dev → init → compose dev up → network:allow → api+web concurrently
pnpm run dev:nosudo   # same, skipping gVisor registration + network:allow (no sudo; bandwidth/tc fails open)
pnpm run build        # turbo build all packages
pnpm test             # turbo run test
pnpm check            # turbo run check
pnpm run test:e2e     # Playwright smoke against a RUNNING dev stack (login/dashboard/permission-gated tabs)
pnpm run test:e2e:full# Playwright full: launch real instance → KasmVNC viewer → WebSocket → teardown

# web only
cd apps/web
pnpm test             # vitest run — 23 files, 290 tests
pnpm check            # svelte-kit sync + svelte-check (typecheck) + eslint . (hard lint gate)
pnpm lint             # eslint . — flat config (eslint-plugin-svelte + typescript-eslint recommended)
pnpm run analysis:web # soft report: eslint complexity / max-lines-per-function warnings — exit 0

# api only
cd apps/api
bash scripts/check.sh          # zero-warning gate (both feature sets) — must produce NO output
bash scripts/run_tests.sh      # cargo nextest run --features docker (158 unit + 324 integration tests)

# Rust quality gates & analysis reports
cd apps/api
bash scripts/check.sh          # also runs `cargo clippy --all-targets --all-features -- -D warnings` (hard lint gate)
pnpm run analysis:rust         # soft report: clippy too_many_lines (100) / cognitive_complexity (25) — exit 0
pnpm run analysis:unsafe       # soft report: cargo geiger (third-party unsafe surface) — exit 0
pnpm run analysis:bloat        # soft report: cargo llvm-lines (monomorphization hotspots) — exit 0
```

Rust quality-gate model (see `.scratch/archive/quality-gates/spec.md`):
- **Hard gates**: Clippy base lints via `-D warnings` inside `check.sh` + the api `check` script (they agree); `#![forbid(unsafe_code)]` in `lib.rs`/`main.rs` — our own code cannot contain `unsafe` (compiler-enforced). Thresholds live in `apps/api/clippy.toml` (`too-many-arguments-threshold = 25`, plus the two soft-report thresholds).
- **Soft reports** (non-blocking, exit 0, run via root `pnpm analysis:*`): clippy complexity rules (`too_many_lines`/`cognitive_complexity` are CLI-only — never in crate attributes, so `-D warnings` can't promote them), `cargo geiger`, `cargo llvm-lines`.
- Dev tooling: `cargo-geiger` and `cargo-llvm-lines` are installed via `cargo install`.
- Security fuzzing: `pnpm run security:api` fuzzes the 17 safe endpoints of a **running** dev stack (`pnpm run dev:nosudo`) with Schemathesis in two passes — admin session (schema-conformance + no-5xx) and a self-provisioned `fuzz-user` session (RBAC boundary: `admin-gated` ops must never 2xx). Runs via the `ow-schemathesis` Docker image built from `apps/api/scripts/schemathesis.Dockerfile` (the official `schemathesis:stable` image bundles a broken `tracecov` plugin that crashes `run`), no host Python / pipx. The spec is regenerated each run; fixed `--seed` (default 20260101) makes failures reproducible. Fuzzes the API directly at `http://localhost:3000` (Traefik's `/api` router can't reach `/health`).

`pnpm lint` runs `eslint .` in `apps/web` only; root `turbo lint` runs per-package (web only).

## Build/deploy flow (production: `docker/openworkspace/`)

1. `apps/web/Dockerfile` builds the SvelteKit static site (pnpm workspace, repo-root build context) and serves it from nginx.
2. `apps/api/Dockerfile` builds the Rust API (runs as root; needs `pid: host`, `cap_add: [SYS_ADMIN, NET_ADMIN, SYS_PTRACE]`, `apparmor=unconfined`, rw Docker socket — see compose).
3. Traefik is the single reverse proxy (file-provider only, no Docker socket): `/` → web, `/api` → api, and `/kasmvnc/<token>/websockify` + `/ttyd/<token>/` + `/jupyter/<token>/` → per-instance containers.
4. The API writes per-instance route files into `./traefik/dynamic` (its `TRAEFIK_DYNAMIC_DIR`, mounted rw into the api container, ro into traefik at `/etc/traefik/dynamic`; traefik watches it). Instance route files (`*-ws.yml`) are gitignored there.
5. Deploy: `docker compose -f docker/openworkspace/docker-compose.yml up -d --build`. Postgres data lives in the `server-pgdata` volume; `ow-network` is created by `scripts/docker-network.sh` (`pnpm run init`).
6. DB migrations run automatically on API startup (sqlx) — no manual step. Traefik hot-reloads route files, so updates are zero-downtime.
7. Dev flow differs: `docker/openworkspace_dev/` traefik proxies to host-run dev servers (`host.docker.internal:5173` / `:3000`); the API writes routes to `docker/openworkspace_dev/traefik/dynamic` (compile-time default when `TRAEFIK_DYNAMIC_DIR` is unset).

## RBAC model (flat groups — no roles)

- No `users.role` / `is_system_admin` columns (dropped by migrations `000018`–`000020`). Permissions live on **groups**: five flags (`can_create_template`, `can_manage_users`, `can_manage_group_instances`, `can_manage_docker`, `can_manage_registry`), a template whitelist (`group_templates`), and `max_instances`.
- Three fixed **system groups** seeded at startup: Admin, Manager, User (`groups.kind`). Admin-group membership *is* admin; tiers are derived (Admin=2, Manager=1, else 0). System groups cannot be renamed/deleted; Admin flags are pinned all-on, User flags all-off.
- Every request resolves the user's **effective context** from the DB (JWT is identity-only: `sub` + `exp`). Permissions change takes effect on the next request. JWT never carries roles.
- **Template authorization is group-only** — admins do NOT bypass the whitelist. Template `visibility`: `public` / `private` / `hidden`.
- Instance ceiling = `groups.max_instances` (union, highest) with optional `users.direct_max_instances` (can only raise); host-wide `host_instance_limit` in `admin_settings`. Enforcement is precise (`FOR UPDATE` counting) for per-user, best-effort for host.
- Instance control (`mayControlInstance`): owner, admin, or a group-instance holder whose target owner shares a group and is of a strictly lower tier.
- See [docs/user-guide/rbac.md](docs/user-guide/rbac.md) — keep it in sync when permissions change.

## Network bandwidth limiting (tc/HTB)

- Per-template `network_bandwidth_up_mbps` / `network_bandwidth_down_mbps` (0 = unlimited).
- Enforced with `tc`/HTB at the kernel on the instance veth pair: upload shapes egress on
  `eth0` inside the container netns; download shapes egress on the host-side veth. To find
  that veth, read the container's `eth0@ifN` via `nsenter` — `N` is the peer veth's ifindex
  in the *host* netns (unique); do NOT match the container-side ifindex against host `@ifN`
  values, since every container numbers its `eth0` as `2`.
- `DockerService::apply_bandwidth_limit` (mockable) is the seam; the real `DockerClient`
  runs `nsenter -t <pid> -n tc ...` (container netns for upload, pid 1 / host netns for
  download). Apply points: inside `create_container_from_template` after start, and in the
  start-instance route after restarting a stopped container (Docker recreates the veth pair
  on every start, destroying prior qdiscs). Failures are fail-open (`tracing::error!`).
- The API image runs as root with `iproute2`/`util-linux`; the prod compose gives the API
  `pid: host`, `cap_add: [SYS_ADMIN, NET_ADMIN]`, and rw Docker socket.
- Pure logic lives in `apps/api/src/network_qos.rs` (tc arg builders + veth matcher), unit
  tested without Docker.
- Verify a live host actually shapes: `sudo apps/api/scripts/apply_bw_smoke.sh` (needs
  host `iproute2`, `iperf3` not required — uses python3; busybox:1 image).

## Host port & subnet lock registry (flock)

- Both finite per-host pools — host ports (`host_port.rs`) and instance `/30`
  subnets (`instance_net.rs`) — are allocated under non-blocking `flock`
  lockfiles in a shared per-UID directory, so concurrent launches across any
  number of API processes on one host never claim the same resource. This
  replaced the old in-process `network_lock` mutex. See `docs/developer-guide/lock-registry.md`.
- Lockfile key = resource identity: `{port}.lock` and `{network_addr}.lock`
  (e.g. `10.200.0.0.lock`). Files are created if absent and **never unlinked**
  (unlinking would let two processes lock different inodes at one path).
  Reservations (`ReservedPort`, `ReservedSubnet`) are RAII over the `OwnedFd`;
  dropping the handle — or the process dying — releases the lock via the kernel.
  There is intentionally no force-unlink/force-release.
- Lock dir resolution order: `PORT_LOCK_DIR` env → `/run/user/<uid>/ow_ports` →
  `$XDG_RUNTIME_DIR/ow_ports` → `/tmp/ow-ports-<uid>`; dir made/verified `0700`,
  owned by the current UID. `resolve_lock_dir` → `None` makes allocation **fail
  closed**. Per-candidate `flock` failure just skips that candidate.
- Port allocator adds a TCP probe (`port_in_use`) on lock winners to catch
  binders that don't participate (other tools, Docker itself). Subnet allocator
  probes nothing — the `docker list_networks` used-set is the source of truth.
- Stale-snapshot races are absorbed by bounded retries keyed on Docker's own
  errors: `is_port_conflict` (`port is already allocated`, in
  `create_container_with_port_retry`) and `is_network_pool_overlap` (`Pool
  overlaps`, in `ensure_instance_network`, up to 4 attempts). Retries re-scan
  from a per-instance token-derived spread (`spread_offset` /
  `spread_block_offset`) so concurrent retries don't stampede.
- Subnet reservation is held only through `create_network`; port reservation
  through container create/start. On delete, `docker rm -f` (force) frees the
  port binding, and `kill_residual_runtime_procs` kills stuck runsc processes
  so veths don't pin the network (network removal fails while veths live).
- Test seams: unit tests in `host_port.rs`/`instance_net.rs` use isolated temp
  lock dirs; `instances_mock_test.rs` exercises real allocators through the HTTP
  stack; `two_process_flock_e2e_test.rs` runs two API processes against one DB
  and asserts distinct ports and aligned `/30`s.

## SvelteKit specifics

- `adapter-static` — fully static SSG, no server
- `ssr = false` + `trailingSlash = 'always'` in `+layout.js`
- `base: ''` in svelte.config.js — app detects instance from `window.location.pathname`
- Param routes `kasmvnc/[token]/+page.svelte` and `open/[token]/+page.svelte` handle session URLs
- noVNC core files live in `apps/web/src/lib/vnc/` with shim files in `apps/web/src/lib/vnc/shims/`
- `pako@1` pinned — v3 broke internal import paths used by noVNC

## KasmVNC gotchas

- KasmVNC startup hardcodes `-sslOnly` — Traefik must use `https://host.docker.internal:<host_port>` with a `serversTransport` that has `insecureSkipVerify: true` (`kasm-insecure`)
- `VNCOPTIONS=-disableBasicAuth` env var required to disable HTTP Basic Auth on websockify endpoint
- `RFB` constructor: `touchInput` param must be a real hidden `<input>` DOM element, not `false`/`null`
- `mouseButtonMapper` is initialized to `null` in rfb.js — must be manually instantiated with `MouseButtonMapper` class after RFB creation (default button mapping in `apps/web/src/lib/components/vnc/VncViewer.svelte`)

## Testing

### Rust API (`apps/api`)

- 158 unit tests in `src/` + 324 integration tests in `tests/` (auth, db, docker lifecycle, instances, registry, templates, users, groups, vnc-verify, health). Integration tests require Docker (real containers/networks), run in parallel via `cargo nextest`.
- Gate: `bash scripts/check.sh` must be **silent** (zero warnings, both feature sets) before `bash scripts/run_tests.sh`.

### Web (`apps/web`)

- `pnpm test` — vitest with `happy-dom` + `@testing-library/svelte`. **23 files / 290 tests** in `src/tests/` (auth-store, permissions, rbac-actions, preflight, rejection-notice, group-panel, user-panel, admin-settings, api-client, template-actions, dashboard-view, template-form, quick-launch, keepalive, keep-time-line, orphaned-volumes-*, countdown, format, …).
- `tests/mocks/` provides `app-navigation` (`goto`) and `app-stores` (`page`) stubs.
- Playwright E2E configured but not actively run (requires live VNC containers). No CI currently.

## Rust API — zero-warning policy

`apps/api/` must compile with **zero warnings**. This is enforced — do not use `#[allow(dead_code)]`, `#[allow(unused)]`, `#[allow(warnings)]`, or any suppression attribute to silence compiler warnings.

### Check command

```bash
cd /home/user/workspace/OpenWorkspace-Engine/apps/api && bash scripts/check.sh
```

Both invocations must produce **no output**. The first checks default features; the second checks the `docker` feature gate.

The actual test runner is `apps/api/scripts/run_tests.sh`, which starts a Postgres test container via Docker and runs `cargo nextest run --features docker`. Run both warning checks before the test script:

```bash
cd apps/api && bash scripts/check.sh
cd apps/api && bash scripts/run_tests.sh 2>&1 | grep -E "(FAIL|Summary)"
```

### How to fix warnings

The test harness (`tests/common/mod.rs`) is compiled independently by each integration-test binary. Items unused by *any single binary* trigger `dead_code`. The fix pattern:

1. **Split shared code into focused submodules** — e.g. `common/pg.rs` for Postgres setup, so binaries that only need `ensure_pg` don't compile `TestContext`.
2. **Use `#[path]`** when a test file needs one submodule without the parent: `#[path = "common/pg.rs"] mod pg;`.
3. **Move single-use helpers** (like `ensure_network`) into the test file that uses them, rather than keeping them in the shared module.
4. **Convert `ctx.client.get(…)` to `ctx.get(…)`** (and `post`, `put`, `delete`) so the helper methods are exercised across all test files.
5. **Add `test_context_helpers`** — a small test in each binary that calls every `TestContext` method at least once, ensuring no method is dead in any binary.

Never suppress a warning. Always fix the root cause.

## .gitignore

`build/`, `.svelte-kit/`, `node_modules/`, `.turbo/`, `.codegraph/`, `.env*` are gitignored. Do not commit compiled output. Per-instance route files (`traefik/dynamic/*-ws.yml`) are gitignored.

## Reference code

`references_repo/KasmVNC/kasmweb/` is the upstream KasmVNC source. `core/` contains noVNC protocol files (the basis for `apps/web/src/lib/vnc/`). `app/` contains the original UI logic (reference only).

`references_repo/gvisor/` is a shallow (depth-1) clone of upstream gVisor (`runsc`), sparse-checked-out to only the `g3doc/` docs directory. runsc is registered as a Docker runtime by `scripts/docker-runtime-gvisor.sh` (`pnpm run init`) and selectable per template (`container_runtime` field). See `docs/developer-guide/gvison.md`.

`references_repo/docker-docs/` is a shallow (depth-1) clone of upstream Docker docs, sparse-checked-out to only the `content/` docs directory (reference for Docker compose/networking/custom-runtime docs).

<!-- CODEGRAPH_START -->
## CodeGraph

In repositories indexed by CodeGraph (a `.codegraph/` directory exists at the repo root), reach for it BEFORE grep/find or reading files when you need to understand or locate code:

- **MCP tool** (when available): `codegraph_explore` answers most code questions in one call — the relevant symbols' verbatim source plus the call paths between them, including dynamic-dispatch hops grep can't follow. Name a file or symbol in the query to read its current line-numbered source. If it's listed but deferred, load it by name via tool search.
- **Shell** (always works): `codegraph explore "<symbol names or question>"` prints the same output.

If there is no `.codegraph/` directory, skip CodeGraph entirely — indexing is the user's decision.
<!-- CODEGRAPH_END -->

## Development workflow

New work flows through five stages, each a skill. A stage starts only when the previous one is explicitly agreed/published — don't skip ahead or combine stages.

Prerequisite: the pipeline assumes the repo is already configured with an issue tracker, triage label vocabulary, and domain-doc layout (the `## Agent skills` block below). If those are missing, run the `setup-matt-pocock-skills` skill once first — it is one-time repo bootstrap, not a per-feature stage.

1. **grilling** (`grilling`) — Clarify the problem. Grill the user one question at a time, walking each branch of the decision tree and resolving dependencies between decisions before moving on. Look facts up in the environment (filesystem, code, tools) rather than asking; the user owns the *decisions*. Stop only when AI and user share a mental model.

2. **to-spec** — Without further interview, distill the agreed understanding into the spec at `.scratch/<feature-slug>/spec.md`. The spec is conceptual consensus, not code: Problem Statement, Solution, a long numbered User Stories list, Implementation Decisions, Testing Decisions (including the pre-agreed seams), Out of Scope, Further Notes. No file paths or code snippets. Use the `codebase-design` vocabulary when deciding the seams.

3. **to-tickets** — Split the spec into exactly three tickets, one file per ticket at `.scratch/<feature-slug>/issues/<NN>-<slug>.md`: `01-be-<slug>` (all backend/Rust API work), `02-fe-<slug>` (all frontend/SvelteKit UI work), and `03-int-end-to-end` (the full-stack test). Present the breakdown to the user, iterate until approved, then publish. No subagents and no large ticket fan-out — the backend ticket is completed in one pass before the frontend ticket begins.

4. **implement & tdd** — Implement the tickets directly, in order, one continuous pass each: finish all of the backend ticket, then all of the frontend ticket, then the E2E ticket. No subagents. Test-first at the pre-agreed seams (red → green, one slice at a time), typecheck and run the relevant test files while working, and use the `codebase-design` vocabulary when designing module interfaces. Run the full suite once at the end.

5. **code-review** — Review the completed work against the tickets along two axes: Standards (repo conventions + smell baseline) and Spec (does it fulfill `.scratch/<feature-slug>/issues/<NN>-<slug>.md`). Fix findings, then run the analysis gates before closing out: re-run the suite (`bash scripts/check.sh` + web `pnpm check` — these already enforce clippy `-D warnings` and eslint); run the static-analysis soft reports where the change touches the relevant layer (`pnpm run analysis:rust` / `analysis:unsafe` / `analysis:bloat` for Rust, `pnpm run analysis:web` for web); and run the security fuzzer `pnpm run security:api` against a running dev stack when the change touches the API surface (RBAC/instance/registry/routes) — hard-gate failures are findings to fix, soft-report regressions are review findings too. Then close out with **automated doc sync**:
   - Update `.scratch/<feature-slug>/spec.md` and `.scratch/<feature-slug>/issues/*.md` to reflect what was actually built (close completed tickets, adjust the spec's Implementation/Testing Decisions if reality diverged).
   - Update `docs/user-guide/` and `docs/developer-guide/` where the change touches them (rbac, architecture, frontend, persistent-storage, remote-auth in user-guide; development, tech-stack, caching-strategy, gvison in developer-guide, …) — keep them in sync with what was built, following the [Docs conventions](#docs-conventions).
   - Update `roadmap.md` — move the delivered phase/feature from planned to completed (with a commit reference) or record the deviation.
   - Generate or update `CHANGELOG.md` with a concise summary of the user-visible changes (chronological; append, don't rewrite history).
   Only then commit to the current branch.
6. **archive** — Once a feature is fully delivered and committed, flip its `Status:` line (and any ticket `**Status:**` lines) from `ready-for-agent` to `completed` in the spec, then `git mv .scratch/<feature-slug>/ .scratch/archive/<feature-slug>/`. The archive is the closed-ticket record (history is also in git); only *planned* features stay live at `.scratch/<feature-slug>/`.

The pipeline is a loop, not a one-way gate: findings from stage 5 can bounce back to a new round of grilling/spec/tickets for follow-up work.

Two supporting skills are used *within* the stages, not as stages of their own:

- **codebase-design** — the deep-module vocabulary (module, interface, seam, depth, adapter) for designing module interfaces and deciding where a seam goes. Reach for it during **to-spec** (the pre-agreed seams) and **implement** (interface design).
- **github-repository-reference** — when a stage needs to consult an external GitHub repo, add it as a git submodule under `references_repo/` (upstream KasmVNC, gVisor, and docker-docs already live there).

## Agent skills

### Issue tracker

Issues are tracked as local markdown files under `.scratch/<feature-slug>/`. See `docs/agents/issue-tracker.md`.

### Triage labels

The five canonical triage roles use their standard names: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context layout. See `docs/agents/domain.md`.
