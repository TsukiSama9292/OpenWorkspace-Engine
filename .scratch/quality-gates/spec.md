Status: ready-for-agent

# Quality Gates: Static Analysis (Rust + Web) & Standalone Playwright E2E

## Problem Statement

The repo's quality gates are uneven. `apps/api` has a zero-warning compile policy (`check.sh`) but runs **no Clippy at all**; `apps/web` has **no linter** (root `turbo lint --filter=web` finds no task and runs nothing); and E2E is declared in `apps/web/package.json` (`test:e2e` → `playwright test`, with `@playwright/test` in devDependencies) but there is **no playwright config and no test file** — running it fails with "no tests found". Two more gaps weaken the "balanced Security · Stability · Performance" mission: there is no enforcement that *our own* Rust code stays free of `unsafe` (AI/vibe-coded contributions can quietly bypass the borrow checker), and there are no reporting tools for dependency-unsafe surface (`cargo-geiger`) or code bloat / monomorphization (`cargo-llvm-lines`).

Separately, E2E deserves to be a standalone project, decoupled from the web app, so it can run against the *live* dev stack (`pnpm run dev`) without being swept into `turbo run test`.

## Solution

Introduce a **layered quality-gate model**, applied consistently across Rust and Web:

- **Hard gates** (fail CI-like local checks, zero tolerance):
  - Rust: base Clippy lint set via `cargo clippy --all-targets --all-features -- -D warnings`, wired into the existing `check.sh` gate **and** the `apps/api` `"check"` script so `bash scripts/check.sh` and `pnpm check` agree.
  - Rust: `#![forbid(unsafe_code)]` in the API crate roots (`lib.rs`, `main.rs`) so our own code cannot contain `unsafe` — enforced by the compiler itself, not a script.
  - Web: ESLint (flat config) with `eslint-plugin-svelte` + `typescript-eslint` recommended rules + `eslint-config-prettier`, run as `eslint .` wired into the web `"lint"` script **and** appended to the web `"check"` script, so `pnpm check` covers types + lint in one shot.
- **Soft reports** (non-blocking analysis, exit 0, run on demand via root `pnpm analysis:*`):
  - Rust complexity: Clippy `restriction` rules `too_many_lines` (threshold 100) and `cognitive_complexity` (threshold 25) enabled **only in the analysis invocation** (never in the codebase attributes, so they cannot leak into the `-D warnings` gate).
  - Rust dependency-unsafe surface: `cargo geiger` report.
  - Rust code bloat: `cargo llvm-lines` (release build).
  - Web complexity: ESLint `complexity` / `max-lines-per-function` as warnings surfaced by an analysis invocation.

E2E becomes a standalone root-level workspace package `e2e/` using `@playwright/test`, targeting the **already-running dev stack** at `http://localhost` (Traefik :80). Two projects: `smoke` (login + dashboard, read-only, `admin/admin`) and `full` (launch a real instance → enter the KasmVNC viewer → verify the WebSocket path → teardown & clean up). A `globalSetup` pings the stack first so a missing dev environment fails fast with a clear message instead of timing out. The e2e package's scripts are named `e2e` / `e2e:full` (not `test`), so Turborepo (`turbo run test/check/lint/build`) ignores it; root forwards `pnpm run test:e2e` / `pnpm run test:e2e:full`. The old `apps/web` E2E scripts and `@playwright/test` devDependency are removed.

## User Stories

1. As a developer, I want `apps/api` Clippy run as a zero-warning gate inside `check.sh`, so that lint violations fail the same gate that already enforces compile warnings.
2. As a developer, I want `apps/api`'s `"check"` package script to run the same Clippy gate, so that `pnpm check` and `check.sh` cannot disagree.
3. As a developer, I want the existing Clippy warnings fixed rather than suppressed, so that the gate is green from the first commit (no `#[allow]` shortcuts).
4. As a developer, I want `#![forbid(unsafe_code)]` in the API crate roots, so that any `unsafe` our own code introduces fails to compile.
5. As a developer, I want the two existing `std::env::set_var` test helpers refactored to an injectable source, so that the `unsafe_code` forbid compiles cleanly without changing production behavior.
6. As a developer, I want Clippy's `too_many_lines` and `cognitive_complexity` reported on demand (thresholds 100 / 25) without blocking the hard gate, so that I can see complexity trends before deciding to tighten them.
7. As a developer, I want `cargo geiger` available as a report, so that I can review the `unsafe` surface of third-party dependencies during dependency review.
8. As a developer, I want `cargo llvm-lines` available as a report, so that I can find monomorphization / code-bloat hotspots during binary-size or compile-time optimization.
9. As a developer, I want all soft reports reachable from one consistent root entry point (`pnpm analysis:*`), so that I do not need to remember per-app commands.
10. As a developer, I want `apps/web` to have an ESLint flat-config setup with Svelte + TypeScript recommended rules, so that `.svelte`/`.ts`/`.js` are all linted.
11. As a developer, I want ESLint wired into the web `"lint"` script **and** appended to the web `"check"` script, so that `pnpm check` runs type-check + lint in one command.
12. As a developer, I want ESLint complexity rules (`complexity`, `max-lines-per-function`) reported softly rather than failing the gate, mirroring the Rust layered model.
13. As a developer, I want the ESLint config compatible with Prettier (`eslint-config-prettier`), so that `pnpm lint` and `pnpm format` never fight.
14. As a developer, I want a standalone `e2e/` workspace package using `@playwright/test`, so that E2E is decoupled from the web app.
15. As a developer, I want `pnpm run test:e2e` to run a fast smoke suite (login `admin/admin`, dashboard renders, template/instance lists, permission-gated tabs) against the **running** dev stack at `http://localhost`, so that I get quick regression feedback without launching containers.
16. As a developer, I want `pnpm run test:e2e:full` to additionally launch a real instance, open the KasmVNC viewer, and verify the proxied WebSocket path, so that the most complex route is covered end-to-end.
17. As a developer, I want the `full` suite to tear down its instances and routes afterwards, so that the dev DB and Docker never accumulate test residue.
18. As a developer, I want a `globalSetup` that pings `http://localhost` and `/api` and fails fast with "dev stack not running" if the stack is down, so that I am not left guessing through timeout errors.
19. As a developer, I want the E2E scripts named `e2e` / `e2e:full` (not `test`) so that Turborepo's `turbo run test` never sweeps E2E into the regular suite.
20. As a developer, I want the old `apps/web` `test:e2e` / `test:e2e:ui` scripts and `@playwright/test` devDependency removed, so that there is exactly one E2E home.
21. As a developer, I want the AGENTS.md / docs updated to reflect the new gates and commands, so that future agents run the same checks.
22. As an operator, I want the layered model documented (hard gates vs soft reports), so that the project's quality posture is explicit and extensible.

## Implementation Decisions

- **Clippy hard gate**: extend `apps/api/scripts/check.sh` with a step running `cargo clippy --all-targets --all-features -- -D warnings` (plus the existing `cargo check --lib`), and set `apps/api/package.json` `"check"` to the same gate. Base (non-restriction) Clippy lints only — default `cargo clippy` behavior, no `pedantic`/`restriction` groups in the gate.
- **Restriction lints are command-line only**: `too_many_lines` / `cognitive_complexity` are enabled via the analysis invocation (`cargo clippy -- -W clippy::too_many_lines -W clippy::cognitive_complexity`), **not** via `#![warn(...)]` attributes in the crate — attributes would be promoted to errors by the gate's `-D warnings` and break the layered split. Thresholds live in `apps/api/clippy.toml` (`too-many-lines-threshold = 100`, `cognitive-complexity-threshold = 25`).
- **Forbid unsafe**: `#![forbid(unsafe_code)]` at the top of `apps/api/src/lib.rs` and `apps/api/src/main.rs`. The two `std::env::set_var` calls in `src/core/settings.rs` unit tests (edition-2024-unsafe) are removed by refactoring the tests to build `Settings` from an injected source instead of mutating process env; production logic unchanged.
- **ESLint flat config**: new `apps/web/eslint.config.js` using `eslint-plugin-svelte` (flat config), `typescript-eslint` recommended, and `eslint-config-prettier`. Base recommended rules error (hard gate); `complexity` and `max-lines-per-function` configured as `warn` (soft). Dev dependencies added to `apps/web` (`eslint`, `eslint-plugin-svelte`, `typescript-eslint`, `eslint-config-prettier`, `eslint-plugin-js` if needed for `.js`).
- **Web scripts**: `apps/web/package.json` — `"lint": "eslint ."`; `"check"` becomes `svelte-kit sync && svelte-check --tsconfig ./tsconfig.json && eslint .`. The complexity warnings are surfaced by a web `analysis` script (ESLint invocation printing warnings, exit 0).
- **Root analysis group**: root `package.json` adds `analysis:rust`, `analysis:unsafe`, `analysis:bloat`, `analysis:web` forwarding to the app packages; all soft, non-blocking.
- **E2E package**: new `e2e/` directory with `package.json` (`name: "e2e"`), `tsconfig.json`, `playwright.config.ts`, and `tests/`. `pnpm-workspace.yaml` gains `"e2e"`. Scripts: `"e2e": "playwright test --project=smoke"`, `"e2e:full": "playwright test --project=full"`. Root forwards `"test:e2e"` → `pnpm --filter e2e e2e` and `"test:e2e:full"` → `pnpm --filter e2e e2e:full`.
- **Playwright config**: `baseURL: 'http://localhost'` (Traefik :80, the full path incl. `/api` and instance routes); `globalSetup` pings `http://localhost` and `/api` (e.g. GET returning 401 is fine — proves the stack is up) and errors out with a clear "dev stack not running — start with pnpm run dev" message otherwise. Projects: `smoke` (testMatch `*.smoke.spec.ts`), `full` (testMatch `*.full.spec.ts`). `fullyParallel: false` (shared dev DB), `trace: 'on-first-retry'`. No `webServer` — the stack is expected to be up.
- **E2E cleanup**: `full` specs use `afterAll`/teardown to delete created instances (and their routes are removed by the API on delete); smoke specs are read-only.
- **Turbo isolation**: E2E scripts deliberately named `e2e`/`e2e:full` so the default Turborepo task set (`test`/`check`/`lint`/`build`/`dev`) ignores them. Deviation from plan: turbo.json **did** gain a `"test": { "cache": false }` task — under turbo 2.10.5 an undeclared `test` task made `turbo run test` error ("Missing tasks in project") for every package, so `pnpm test` was pre-broken at the root. Declaring it fixes root `pnpm test` (web vitest + api nextest, e2e skipped since it has no `test` script).
- **Doc sync**: AGENTS.md and the README/docs get the new commands and gate model; this is part of the feature's Definition of Done.

## Testing Decisions

A good test here asserts *observable gate behavior*, not tooling internals: the gates either fail loudly on violations or stay silent when clean, and E2E either passes against the live stack or reports a clear precondition failure.

- **Gate seams (Rust)**: `bash scripts/check.sh` must be **silent** before and after the change (existing policy); the newly-added Clippy step is verified by (a) clean on the current codebase once warnings are fixed, and (b) a throwaway check that a synthetic violation fails the gate (performed once during development, not committed). The `settings.rs` refactor is covered by the existing unit tests, which must stay green.
- **Gate seams (Web)**: `pnpm check` (svelte-check + eslint) is clean on the current codebase; a synthetic error in a fixture demonstrates failure. Existing vitest suite (23 files / 287 tests) stays green.
- **Soft report seams**: `pnpm analysis:rust`, `pnpm analysis:unsafe`, `pnpm analysis:bloat`, `pnpm analysis:web` all exit 0 and print a report. `cargo geiger` and `cargo llvm-lines` are recorded as installed tooling in the API dev tooling notes.
- **E2E seams (Playwright, live stack)**: `pnpm run test:e2e` (smoke) passes against a running dev stack with `admin/admin`; the spec asserts dashboard renders, template/instance lists load, and a non-admin-equivalent session sees permission-gated tabs hidden. `pnpm run test:e2e:full` launches a real instance (requires template images + Docker), enters the KasmVNC viewer, and cleans up afterwards; teardown asserts no leftover instance of the test's name remains.
- **Precondition seam**: `globalSetup` fails fast with a clear message when the stack is down (verified by running `test:e2e` with no stack up and asserting the actionable error).
- **Prior art**: Rust integration tests run real containers via `cargo nextest`; the existing `apps/web` vitest suites use `happy-dom`; Playwright is already declared in the web package but never wired — this spec finally exercises it. Regression gates: `bash scripts/check.sh` silent, `apps/web` vitest green, `turbo run test` unaffected by E2E.

## Out of Scope

- Promoting `too_many_lines` / `cognitive_complexity` / ESLint complexity rules to hard gates (they are intentionally soft reports; revisit when the codebase is clean).
- Enabling `pedantic`/`restriction`/`nursery` Clippy groups beyond the two chosen rules.
- Auditing the `unsafe` inside third-party dependencies (report only; no dependency changes).
- Binary-size budget enforcement (`llvm-lines` is a report, not a check).
- CI configuration (no `.github` workflows today; the gates are designed to drop into CI later).
- Playwright `webServer` auto-start of the dev stack (the stack must be running; only `globalSetup` health-check is added).
- E2E coverage of Jupyter/ttyd instances in this pass (KasmVNC desktop path is the primary target for `full`; other remotes can be added later).
- Changing the dev authentication story (tests reuse the dev `admin/admin` bootstrap account; no test-only accounts).

## Further Notes

- The layered model (hard gates + soft reports) is chosen deliberately to match the project's "Security · Stability · Performance" balance: strict where failure is cheap and meaningful (compile/lint/unsafe in our code), lenient where output is advisory (complexity trends, dependency unsafe, bloat).
- `#![forbid(unsafe_code)]` is the strongest form of the "no unsafe in our code" goal — stronger than a lint gate because it cannot be bypassed by script omission or `#[allow]`.
- The restriction lints being CLI-only is an important subtlety: putting `#![warn(clippy::too_many_lines)]` in the crate would let the gate's `-D warnings` promote them to errors, silently re-introducing the hard gate the layered model deliberately defers.
- Known upstream limitation for `analysis:unsafe`: cargo-geiger 0.13.0 (pinning krates 0.18.1) prints hundreds of `Failed to match (ignoring source) package: …` stderr lines for crates that cargo locks into `Cargo.lock` but no activated feature reaches (e.g. sqlx's feature-gated `sqlx-mysql`/`sqlx-sqlite`, plus schemars 0.9/1.2, borsh, rkyv, quinn, pgvector, mac_address, indexmap 1.9, …). These are normal lockfile residents, not stale orphans — `cargo generate-lockfile` re-locks them, and the lockfile format version (3 vs 4) is irrelevant (cargo metadata output is identical). krates 0.18.1 re-resolves from the workspace roots and prunes unreachable packages, so cargo-geiger's fallback (`mapping/metadata.rs:81`, `matches_ignoring_source`) re-prints each failed lookup (~39×). The report table goes to stdout and is correct; only stderr is noisy. `analysis:unsafe` therefore runs `cargo geiger 2>/dev/null || true`: the redirect drops the noise, `|| true` honors the soft-report exit-0 contract. Do not attempt to fix this by editing `Cargo.lock`.
- E2E against the live dev stack is the chosen trade-off: slower and environment-dependent, but it exercises the real Traefik → API → container → WebSocket path that mock-based tests cannot.
