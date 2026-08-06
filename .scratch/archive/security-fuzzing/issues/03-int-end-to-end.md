# 03 — End-to-end verification on the dev:nosudo stack: schema convergence, mutation test, docs sync

**Track:** integration

**What to build:** Prove the whole `pnpm run security:api` flow actually works against the running dev stack, converge the declared schemas to the real responses (the fuzzer as drift detector), and prove the Pass-2 authorization guard is not a no-op by mutating a permission check and watching the run go red. Finish with the automated doc sync (agent guide, dev docs, changelog, roadmap, spec/issue sync). When this ticket lands, `pnpm run security:api` is a verified, documented, reproducible one-command security fuzz on a running dev stack: malformed input is rejected with 400/415/422 and never 500, the low-privilege session never gets 2xx on admin-gated endpoints, and both failures are provably detected.

**Blocked by:** `02-be-schemathesis` (the harness must exist to verify it)

**Status:** done — all verification items green and doc sync complete.

- [x] `pnpm run security:api` runs green against a `dev:nosudo` stack; with the stack down it fails fast with the actionable message
- [x] Malformed inputs produce 400/415/422 and never 500 (evidence: `negative_data_rejection` and `not_a_server_error` checks pass)
- [x] Pass 2: every `admin-gated` endpoint returns 403 (or 404) to the fuzz user and never 2xx
- [x] Mutation test: temporarily weakened the `list_users` permission guard (`users.rs:146`, `can_manage_users`), ran Pass 2 — the custom check fired red on `GET /api/users` and `GET /api/users/{id}` (`admin_gated_boundary`), then reverted the mutation and both passes went green again; the guard provably fires
- [x] Schema convergence complete: the declared response schemas match reality — no response-validation failures remain in either pass
- [x] Drift-check test is green (`cargo test --lib -- openapi`); `check.sh` silent; API + web suites still green
- [x] Doc sync: agent guide key commands + dev tooling (AGENTS.md), development docs security section (`docs/developer-guide/development.md` — "Security Fuzzing"), CHANGELOG entry, roadmap completed-stage entry, and `.scratch/archive/security-fuzzing/` spec/issue statuses all updated to reflect what was actually built
