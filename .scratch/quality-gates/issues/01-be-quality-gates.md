# 01 — Clippy hard gate, forbid unsafe, and Rust analysis tooling

**Track:** backend

**What to build:** The Rust API enforces its quality gates at compile/check time. `check.sh` (and `apps/api`'s `check` package script) run Clippy as a zero-warning gate so any lint violation fails the same gate that already enforces compile warnings — and the gate is green from the first commit, with all existing Clippy warnings fixed rather than suppressed. Our own code can no longer contain `unsafe` at all (`#![forbid(unsafe_code)]` in the crate roots), with the two `std::env::set_var` test helpers refactored to an injectable source so the forbid compiles cleanly. On-demand, non-blocking reports (via root `pnpm analysis:rust` / `analysis:unsafe` / `analysis:bloat`) surface Clippy complexity rules (`too_many_lines` 100 / `cognitive_complexity` 25), third-party `unsafe` usage (`cargo geiger`), and code bloat / monomorphization (`cargo llvm-lines`).

**Blocked by:** None — can start immediately.

**Status:** completed

- [x] All existing Clippy warnings in `apps/api` fixed (base lints only; no `#[allow]` suppression, no new warnings from the `docker` feature gate)
- [x] `cargo clippy --all-targets --all-features -- -D warnings` runs inside `check.sh` (zero-warning gate stays silent on the clean codebase, and fails on a synthetic violation)
- [x] `apps/api`'s `check` package script runs the same Clippy gate, so `pnpm check` and `check.sh` agree
- [x] `#![forbid(unsafe_code)]` present in `apps/api/src/lib.rs` and `apps/api/src/main.rs`
- [x] `settings.rs` unit tests no longer call `std::env::set_var` (refactored to an injectable source); production behavior unchanged; unit tests stay green
- [x] `clippy.toml` exists with `too-many-lines-threshold = 100` and `cognitive-complexity-threshold = 25`; restriction rules enabled only via the analysis invocation (never in crate attributes, so the `-D warnings` gate is unaffected)
- [x] `cargo-geiger` and `cargo-llvm-lines` installed (recorded in API dev tooling)
- [x] Root `pnpm analysis:rust` / `analysis:unsafe` / `analysis:bloat` forward to the API and exit 0 while printing a report
  - `analysis:unsafe` runs as `cargo geiger 2>/dev/null || true`. Verified: clean report (no noise), exit 0, ~1 min.
  - Known upstream limitation (cargo-geiger 0.13.0 / krates 0.18.1, see spec Further Notes): geiger prints hundreds of `Failed to match (ignoring source) package: …` stderr lines for feature-gated crates that cargo locks (sqlx-mysql/sqlite, schemars, borsh, rkyv, quinn, …) but no activated feature reaches. These are NOT stale lockfile orphans — `cargo generate-lockfile` re-locks them (only dropped `concurrent-queue`). krates prunes unreachable-from-root packages from its graph, so cargo-geiger's `matches_ignoring_source` fallback (mapping/metadata.rs:81) re-prints them (~39× each). The stderr redirect removes the noise; the report on stdout is unaffected.
- [x] Full API suite green (`bash scripts/check.sh` silent, `cargo nextest` passes)
- [x] Agent guide's key-commands section updated for the new Rust gate/report commands
