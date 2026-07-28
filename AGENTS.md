# OpenWorkspace Engine — Agent Guide

## Project nature

Experimental. A Docker Compose stack that runs KasmVNC containers behind nginx, with a custom SvelteKit browser UI. Goal: share one Linux box's resources among multiple devs via browser-based virtual desktops.

## Monorepo layout

Two active apps: `apps/vnc-ui/` (SvelteKit frontend) and `apps/api/` (Rust API). pnpm workspaces + Turborepo.

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

## Build/deploy flow

1. `pnpm build` in `apps/vnc-ui/` produces static files in `build/`
2. Docker Compose bind-mounts `build/` into nginx container at `/usr/share/nginx/html`
3. After rebuilding frontend: `docker compose restart nginx` to pick up changes
4. KasmVNC containers (`kasm`, `kasm2`) serve desktops on internal port 6901
5. nginx proxies WebSocket at `/kasm1/websockify` → `https://kasm:6901/websockify` (SSL verify off)

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
cd /home/user/workspace/OpenWorkspace-Engine/apps/api \
  && cargo test --no-run 2>&1 | grep -i warning; \
  cargo test --no-run --features docker 2>&1 | grep -i warning
```

Both invocations must produce **no output**. The first checks default features; the second checks the `docker` feature gate.

The actual test runner is `apps/api/scripts/run_tests.sh`, which starts a Postgres test container via Docker and runs `cargo nextest run --features docker`. Run both warning checks before the test script:

```bash
cd apps/api && cargo test --no-run 2>&1 | grep -i warning
cd apps/api && cargo test --no-run --features docker 2>&1 | grep -i warning
cd apps/api && apps/api/scripts/run_tests.sh
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

<!-- CODEGRAPH_START -->
## CodeGraph

In repositories indexed by CodeGraph (a `.codegraph/` directory exists at the repo root), reach for it BEFORE grep/find or reading files when you need to understand or locate code:

- **MCP tool** (when available): `codegraph_explore` answers most code questions in one call — the relevant symbols' verbatim source plus the call paths between them, including dynamic-dispatch hops grep can't follow. Name a file or symbol in the query to read its current line-numbered source. If it's listed but deferred, load it by name via tool search.
- **Shell** (always works): `codegraph explore "<symbol names or question>"` prints the same output.

If there is no `.codegraph/` directory, skip CodeGraph entirely — indexing is the user's decision.
<!-- CODEGRAPH_END -->
