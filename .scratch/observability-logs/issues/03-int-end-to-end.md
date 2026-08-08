# 03 — End-to-End Observability Smoke (integration)

**Track:** integration

**What to build:** A full-stack Playwright test proving the audit trail and on-demand container logs work together against a real running dev stack — the backend contracts from `01-be` served through the UI built in `02-fe`. Verifies the happy path end to end plus the RBAC boundaries that protect the feature.

**Blocked by:** `02-fe-observability-logs`

**Status:** completed

- [x] Admin login → Logs tab visible → audit entries render (with actor, action, target, outcome, timestamp) and filters narrow the list.
- [x] Admin launches a real instance → opens its Logs panel → sees the container's stdout (tail) and live follow output; closing the panel stops the stream cleanly.
- [x] RBAC boundary: a user without `can_view_audit_logs` does not see the Logs tab and gets no audit data; a user cannot open logs of an instance they do not own.
- [x] SSE end states: opening logs on a stopped instance shows the tail plus the ended state with the stop reason instead of hanging.
- [x] Acceptance: `pnpm run test:e2e:full` green against a running dev stack (`pnpm run dev:nosudo`, launched via the setsid launcher per AGENTS.md), then clean teardown. 9/9 full-suite tests green (observability 5/5).

During the run two issues surfaced and were fixed: (1) the SSE client unconditionally
overrode the real `end` reason with `eof` after the stream closed — the running/stopped
end-state test caught it; fixed in `apps/web/src/lib/api/instance-logs.ts`. (2) The audit
row selector `.td-date` matched both Time and IP cells (strict-mode violation) — scoped to
`.td-date:not(.td-ip)`. Both verified against the running stack.
