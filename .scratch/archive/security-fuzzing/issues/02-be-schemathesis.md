# 02 — Schemathesis fuzzing harness: dual-pass script, provisioning, custom 403 check

**Track:** backend (tooling — this feature has no SvelteKit UI work, so the middle ticket is the fuzzing harness, not `02-fe`)

**What to build:** A one-command `pnpm run security:api` that fuzzes the 17 green endpoints of a **running** dev stack with Schemathesis in two passes. Pass 1 (admin session) asserts schema-valid 200s (or declared 4xx) and never a 5xx under malformed/extreme input. Pass 2 (a self-provisioned low-privilege `fuzz-user` session) asserts the RBAC boundary: `admin-gated` endpoints must never return 2xx to the fuzz user (403 or 404 are acceptable). The script fails fast with an actionable message when the stack is down, provisions the fuzz user idempotently, and exits non-zero if either pass fails. When this ticket lands, a contributor can run security fuzzing against the running dev stack with zero manual setup and reproduce the result with a fixed seed.

**Blocked by:** `01-be-utoipa` (the exported spec file is the fuzzer's input)

**Status:** done

- [x] Shell script in the API's scripts directory: fails fast (ping `/api/health`; if down, report "start with pnpm run dev:nosudo"), then regenerates the exported spec, then runs both passes; exits non-zero on any failed pass
- [x] Idempotent provisioning: log in as the dev `admin`/`admin`, confirm `fuzz-user` exists via the users list, create it via the users API (placed in the **User** system group, fixed dev password) if absent, then log it in for the Pass-2 cookie; the account is left in place after the run
- [x] Pass 1 (admin session): Schemathesis via the `ow-schemathesis` Docker image (built from `schemathesis.Dockerfile` — the official `schemathesis:stable` bundles a broken `tracecov` plugin that crashes `run`) with `--network host`, the spec and hook mounted read-only, fuzzing directly at `http://localhost:3000`, the admin cookie as a `Cookie` header, `--max-examples 30` (env-overridable), a fixed default seed (env-overridable), and checks `not_a_server_error`, `status_code_conformance`, `content_type_conformance`, `response_schema_conformance`, `negative_data_rejection`
- [x] Pass 2 (fuzz-user session): same invocation with the fuzz-user cookie plus a custom check (a small Python module mounted into the container) that fails any `admin-gated`-tagged operation returning 2xx to the fuzz user (403 or 404 pass)
- [x] API package script + root `pnpm run security:api` forwarding wired up; the custom check loads via `schemathesis.toml` (`hooks = "schemathesis_pre_run"`) with the module resolved through `PYTHONPATH=/harness`; activation is gated on `OW_ENFORCE_RBAC=1` (set only for Pass 2)
- [x] Agent guide's dev-tooling section notes Schemathesis runs via the Docker image (no host Python / pipx)
