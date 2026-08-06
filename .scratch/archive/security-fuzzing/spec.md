Status: complete — 01-be, 02-be, and 03-int all shipped, verified, and documented. `pnpm run security:api` passes both passes against the dev stack (182 cases, 0 failures, seed 20260101), the drift-check + 17-endpoint unit tests are green, `check.sh` is silent, the Pass-2 RBAC guard provably fires under the mutation test, and the doc sync (AGENTS.md, development.md, CHANGELOG, roadmap) is done. One runtime hardening fix landed during convergence: Schemathesis's "unexpected-method" coverage was disabled (`unexpected-methods = []`) after the fuzzer destroyed the admin user by probing `DELETE /api/users/{id}` — see Further Notes.

# Security Fuzzing: utoipa OpenAPI Spec + Schemathesis (Dual-Pass)

## Problem Statement

The HTTP API has no automated security testing layer. There is no OpenAPI specification (only a hand-written API reference, `docs/api-reference.md` — since deleted and replaced by the generated `apps/api/security/openapi.json`), so no schema-driven fuzzing, no automated probing of malformed-input handling, and no fuzz-level verification that the RBAC boundary holds. The existing 597+324 Rust tests exercise the authorization gates with *well-formed* requests, and the new `e2e/` Playwright suite covers the browser path — but nothing throws *malformed/extreme* input at the API while asserting "no 5xx, no crash, no privilege escalation." A security-conscious experimental project (per `mission.md` — Security is the first pillar) is shipping without this layer.

Two concrete gaps feed this:

1. **No spec, no fuzzer.** Without an OpenAPI document, Schemathesis/OFFAT-class tools have nothing to drive on. The 40 registered endpoints exist only as Axum routes + prose docs.
2. **The RBAC boundary is untested under hostile input.** The effective-context model (five flags, template whitelist, tier guardrail) is unit/integration tested with valid shapes; a low-privilege session receiving a garbage payload on an admin-gated endpoint is not. That is exactly the BOLA/IDOR surface.

## Solution

Generate an OpenAPI specification for a curated set of **17 safe endpoints** from the Axum handlers themselves (utoipa, code-first), export it to a committed artifact at build time, and run **Schemathesis** against the running dev stack in **two passes**: one authenticated as admin (response integrity, no 5xx), one as a dedicated low-privilege fuzz user (the 403 authorization boundary must hold). A committed spec snapshot plus a drift-check test keeps the artifact honest. The spec is never served at runtime — it is a build-time export only, so production exposes no new URL.

The 17 endpoints are the only ones fuzzable without risking host resources, destructive state, or lockout:

- Public/any-authenticated reads + auth boundary: `GET /health`, `POST /api/auth/login`, `GET /api/auth/me`, `GET /api/auth/validate`, `GET /api/templates`, `GET /api/templates/{id}`, `GET /api/instances`, `GET /api/instances/{id}`, `GET /api/vnc/verify`
- Admin-gated reads (the Pass-2 boundary targets): `GET /api/users`, `GET /api/users/{id}`, `GET /api/groups`, `GET /api/registry`, `GET /api/registry/url`, `GET /api/docker/containers`, `GET /api/persistent-volumes`, `GET /api/admin/settings`

Everything else (users/groups/templates/instances writes, instance lifecycle, registry sync/url PUT, docker container create, persistent-volume cleanup, admin-settings PUT, change-password, logout, heartbeat) is **excluded from the spec entirely** — the whitelist is enforced at spec-generation (the spec *is* the fuzz surface), and the exported file is asserted to cover exactly 17 paths before each run.

## User Stories

1. As a developer, I want the OpenAPI spec generated from the code (utoipa), so that the spec can never silently drift from the handlers.
2. As a developer, I want only the 17 safe endpoints annotated, so that no destructive endpoint (launch, delete, cleanup, docker create, registry url write) is ever in the fuzzable surface.
3. As a developer, I want a build-time export binary that writes the spec to a file without starting the server, so that nothing is served at runtime.
4. As a developer, I want the exported spec committed to the repo, so that spec changes are reviewable in diffs.
5. As a developer, I want a drift-check test that fails when the regenerated spec differs from the committed file, so that the artifact stays in sync with the annotations.
6. As a developer, I want the response schemas for endpoints that already have real serializable structs (e.g. the effective context) to be derived directly, so that no duplicate schema maintenance exists for them.
7. As a developer, I want the hot `json!`-assembled responses (templates, instances) left untouched with declared-only schema structs, so that shared serializers and the 597-test blast radius are protected.
8. As a developer, I want `pnpm run security:api` to run Schemathesis against the running dev stack, so that security fuzzing is one command.
9. As a developer, I want the script to fail fast when the dev stack is down, so that I get an actionable message instead of a confusing timeout.
10. As a developer, I want the script to self-provision the low-privilege fuzz user idempotently, so that there is zero manual setup and the run is reproducible.
11. As a developer, I want the admin pass to assert that the 17 endpoints return schema-valid 200s (or declared 4xx) and never a 5xx under garbage input, so that parser/renderer robustness is verified.
12. As a developer, I want the low-privilege pass to assert that admin-gated endpoints never return 2xx to the fuzz user, so that the RBAC boundary holds under hostile input (BOLA/IDOR).
13. As a developer, I want malformed JSON/data rejected with 400/415/422 (never 500), so that Axum/serde rejection semantics are locked in.
14. As a developer, I want a fixed seed and bounded examples, so that runs are reproducible and fast during schema convergence.
15. As a developer, I want the spec itself schema-validated before fuzzing, so that utoipa output bugs surface as clear errors rather than false failures.
16. As a developer, I want to verify the custom 403-boundary check actually fires (mutation test), so that a silent no-op guard is impossible.
17. As a developer, I want the security tooling documented in the agent guide and dev docs, so that future contributors know the command and its precondition.

## Implementation Decisions

- **Specification library**: `utoipa` (with the `axum_extras` feature for path/query/body params). No `utoipa-swagger-ui`, no `utoipa-axum` — the spec is exported, not served.
- **No runtime route**: the spec is reachable only as a build-time artifact. There is no `/api/openapi.json` handler; production never exposes the spec.
- **Export mechanism**: a small binary target builds `ApiDoc::openapi()` (the derived `OpenApi` struct listing all 17 annotated handlers) and writes pretty JSON to the spec file. The library target already exists, so the binary `use`s the crate without starting the server.
- **Response schemas — dual-track**:
  - Real serializable structs (e.g. `EffectiveContext`, settings/registry payloads, list-item structs) get `#[derive(ToSchema)]` added directly — no duplicate declaration.
  - `json!`-assembled responses (`instance_to_json`, `template_to_json`) keep their handlers returning `Json<Value>`; the utoipa annotations declare dedicated schema structs that exist only as documentation. utoipa's lenient object schemas (extra fields allowed) mean Schemathesis flags only missing fields / type mismatches — the fuzzer doubles as the drift detector and the annotation set is converged to green.
- **Admin-gated tagging**: the eight admin-gated endpoints are tagged (e.g. `admin-gated`) so the Pass-2 custom check can identify them regardless of path.
- **Committed artifact**: the exported spec file is committed and covered by a drift-check test.
- **Schemathesis runtime**: a custom Docker image `ow-schemathesis` (built from `apps/api/scripts/schemathesis.Dockerfile` — the official `schemathesis/schemathesis:stable` bundles a broken `tracecov` plugin that crashes `run`) with `--network host`; no host Python toolchain is introduced. The spec file and the custom-check hook are mounted read-only into the container.
- **Fuzzing command shape**: `run <mounted-spec> --base-url http://localhost:3000` — the fuzzer targets the API directly on the host port, not Traefik (Traefik's `/api` router can't reach `/health`). The session rides as a `--header "Cookie: ow_token=…"`, with `--max-examples 30` (env-overridable), a fixed default seed (env-overridable), and `--phases examples,coverage,fuzzing`. The spec is schema-validated by a jq assertion on the exported file (`paths | length == 17`) rather than a Schemathesis `--validate-schema` flag.
- **Active checks**: `not_a_server_error`, `status_code_conformance`, `content_type_conformance`, `response_schema_conformance`, `negative_data_rejection`. The declared responses for each green endpoint explicitly include the rejection statuses (`400`/`401`/`404`/`415`/`422` as applicable) so legitimate Axum/serde rejections are not flagged as undeclared statuses.
- **Dual pass**:
  - Pass 1 (admin session): asserts schema-valid 200s (or declared 4xx), never 5xx.
  - Pass 2 (fuzz-user session): same endpoints, plus a **custom check** asserting that `admin-gated`-tagged operations never return 2xx to the fuzz user (403 or 404 are both acceptable — random-UUID path lookups may legitimately 404 before the permission gate).
- **Custom check**: a small Python module (`schemathesis_pre_run.py`, mounted into the container, resolved via `PYTHONPATH=/harness`) registered as a Schemathesis hook from `schemathesis.toml` (`hooks = "schemathesis_pre_run"`). It fails any request on an `admin-gated` operation whose response status is 2xx under the low-privilege session. Custom checks run on every invocation regardless of the `-c/--checks` flag (verified against Schemathesis 4.24.3), so activation is gated on the `OW_ENFORCE_RBAC=1` env var — set only for Pass 2, inert for Pass 1.
- **Provisioning**: the script logs in as `admin`/`admin` (dev stack), reads `GET /api/users` to confirm the `fuzz-user` account exists, creates it via `POST /api/users` (placed in the **User** system group, fixed dev password) if absent, then logs it in for the Pass-2 cookie. Idempotent; the account is left in place after the run (deliberately not deleted).
- **Script**: a shell script in the API's scripts directory wired to a package script and forwarded at the root as `pnpm run security:api`. Sequence: fail-fast health ping → regenerate the exported spec → provision → Pass 1 → Pass 2. Any failed pass exits non-zero.
- **Standalone**: `security:api` is not wired into `test:e2e:full` or `turbo run test` — it shares the "running dev stack" precondition with the Playwright suite but is a separate concern and run on demand.

## Testing Decisions

Two seams, confirmed with the user:

1. **Primary — black-box Schemathesis against the running dev stack** (the highest seam; same shape as the existing `e2e/` Playwright package, which targets the live stack at `http://localhost`). This is where the feature's value lives: external HTTP behavior under malformed/extreme input.
   - What makes a good test here: a **negative** contract — schema-valid 200 on the authorized session, declared 4xx on rejection, **never 5xx**; on the low-privilege session, **never 2xx** on `admin-gated` operations. The `negative_data_rejection` and `not_a_server_error` checks are the machine-readable form of "Axum/serde rejects garbage instead of crashing."
   - **Mutation test (03 ticket)**: temporarily weaken a permission guard, run Pass 2, and require the custom check to fail — proving the guard is not a no-op — then revert.
2. **Secondary — unit-test drift guard in the API test suite** (`cargo test`, no stack): a unit test in the OpenAPI module rebuilds `ApiDoc` and asserts the regenerated spec equals the committed file. Prior art: the existing 158 unit tests and the zero-warning/nextest gate in `apps/api`.

Acceptance for the integration ticket (03): `pnpm run security:api` is green against a `dev:nosudo` stack; malformed inputs produce 400/415/422 and never 500 (evidence: `negative_data_rejection` + `not_a_server_error` checks pass); Pass 2 admin-gated endpoints return 403; the custom check demonstrably fires under the mutation test; the drift-check test is green.

## Out of Scope

- Annotating the remaining 23 endpoints (yellow/red) or serving a full 40-endpoint spec.
- Any runtime exposure of the spec (`/api/openapi.json` route, Swagger UI) — the spec is export-only, ever.
- Wiring `security:api` into CI or into `turbo run test` / the Playwright suite.
- Other security tools (ZAP, Akto, OFFAT) — deliberately deferred; Schemathesis covers the API-fuzzing + RBAC-boundary need for now. Browser-layer DAST waits for a real browser-heavy attack surface.
- Fuzzing authenticated `change-password` (lockout risk), `logout` (session invalidation mid-run), or any resource-heavy/destructive endpoint.
- Network-layer scanning, dependency/SCA, or infrastructure security.

## Further Notes

- The actual registered endpoint count is **40**, not 57 (the "57" was a count of method keywords in the then-hand-written API reference, which double-counted table cells). The green subset is 17.
- Axum rejection status semantics, locked in by the fuzzer during convergence: `Json<T>` **syntax** errors (e.g. a `\x00` body) → `400` (added to the login responses — the fuzzer's one real finding), schema/data errors → `422`, missing `Content-Type: application/json` → `415`, query rejection → `400`, missing/invalid `ow_token` → `401`, permission denied → `403`, unknown resource → `404`. The declared responses include these so conformance checks don't misfire.
- **Unexpected-method hardening (found during convergence, not by the fuzzer's own reports):** Schemathesis's coverage phase sends "unexpected HTTP method" requests (e.g. `DELETE /api/users/{id}`, `DELETE /api/templates/{id}`) to every path even when that method is absent from the spec — and, per the docs, those requests are sent regardless of whether the `unsupported_method` check is enabled. The exported spec only declares `GET`, but the **real Axum router registers the mutating handlers at the same paths** (`delete_user`, `delete_template`). Pass 1 runs with the admin cookie on every request, so on 2026-08-06 the fuzzer deleted the admin user and a template mid-run (SQL-visible in the API log: `DELETE FROM "users"`, `DELETE FROM "workspace_templates"`). Fix: `schemathesis.toml` sets `[phases.coverage] unexpected-methods = []` to disable unsupported-method testing entirely — the exported spec is the only surface the fuzzer may touch. Re-verified: both passes green, no state damage.
- The `fuzz-user` account and its fixed dev password exist only in the dev database; they never touch production. The dev DB's existing `test` template is whitelisted to Admin + User groups — `POST /api/instances` is excluded from the spec regardless, so the fuzz session cannot launch containers.
- Dev tooling note for the agent guide: Schemathesis runs via the Docker image (no `pipx`/host Python), matching the "no-sudo / clean host" posture of `dev:nosudo`.
- The drift detector works both ways: a schema declaration that is wrong about the response will surface as a Schemathesis response-validation failure during the convergence loop, and the committed spec snapshot keeps those declarations reviewable.
