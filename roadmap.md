# OpenWorkspace Engine — Roadmap

> This document is the project's "constitution", article three: it breaks down
> the stages and planned features **in implementation order**.
> Principle: finish first, polish later; every stage ends with a working
> result, and **a single host is always the viable minimum bar**.

---

## Planning principles

1. **Vertical slices first** — every stage delivers a complete "usable from the
   browser" feature, never half-built pieces.
2. **Single-host first** — multi-host/clusters are a plus, but a single host
   must always run the full platform.
3. **Security never lags** — every stage keeps zero-trust isolation (isolated
   subnets, per-instance credentials, group RBAC).
4. **Status markers**: ✅ Done (with commit references) / 🔵 In progress / 📋
   Planned / 💡 Idea (unscheduled).

---

## Completed Stages (retrospective)

### ✅ Stage 1: Core Infrastructure

The bare minimum for a single host to "just run": one browser admin UI + one
launchable container + one route.

| Deliverable | Content |
|---|---|
| Control-plane API | Rust + Axum, bollard controlling Docker, sqlx + PostgreSQL |
| Dynamic routing | API writes Traefik route YAML; inotify hot reload; new instances reachable in seconds |
| JWT auth | `ow_token` cookie, login/logout, `/auth/me` |
| Container lifecycle | create/start/stop/pause/delete + health checks + state machine |
| SvelteKit static SPA | adapter-static, single-page dashboard, VNC viewer (noVNC integration) |
| gVisor readiness | runsc runtime registration script + per-template runtime selection |

### ✅ Stage 2: Multiple Interfaces & Resource Governance

Expand from "one desktop" to "three interfaces", and start treating resources
as something to govern.

| Deliverable | Content |
|---|---|
| Jupyter Lab and ttyd terminal | Three `remote_type`s, each with its own route/auth pattern (Basic injection vs URL token) |
| Auto-sleep (run-time limit) | `max_run_seconds` + `timeout_action`, frontend countdown warning and redirect |
| Keep time (idle reclamation) | focus heartbeat + `keep_time_seconds`; reclaim when exceeded |
| Network bandwidth shaping | `tc`/HTB up/down Mbps caps, veth-pair discovery, fail-open |
| Persistent user data | whole-home persistence, three launch modes, re-populate on restart |
| Single-page dashboard | all admin on one page, no page-switch latency |
| Docker-in-instance (`_dini`) | operate Docker from inside an instance (`--privileged` + tmpfs config) |

### ✅ Stage 3: Network Isolation & Concurrency Safety

Make "multi-tenant" actually safe: instances are fully isolated from each other
and from the control plane; concurrent launches never collide.

| Deliverable | Content |
|---|---|
| Per-instance `/30` subnet | east-west attacks structurally impossible; control plane and instance networks separated |
| Host-port pool | `OW_HOST_PORT_START–END` allocation with conflict retry |
| flock cross-process arbitration | ports and `/30` subnets allocated via lockfiles, so any number of API processes can launch concurrently without conflict (`host_port.rs` / `instance_net.rs`) |
| runsc DNS fix | user-defined bridges break Docker's embedded resolver under runsc → `OW_DNS` injection + entrypoint rewrite |

### ✅ Stage 4: Group-based RBAC & Admin Surfaces

Upgrade permissions from "roles" to "groups" and round out the admin UI.

| Deliverable | Content |
|---|---|
| Flat group RBAC | five flags (`can_create_template` / `can_manage_users` / `can_manage_group_instances` / `can_manage_docker` / `can_manage_registry`) + template whitelist + instance ceiling; effective context recomputed per request from the DB |
| System groups | Admin / Manager / User system groups (fixed kind, pinned flags) |
| Template visibility | `public` / `private` / `hidden` + group whitelist (no admin whitelist bypass) |
| Instance ceiling | group `max_instances` + personal `direct_max_instances` → effective ceiling, precise `FOR UPDATE` counting |
| Host instance limit | global cap in `admin_settings` (`host_instance_limit`, admins count too) |
| Admin UI | Groups / Users / Volumes / Settings tabs, password change, logout |
| Template whitelist UI | group ↔ template association editing |

### ✅ Engineering Quality Gates

Not a product stage — a developer-experience / engineering-discipline milestone
(`.scratch/archive/quality-gates/`).

| Deliverable | Content |
|---|---|
| Rust Clippy hard gate | `check.sh` and the `apps/api` `check` both run `cargo clippy --all-targets --all-features -- -D warnings`; all existing warnings fixed (no `#[allow]`) |
| Forbid unsafe | `#![forbid(unsafe_code)]` at crate roots; `set_var` tests converted to injected sources |
| Soft analysis reports | `analysis:rust` (too_many_lines 100 / cognitive_complexity 25), `analysis:unsafe` (cargo geiger), `analysis:bloat` (cargo llvm-lines) — exit 0 |
| Web lint | `apps/web` ESLint flat config; `lint` / `check` / `analysis:web` |
| Standalone E2E | `e2e/` Playwright suite (smoke + full), root `test:e2e` / `test:e2e:full` |
| Turbo fix | `turbo.json` declares the `test` task; `pnpm test` restored |
| No-sudo dev | `pnpm run dev:nosudo` (skips gVisor registration + `network:allow`) |

### ✅ Security Fuzzing (API fuzzing)

Not a product stage — a security-engineering milestone
(`.scratch/archive/security-fuzzing/`).

| Deliverable | Content |
|---|---|
| Code-first OpenAPI | 17 safe endpoints annotated with `#[utoipa::path]` + build-time export to the committed `security/openapi.json` (not exposed at runtime) + drift-check unit test |
| Two-pass Schemathesis | `pnpm run security:api`: admin session (schema-conformant, never 5xx) + fuzz-user session (`admin-gated` never 2xx; RBAC/BOLA/IDOR boundary); fixed seed (20260101) for reproducibility |
| Custom RBAC check | `admin_gated_boundary` hook (`OW_ENFORCE_RBAC` gating); mutation test proves it is not a no-op |
| Self-built fuzz image | `ow-schemathesis` (official `:stable` has a broken tracecov); no host Python / pipx |
| Caught-and-fixed by the fuzzer | login missing 400 declaration (JSON syntax error); disabled unexpected-method probing (it once deleted the admin via `DELETE /api/users/{id}`) |

### ✅ Admin Protection (admin cannot be deleted / demoted)

Not a product stage — an RBAC security-hardening milestone
(`.scratch/archive/admin-protection/`) — closes the "admin can be deleted"
product hole the fuzzer found, and makes the harness impossible to silently
break.

| Deliverable | Content |
|---|---|
| `delete_user` guard | target is an Admin system-group member (`kind='admin'`, not a hardcoded username) → 403; covers self-delete, admin-deletes-admin, non-admin-deletes-admin |
| `update_user` guard | target currently in Admin and payload carries `group_ids` → 403 (any Admin membership rewrite is rejected: adding the Admin group id is blocked by existing `validate_assignable_groups`, removal by the new guard); shares one `load_user_tier` with the existing tier-upgrade guard, no fail-open |
| UI mirror | `is_admin` rows hide Delete, keep Edit; the policy dialog for admin rows shows a "membership protected" notice and omits `group_ids` (ceiling still editable) |
| Pre-flight config guard | harness verifies `schemathesis.toml`'s `[phases.coverage]` still has an active `unexpected-methods = []` (section-scoped, blocks relocation bypass), otherwise refuses to run |
| Post-run integrity check | snapshots admin existence + `is_admin` + templates/instances row counts, compares after the run; pass-failures still run, unreadable counts count as failure; a mismatch `die`s naming the damaged resource |
| Mutation verification | removing/moving a config line → pre-flight `die`; directly deleting the dev DB's admin row → post-run `die` naming the admin; all green after restore |

### ✅ Production Benchmark (CPU/RAM benchmark of the production stack)

Not a product stage — an operational-observability milestone
(`.scratch/archive/production-benchmark/`) — the first reproducible answer to
"how much does the platform itself and each instance actually consume".

| Deliverable | Content |
|---|---|
| Pure-bash benchmark script | `scripts/benchmark/benchmark-prod.sh`: preflight → host-before → compose up → platform window → 6-instance concurrent window → teardown; `--phase` / `--smoke` / `--seconds` / `--out` |
| Pure-function library | `scripts/benchmark/benchlib.sh`: `/proc` sampling, `docker stats` JSON parsing, CSV, peak/average aggregation, Markdown tables — fixture unit-tested (no Docker) |
| Four-table report | platform-container peaks, per-instance peaks (remote type × runtime), runC vs runsc aggregation, host before→after delta + provenance (timestamp / default runtime / compose commit / image digests) |
| Live E2E smoke | `scripts/benchmark/smoke_test.sh`: short-window full pipeline, verifying platform health, 6 instances running, report produced, host clean after teardown (incl. DB row re-check) |

### ✅ Resource Monitoring Dashboard (Monitor tab)

Not a whole stage — the first Stage-5 item to land
(`.scratch/archive/monitor-dashboard/`). It replaces the admin-only Monitor
placeholder with an operator view of "what is happening on the box".

| Deliverable | Content |
|---|---|
| Host cards | CPU / RAM / Disk with current value + 1-hour native-SVG sparkline |
| Active Instances table | running/starting/paused rows: owner, template, runtime badge, uptime, CPU % / RAM (value + sparkline); paused greyed with `[paused]` badge; stopped/errored excluded; sortable columns |
| 1h / 24h range toggle | 15 s fine-grained (1 h) vs five-minute mean+peak aggregates (24 h) |
| Background sampler | `health_worker` tick reuse (every 5th tick, 15 s): `/proc` host parsers + one-shot `docker stats` per active instance via new `DockerService::container_stats()`; fail-open |
| In-memory `MetricsStore` | two-tier ring buffer (240 × 15 s + 288 × 5 min), nothing persisted, ~2 MB per 100-instance box |
| RBAC flag `can_view_monitoring` | sixth flat group flag (Admin/Manager on by default, User off), gates the tab and the snapshot endpoint, checkbox in the group editor |
| E2E | `e2e/tests/monitor.full.spec.ts` — real instance, live sparklines, 24h re-fetch, paused badge, RBAC boundary |

### ✅ Monitor Dashboard Optimization (interactive time-series)

Operator-view iteration on the Monitor tab
(`.scratch/archive/monitor-dashboard-optimization/`). Replaces the static
sparklines and the 1h / 24h toggle with a single 24-hour interactive time
axis that auto-switches resolution as you zoom.

| Deliverable | Content |
|---|---|
| Snapshot payload | timestamped two-tier points `{ t, v }` per metric (`*_fine` / `*_coarse`); endpoint + `can_view_monitoring` gating unchanged |
| Interactive charts | hand-written SVG `TimeSeriesChart` (no library): hover crosshair + value/time readout, click-to-pin, drag-select with live avg/max/min stats + auto-zoom, follow / "back to now", 1 h fine-data boundary marker |
| Host cards | CPU / RAM / Disk enlarged to full interactive charts (~180 px tall, 3-across) |
| Instance detail modal | per-row Detail button opens CPU + memory interactive charts from the already-fetched snapshot (no extra request); close via overlay / × / Esc |
| Row sparklines | light hover tooltip + click-to-pin on the existing `Sparkline` |
| Pure chart math | `apps/web/src/lib/chart/` module (time↔x, merging, nearest-point, zoom clamping, follow state) — DOM-free and unit-tested |
| E2E | `e2e/tests/monitor.full.spec.ts` — live interactive host charts, drag-zoom + back-to-now, detail modal, paused badge, RBAC boundary |

### ✅ Observability & Logs (audit trail + on-demand container logs)

Stage 5's observability trio
(`.scratch/archive/observability-logs/`): a queryable audit trail of administrative
and security events, an on-demand container-log viewer streamed live from
Docker, and bounded log rotation for instance and control-plane containers.

| Deliverable | Content |
|---|---|
| Audit trail | `audit_logs` table (migration `000024`) recording auth events, instance lifecycle, template / group / user / registry / settings edits, and authenticated 403s; async best-effort bounded channel → batching writer → graceful-shutdown flush; 90-day retention pruned daily from the health worker |
| RBAC flag `can_view_audit_logs` | seventh flat group flag (Admin/Manager on by default, User off), gates the Logs tab + query endpoint, checkbox in the group editor |
| Audit query endpoint + Logs panel | keyset-paginated (newest-first) filterable viewer with actor / action / target / outcome / IP / time, redacted before/after diffs on edit events (sensitive fields `[REDACTED]`, URL userinfo stripped), joins the 20-endpoint fuzz surface |
| On-demand container logs | `GET /api/instances/{id}/logs` status-aware SSE (`mayControlInstance` scope): tail 200 + follow, `end` event with reason (stopped / paused / deleted / eof), prompt upgrade for quiet containers, active end on status change |
| Instance log bounds | `json-file` `max-size=5m` `max-file=3` (~15 MB per instance) baked into the container config; control-plane logs rotated by compose (`max-size=10m` `max-file=3`) |
| E2E | `e2e/tests/observability.full.spec.ts` — audit rows render + filter, real-instance logs tail/follow, RBAC boundary, SSE end states |

### ✅ Log UI redesign (Audit Logs page + Container Log modal)

Post-observability frontend polish (`.scratch/log-ui-redesign/`): both log
surfaces got a structural layout/interaction overhaul while keeping the dark
glassmorphism + zinc/indigo language — no backend change. `feature/log-ui-redesign`
(`fcd7b12` + follow-up fixes).

| Deliverable | Content |
|---|---|
| Audit filter bar | six filters in an `auto-fit` CSS grid (dates paired), Apply / Clear + entry count on a separate right-aligned action row; shared `.filter-bar` styles so the Sessions filter bar inherits the same alignment |
| Audit table | compact `YYYY-MM-DD HH:MM` timestamps with the full locale string on hover; first-column minimum width lifted; IP column hidden via a `matchMedia`-driven class below ~900 px (markup stays for screen readers) |
| Diff expansion | dedicated native chevron button in the Event cell (`aria-expanded` / `aria-controls`, keyboard-operable); row body no longer clickable; diff renders only before/after-shaped fields |
| Log modal follow | pinned-to-bottom autoscroll via a pure helper (`shouldAutoscroll`): scroll-up pauses, back-to-bottom resumes; indicator shows streaming / paused / static; follow-off is labeled static |
| Line rendering | line numbers + stdout blue / stderr red left-gutter stripes (replacing O/E letterboxes); Wrap toggle default-on switching to `white-space: pre` + horizontal scroll |
| Size & type | default `min(900px, 92vw)` × `min(82vh, …)`, fullscreen toggle, truncated header title, A−/A+ font size 12–16 px persisted under one shared key |
| Tests | `log-helpers` / `logs-panel` / `container-log-panel` vitest files (pure-helper + component tests) — full web suite 381 tests green, `svelte-check` 0 errors/warnings, eslint + `analysis:web` clean |

---

## In-progress / Planned Stages

### 🔵 Stage 6: Reliability & Backup

| Item | Description | Priority |
|---|---|---|
| Persistent-data backup/snapshot | periodic backup of `server-pgdata` and user home dirs; simple restore flow | High |
| Orphaned-folder cleanup | remove persistent folders that no longer exist in the DB (the existing "Thorough Cleanup" in the UI) | Medium |
| Graceful shutdown/startup | restore instance state, rebuild routes, re-declare volumes after reboot | Medium |
| Health self-checks | aggregate health endpoints for API/Traefik/DB for external monitoring (uptime checks) | Low |
| Per-group/user resource quotas | beyond instance count, add group-level CPU / memory / GPU quotas | Medium |

### 📋 Stage 7: Identity & Security Hardening

| Item | Description | Priority |
|---|---|---|
| Login failure lockout | temporarily lock an account after consecutive failures (brute-force protection) | High |
| 2FA (TOTP) | users enable one-time passwords | Medium |
| SSO (OIDC / LDAP) | integrate with existing enterprise identity providers (not a baseline commitment; optional roadmap item) | Low |
| Password policy | strength requirements, periodic rotation reminders | Low |

### 📋 Stage 8: Multi-host & Clustering

> Consistent with the mission: this is "optional in the future", not a promise.
> Multi-host must not break the single-host experience.

| Item | Description | Priority |
|---|---|---|
| Multi-host scheduling | one control plane manages multiple hosts; instances can be assigned to / migrated between hosts | High (within the cluster) |
| Tailscale mesh | a secure overlay network between hosts with unified routing | Medium |
| Distributed state | per-instance route/resource allocation moves into dedicated tables (lifting the single-process cache and in-memory-state ceiling) | Medium |
| Host failover | instance recovery strategy when a host fails | Low |

### 💡 Ideas (unscheduled, just recorded)

- **GPU quotas** — allocate GPU count and types per group.
- **Template marketplace/sharing** — export/import template configs, share
  across hosts.
- **Instance snapshots/rollback** — point-in-time snapshots of persistent data.
- **WebRTC low-latency** — replace/strengthen the current WebSocket transport
  encoding.
- **Mobile adaptation** — mobile-browser optimization of the dashboard and
  viewers.
- **Automated E2E (Playwright + live containers)** — move the existing E2E
  setup into CI instead of manual runs.

---

## Completed Features Checklist

> Cross-references the core features in [mission.md](mission.md), confirming
> status item by item.

### Interfaces
- ✅ KasmVNC desktop (HTML5 Canvas + WebSocket)
- ✅ Jupyter Lab
- ✅ ttyd terminal

### Security & isolation
- ✅ per-instance access token (127 chars)
- ✅ per-instance `/30` subnet (east-west isolation)
- ✅ gVisor (runsc) per-template sandbox + NVProxy GPU passthrough
- ✅ JWT cookie + ForwardAuth
- ✅ server-side Basic injection (browser never sees secrets)
- ✅ group-based RBAC (flags + whitelist + ceiling, recomputed per request)
- ✅ API security fuzzing (Schemathesis two-pass, `admin-gated` RBAC boundary,
  mutation-verified)
- ✅ admin cannot be deleted / demoted (API guards + UI mirror + fuzz-harness
  integrity snapshots)

### Resource governance
- ✅ Auto-sleep (run-time limit + timeout_action)
- ✅ Keep time (idle reclamation + focus heartbeat)
- ✅ Bandwidth shaping (tc/HTB)
- ✅ Instance ceiling (group + personal + host-global)

### Persistence
- ✅ whole-home persistence
- ✅ three launch modes (use / no / reset)
- ✅ delete keeps data, restart re-populates

### Admin
- ✅ single-page dashboard
- ✅ group/user/ceiling management
- ✅ template visibility + whitelist
- ✅ password change and logout

---

## Definition of Done (per stage)

Before any stage can be declared complete, all of the following must hold:

1. **Features**: every deliverable in the stage is actually operable from the
   browser, not merely present in the API.
2. **Tests**: `cd apps/api && bash scripts/check.sh` (zero warnings) + `bash
   scripts/run_tests.sh` (nextest, Docker); `cd apps/web && pnpm check && pnpm
   test` (310 tests) all green.
3. **Docs**: the corresponding docs (architecture / api-reference / rbac /
   frontend / mission / tech-stack) are updated in sync, with no stale
   statements.
4. **Deployment**: `pnpm run docker:up` successfully deploys on a clean host and
   creates the first instance.
5. **Security**: new features do not break zero-trust isolation (subnet,
   credentials, and RBAC layers still hold).

---

## Related docs

- [mission.md](mission.md) — mission and core features (constitution, article one)
- [tech-stack.md](docs/developer-guide/tech-stack.md) — technology decisions, deployment & updates (constitution, article two)
- [docs/user-guide/architecture.md](docs/user-guide/architecture.md) — system architecture and DB schema
