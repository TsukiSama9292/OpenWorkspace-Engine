# 01 — Code-first OpenAPI spec for the 17 safe endpoints (utoipa) + export binary + drift check

**Track:** backend

**What to build:** The 17 safe API endpoints get a code-generated OpenAPI specification (utoipa) with typed response declarations, exported to a committed `security/openapi.json` artifact by a build-time binary that never starts the server, and guarded by a drift-check test so the artifact can never silently diverge from the annotations. No runtime route is added — the spec is export-only, ever. When this ticket lands, running the export produces a spec file that accurately describes the 17 endpoints and their allowed response statuses, and any subsequent annotation change without a re-export fails the test suite.

**Blocked by:** None — can start immediately.

**Status:** done

- [x] `utoipa` added as a dependency with the `axum_extras` feature (for path/query/body params); no `utoipa-swagger-ui` / `utoipa-axum` (the spec is exported, never served)
- [x] All 17 green endpoints annotated with `#[utoipa::path]`: `GET /health`, `POST /api/auth/login`, `GET /api/auth/me`, `GET /api/auth/validate`, `GET /api/templates`, `GET /api/templates/{id}`, `GET /api/instances`, `GET /api/instances/{id}`, `GET /api/vnc/verify`, `GET /api/users`, `GET /api/users/{id}`, `GET /api/groups`, `GET /api/registry`, `GET /api/registry/url`, `GET /api/docker/containers`, `GET /api/persistent-volumes`, `GET /api/admin/settings`
- [x] The 8 admin-gated endpoints are tagged `admin-gated` so the Pass-2 custom check can identify them by tag, not path
- [x] Response schemas are dual-track: real serializable structs (e.g. `EffectiveContext`, settings/registry payloads) get `#[derive(ToSchema)]` directly; `json!`-assembled responses (`instance_to_json` / `template_to_json`) keep their handlers returning `Json<Value>` unchanged, with dedicated declaration-only schema structs referenced from the annotations
- [x] Every annotated response explicitly declares the realistic rejection statuses as applicable (400 / 401 / 404 / 415 / 422), so Axum/serde rejections are not flagged as undeclared statuses by the fuzzer
- [x] Export binary added that builds the derived `OpenApi` document and writes pretty JSON to `security/openapi.json` without starting the server
- [x] `security/openapi.json` is committed to the repo
- [x] Drift-check unit test: rebuilding the `OpenApi` document and comparing the regenerated JSON against the committed file; fails when they differ; runs in the normal API test suite with no stack required
- [x] Zero-warning gate holds (`bash scripts/check.sh` silent for both feature sets) and the full API test suite passes
