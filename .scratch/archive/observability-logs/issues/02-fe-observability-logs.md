# 02 — Audit Logs Page + Container Log Panel (frontend)

**Track:** frontend

**What to build:** The UI half of Stage-5 observability, built against the backend contracts delivered by `01-be`: the empty `#logs` placeholder becomes a filterable, paginated audit-trail viewer; each instance gains a Logs button opening a live container-log panel; the new RBAC flag is editable in the group editor and gates the Logs tab. After this ticket, `svelte-check` and the Vitest suite are green with no new dependencies.

**Blocked by:** `01-be-observability-logs`

**Status:** completed

- [x] Contract types and API client: `EffectiveContext` / `Group` gain `can_view_audit_logs`; audit-query and container-log client helpers built against the `01-be` contracts. Follow the repo's existing convention: types are **hand-written in `src/lib/types.ts`** (there is no OpenAPI codegen in this repo — `can_view_monitoring` was added by hand) and kept in sync with the API's `/auth/me` payload by hand, mirroring how the monitoring flag was added.
- [x] `permissions.ts` gains `mayViewAuditLogs(ctx)` mirroring the monitoring helper; the Logs sidebar item and tab swap `$isAdmin` for `mayViewAuditLogs`.
- [x] Group editor: `can_view_audit_logs` checkbox following the existing flag-checkbox pattern (Admin pinned on, User pinned off, Manager editable, defaulted on).
- [x] `LogsPanel` replacing the placeholder: filter bar (event-type select, actor / target text inputs, outcome select, date range), newest-first list, "load more" via the cursor; rows show actor, action, target, outcome, IP, time; edit events expand to show the redacted before/after diff. The event-type filter options mirror the API's actual emitted vocabulary (no dead options like `instance.restart` / `registry.create` / `registry.delete`).
- [x] Pure `src/lib/logs/ansi.ts` module: SGR parser converting common ANSI sequences (fg/bg color, bold, reset, line controls) to HTML spans, unknown sequences stripped — DOM-free and unit-tested, mirroring the `src/lib/chart/` style.
- [x] Per-instance `Logs` button (visibility matching the endpoint's `mayControlInstance` authorization) opening a `ContainerLogPanel`: tail 200 lines, follow toggle streaming via the SSE endpoint using the native fetch streaming reader, ANSI rendered through `ansi.ts`, clear "session ended" state with reason (stopped / paused / deleted / eof), closing the panel aborts the stream. The SSE stream parser MUST maintain a partial-line buffer across reader chunks (`TextDecoder` with `stream: true` + a pending-buffer, never `decode(chunk).split('\n')`), so split SSE lines and multi-byte UTF-8 sequences crossing chunk boundaries parse cleanly. Non-200 responses from the fetch (401 / 403 / 404) are surfaced as errors from the pre-flight check, not parsed as stream data.
- [x] Acceptance: web `pnpm check` green, web `pnpm test` green, `pnpm run analysis:web` report no regressions; layout follows the existing design language.
