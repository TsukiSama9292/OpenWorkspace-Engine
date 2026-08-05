# OpenWorkspace Engine — Agent Guide

## Project nature

Experimental. A Docker Compose stack that runs KasmVNC containers behind a Traefik reverse proxy, with a custom SvelteKit browser UI. Goal: share one Linux box's resources among multiple devs via browser-based virtual desktops.

## Monorepo layout

Two active apps: `apps/web/` (SvelteKit frontend) and `apps/api/` (Rust API). pnpm workspaces + Turborepo.

## Key commands

```bash
# Root (turbo)
turbo build          # build all packages
turbo dev            # dev servers (no cache)

# vnc-ui only
cd apps/vnc-ui
pnpm test            # vitest run (21 tests)
pnpm build           # vite build → apps/vnc-ui/build/
pnpm check           # svelte-check (typecheck)

# api only
cd apps/api && apps/api/scripts/run_tests.sh   # Rust tests via cargo nextest
```

No lint script exists in vnc-ui. Root `turbo lint` runs if configured per-package.

## Build/deploy flow (production: `docker/openworkspace/`)

1. `apps/web/Dockerfile` builds the SvelteKit static site (pnpm workspace, repo-root build context) and serves it from nginx.
2. `apps/api/Dockerfile` builds the Rust API (runs as root; needs `pid: host`, `cap_add: [SYS_ADMIN, NET_ADMIN]`, `apparmor=unconfined`, rw Docker socket — see compose).
3. Traefik is the single reverse proxy (file-provider only, no Docker socket): `/` → web, `/api` → api, and `/kasmvnc/<token>/websockify` + `/ttyd/<token>/` + `/jupyter/<token>/` → per-instance containers.
4. The API writes per-instance route files into `./traefik/dynamic` (its `TRAEFIK_DYNAMIC_DIR`, mounted rw into the api container, ro into traefik at `/etc/traefik/dynamic`; traefik watches it). Instance route files (`*-ws.yml`) are gitignored there.
5. Deploy: `docker compose -f docker/openworkspace/docker-compose.yml up -d --build`. Postgres data lives in the `server-pgdata` volume; `ow-network` is created by `scripts/docker-network.sh` (`pnpm run init`).
6. Dev flow differs: `docker/openworkspace_dev/` traefik proxies to host-run dev servers (`host.docker.internal:5173` / `:3000`); the API writes routes to `docker/openworkspace_dev/traefik/dynamic` (compile-time default when `TRAEFIK_DYNAMIC_DIR` is unset).

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

## SvelteKit specifics

- `adapter-static` — fully static SSG, no server
- `ssr = false` + `trailingSlash = 'always'` in `+layout.js`
- `base: ''` in svelte.config.js — app detects instance from `window.location.pathname`
- Catch-all route `[...path]/+page.svelte` handles `/kasm1/`, `/kasm2/`
- noVNC core files live in `src/lib/vnc/` with shim files in `src/lib/vnc/shims/`
- `pako@1` pinned — v3 broke internal import paths used by noVNC

## KasmVNC gotchas

- KasmVNC startup hardcodes `-sslOnly` — nginx must use `proxy_pass https://` with `proxy_ssl_verify off`
- `VNCOPTIONS=-disableBasicAuth` env var required to disable HTTP Basic Auth on websockify endpoint
- `RFB` constructor: `touchInput` param must be a real hidden `<input>` DOM element, not `false`/`null`
- `mouseButtonMapper` is initialized to `null` in rfb.js — must be manually instantiated with `MouseButtonMapper` class after RFB creation (default button mapping in `src/lib/components/VncViewer.svelte`)

## Testing

- vitest with `jsdom` environment, `@testing-library/svelte`, `happy-dom` setup
- 21 unit tests in `src/tests/` — run with `pnpm test` from `apps/vnc-ui/`
- Playwright E2E configured but not actively run (requires live VNC containers)

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
cd apps/api && bash scripts/run_tests.sh 2>&1 | grep -iE "(fail|error|warn|summary)"
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

`build/`, `.svelte-kit/`, `node_modules/` are gitignored. Do not commit compiled output.

## Reference code

`references_repo/KasmVNC/kasmweb/` is the upstream KasmVNC source. `core/` contains noVNC protocol files (the basis for `src/lib/vnc/`). `app/` contains the original UI logic (reference only).

`references_repo/gvisor/` is a shallow (depth-1) clone of upstream gVisor (`runsc`), sparse-checked-out to only the `g3doc/` docs directory. Planned default container Runtime for instances — wire it up as the default runtime in compose/instance creation.

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

5. **code-review** — Review the completed work against the tickets along two axes: Standards (repo conventions + smell baseline) and Spec (does it fulfill `.scratch/<feature-slug>/issues/<NN>-<slug>.md`). Fix findings, re-run the suite, then commit to the current branch.

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
