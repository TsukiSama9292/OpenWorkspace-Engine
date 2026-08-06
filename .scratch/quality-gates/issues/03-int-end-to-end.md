# 03 — Standalone Playwright E2E against the live dev stack

**Track:** integration

**What to build:** End-to-end tests move out of the web app into a standalone `e2e/` workspace package using `@playwright/test`, targeting the **already-running** dev stack at `http://localhost` (Traefik :80 — the full browser → Traefik → SPA/API/instance path). Two projects: `smoke` (login as `admin/admin`, dashboard renders, template/instance lists load, permission-gated tabs hidden for a non-admin-equivalent session; read-only) and `full` (launch a real instance, enter the KasmVNC viewer, verify the proxied WebSocket path, then tear down the instance so the dev DB/Docker stay clean). A `globalSetup` pings the stack first and fails fast with an actionable "dev stack not running — start with pnpm run dev" message instead of timing out. Root forwards `pnpm run test:e2e` / `test:e2e:full`; the old `apps/web` E2E scripts and `@playwright/test` devDependency are removed. E2E scripts are deliberately named `e2e`/`e2e:full` (not `test`) so Turborepo never sweeps them into `turbo run test`.

**Blocked by:** `02-fe` (web lint gate — the web app must be green before full-stack tests run against it)

**Status:** completed

- [x] `e2e/` workspace package created with `@playwright/test`, its own `package.json`/`tsconfig.json`; added to `pnpm-workspace.yaml`
- [x] `playwright.config.ts`: `baseURL: 'http://localhost'`, `globalSetup` health-check (pings `/` and `/api`, fails fast with a clear "dev stack not running" message), projects `smoke` (testMatch `*.smoke.spec.ts`) / `full` (testMatch `*.full.spec.ts`), `fullyParallel: false`, `trace: 'on-first-retry'`, no `webServer`
- [x] `smoke` spec: login `admin/admin`, dashboard renders, template/instance lists load, permission-gated tabs hidden for a non-admin-equivalent session — read-only, no writes
- [x] `full` spec: launch a real instance from a template, open the KasmVNC viewer, verify the proxied WebSocket connection, then teardown (delete instance; assert no leftover test instance remains)
- [x] Root `pnpm run test:e2e` → smoke and `pnpm run test:e2e:full` → full; scripts named `e2e`/`e2e:full` so Turborepo ignores them
- [x] `apps/web` `test:e2e` / `test:e2e:ui` scripts and `@playwright/test` devDependency removed
- [x] Smoke passes against a running dev stack; `globalSetup` fails fast with an actionable message when the stack is down
- [x] `turbo run test` is unaffected by the E2E package
- [x] Agent guide's key-commands section updated for the E2E commands

**Notes (post-build sync):** `turbo run test` was found pre-broken under turbo 2.10.5 (the undeclared `test` task made turbo error "Missing tasks in project" for every package, including `--filter=web`). Fixed by declaring `"test": { "cache": false }` in `turbo.json` — E2E stays excluded (no `test` script, `Command = <NONEXISTENT>`, skipped). Root `pnpm test` now runs web vitest + api nextest and passes.
