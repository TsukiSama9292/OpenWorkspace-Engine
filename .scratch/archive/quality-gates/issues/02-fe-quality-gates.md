# 02 — ESLint flat config + web lint gate

**Track:** frontend

**What to build:** The SvelteKit web app gets a real linter. A flat-config ESLint setup (`eslint-plugin-svelte` + `typescript-eslint` recommended + `eslint-config-prettier`) lints `.svelte`/`.ts`/`.js`. The `lint` package script runs `eslint .`, and the `check` script extends to run type-check and lint in one shot — so `pnpm check` fails on lint errors exactly like it fails on type errors, while staying clean on the current codebase. ESLint complexity rules (`complexity`, `max-lines-per-function`) are warnings surfaced by an on-demand `analysis:web` report rather than blocking the gate, mirroring the Rust layered model. Prettier compatibility is preserved so `pnpm lint` and `pnpm format` never fight.

**Blocked by:** `01-be` (Clippy hard gate + analysis tooling — the layered gate model it establishes is mirrored here)

**Status:** completed

- [x] ESLint flat config in `apps/web` (`eslint-plugin-svelte`, `typescript-eslint` recommended, `eslint-config-prettier`); dev dependencies added
- [x] `apps/web` `lint` script runs `eslint .`
- [x] `apps/web` `check` script runs `svelte-check` and ESLint together (type-check + lint in one command)
- [x] `complexity` / `max-lines-per-function` configured as warnings (soft report), not gate-blocking errors
- [x] Root `pnpm analysis:web` forwards to the web app, prints the complexity warnings, and exits 0
- [x] `pnpm check` is clean on the current codebase; a synthetic lint error fails it (verified during development)
- [x] Vitest suite stays green (23 files / 287 tests)
- [x] `pnpm lint` and `pnpm format` are compatible (no conflicting rules)
- [x] Agent guide's key-commands section updated for the web lint/check commands
