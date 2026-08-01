# 07 — Merged countdown overlay

**What to build:** the remote screen pages show a single countdown badge for the event that fires first. If the Instance has an auto-sleep deadline it shows as today; if it has only a keep-time deadline, the badge appears only while the tab is visible but not focused (while focused there is no idle countdown running). When the countdown hits zero the existing resync flow redirects the user to the dashboard when the Instance leaves `running`.

**Blocked by:** 06 — Instance deadline + frontend keepalive

**Status:** ready-for-agent

- [x] Overlay shows the sooner of `auto_sleeps_at` and `keep_time_deadline` (single badge, labelled with the correct action)
- [x] Auto-sleep deadline present → badge shows as today (focused or not)
- [x] Only keep-time deadline present → badge shows only while the tab is visible but not focused
- [x] Zero-hit resync/redirect flow covers keep-time expiry: `hadDeadline` set when a keep-time deadline exists; redirect to `/` when the Instance is no longer `running`
- [x] Overlay unit tests green (merge logic + visibility gating)

## Notes

Implemented and verified green.

**Files changed:** `apps/web/src/lib/countdown/countdown.ts`, `apps/web/src/lib/countdown/CountdownOverlay.svelte`, `apps/web/src/routes/kasmvnc/[token]/+page.svelte`, `apps/web/src/routes/open/[token]/+page.svelte`, `apps/web/src/tests/countdown.test.ts`.

**selectDeadline** — pure, exported helper in `countdown.ts`. Both deadlines present → earlier absolute instant wins; tie prefers `auto_sleeps_at`/`timeout_action`; unparseable value treated as absent; returns `{ deadline, action }` or `null`. `remainingMs`/`formatRemaining`/`severity` unchanged.

**Overlay contract** — props `{ auto_sleeps_at, timeout_action, keep_time_deadline, keep_time_action, onResync }`. `ResyncResult` carries the same four raw fields; the overlay recomputes the merge after each resync. Kept the 1s tick, 30s resync, resync-on-become-visible, and zero-hit resync (now keyed on the merged remaining). Gating: `show = selected && (auto_sleeps_at ? true : visible && !focused)`; `visible`/`focused` are reactive state initialized from `document` at instantiation (SSR-guarded) and updated via `visibilitychange` (document), `focus`/`blur` (window). Badge keeps `pointer-events-none`; "已到期" expired state and action label hidden when expired are preserved.

**Pages** — pass the four raw fields; `resyncDeadline` refetches and returns them (or `null`), redirecting to `/` on missing instance or when `hadDeadline && status !== 'running'`. `hadDeadline` is set when either `auto_sleeps_at` or `keep_time_deadline` is present. Keepalive (06) wiring untouched.

**Verification (verbatim):**
- `pnpm check` → `svelte-check found 0 errors and 0 warnings`
- `pnpm test` → `Test Files  9 passed (9)` / `Tests  128 passed (128)`
- `pnpm vitest run src/tests/countdown.test.ts` → `Test Files  1 passed (1)` / `Tests  36 passed (36)`

**Deviations:** none functional. Two implementation notes: (1) `visible`/`focused` initialize from `document` at component instantiation rather than inside `onMount` so the first render's gating is correct (equivalent on the client); (2) inside the `{#if show}` template the `remaining` null check is expressed as `{:else if remaining !== null}` because svelte-check does not narrow a derived boolean down to `remaining !== null`. One environment note: a concurrent process rewrote `countdown.ts` mid-session dropping ticket-06's `deadlineRemaining` and the new `selectDeadline`; both were re-added and re-verified.
