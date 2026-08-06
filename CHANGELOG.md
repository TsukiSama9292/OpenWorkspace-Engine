# Changelog

Chronological, user-visible changes. Append, don't rewrite history.

## [Unreleased]

### Quality gates: Rust Clippy gate, forbid unsafe, analysis reports, web lint, standalone E2E

- **Rust hard gate**: `cargo clippy --all-targets --all-features -- -D warnings` now runs inside `apps/api/scripts/check.sh` and the `apps/api` `check` script, so `bash scripts/check.sh` and `pnpm check` agree. All pre-existing Clippy warnings were fixed (no `#[allow]` suppressions).
- **Forbid unsafe**: `#![forbid(unsafe_code)]` in the API crate roots (`lib.rs`, `main.rs`); the two `std::env::set_var` test helpers in `core/settings.rs` were refactored to an injectable source so the forbid compiles cleanly.
- **Complexity soft report**: `pnpm analysis:rust` reports `too_many_lines` (100) / `cognitive_complexity` (25) via CLI-only flags; thresholds live in `apps/api/clippy.toml`. CLI-only so the `-D warnings` gate cannot promote them.
- **Dependency-unsafe report**: `pnpm analysis:unsafe` runs `cargo geiger 2>/dev/null || true`. Known upstream limitation (cargo-geiger 0.13.0 / krates 0.18.1) — geiger prints `Failed to match (ignoring source)` stderr noise for feature-gated crates cargo locks but no feature activates; the stderr redirect keeps the report clean and `|| true` keeps the soft-report exit-0 contract. See `.scratch/quality-gates/spec.md`.
- **Code-bloat report**: `pnpm analysis:bloat` runs `cargo llvm-lines` against a release build.
- **Web lint**: `apps/web` gains ESLint (flat config, eslint-plugin-svelte + typescript-eslint + eslint-config-prettier); `lint` = `eslint .`, and `check` now runs svelte-check + eslint together. Complexity rules report softly via the web `analysis` script.
- **Standalone E2E**: new `e2e/` workspace package (Playwright) targeting the running dev stack at `http://localhost`; `pnpm run test:e2e` (smoke: login/dashboard/permission-gated tabs) and `pnpm run test:e2e:full` (launch a real instance → KasmVNC viewer → WebSocket → teardown). Old `apps/web` E2E scripts and `@playwright/test` devDependency removed. Scripts named `e2e`/`e2e:full` so `turbo run test` ignores them.
- **Root `pnpm test` fixed**: `turbo.json` declares `"test": { "cache": false }` — turbo 2.10.5 errored ("Missing tasks in project") on the undeclared `test` task, so `pnpm test` was broken at the root. Now runs web vitest + api nextest.
- **`pnpm run dev:nosudo`**: dev-stack variant that skips gVisor registration and `network:allow`, so no sudo is needed for the stack itself (bandwidth/tc shaping fails open). `pnpm run dev:stop:nosudo` stops it.
