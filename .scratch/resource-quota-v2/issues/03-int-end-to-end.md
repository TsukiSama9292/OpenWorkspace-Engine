# 03 — End-to-end: quota lifecycle against the real stack

**Track:** integration

**What to build:** a full-stack Playwright test proving both sides work
together against a running dev stack with real Docker containers — the quota
feature's happy path and its rejections, end to end: launching into a billing
group, hitting each quota layer and reading the rendered rejection, managing
quotas in the UI, the `-1` unlimited convention round-tripping through
template / group / settings forms, and restart behaviour once a quota is
exhausted.

**Blocked by:** 02 — Frontend: quota UI (billing-group picker, layered Groups
tab, `-1` convention forms)

**Status:** completed (2026-09-12) — `e2e/tests/resource-quotas.full.spec.ts`
12/12 green against the booted dev stack (real containers). E2E caught one
real backend bug: route-level rejections hardcoded `"requested": 1` for every
resource scope — fixed (enum carries `requested` now; unit + integration
assertions updated). Selectors fixed (`.ws-billing` strict-mode), missing
`memberQuota` helper added, admin-launch used for the `-1`-into-finite-host
case (member/pool would bind first otherwise).

## Status — 2026-08-10

Not started. No E2E work has been done and no dev stack has been booted this
session. Backend (01) is **complete and green** (`check.sh` silent,
`run_tests.sh` 734/734), so the API surface the E2E suite will test is stable;
frontend (02) is untouched and must land first, since this suite drives the
browser UI. The full Playwright suite will run against the running dev stack
only once both tickets land (per the AGENTS.md `test:e2e:full` flow).

## Acceptance criteria (`e2e/tests/resource-quotas.full.spec.ts`, 12 tests, all green)

- [x] A multi-group user launches an instance through the UI, sees the billing
      picker defaulted to the highest-cap group, and the launched instance
      reports the chosen billing group and its resource usage.
      → `multi-group launch defaults the billing picker…` (green 2026-09-12).
- [x] A single-group user launches without a picker and the instance is
      attributed to their only group.
      → `single-group launch shows no picker…` (green 2026-09-12).
- [x] Setting a finite group pool below an in-flight request produces a `409`
      rendered as the structured quota rejection notice with readable
      numbers — one test per layer reachable via the UI (personal cap / group
      pool / host cap), including a memory scope showing formatted bytes.
      → `member-cap rejection…`, `group-pool rejection…`,
      `host memory-cap rejection…` (green 2026-09-12; memory case asserts `4 GB`).
- [x] A manager (non-admin) edits a lower-tier member's quota in the Groups
      tab and sees it take effect on the member's next launch attempt.
      → `manager edits a lower-tier member quota…` (green 2026-09-12).
- [x] An admin edits a group pool and uses the one-click member reset; the
      blocked-tightening `409` (active `-1` snapshot, or pool below a member
      quota) is asserted where exercised.
      → `admin pool edit below a member cap is refused…` (green 2026-09-12;
      covers pool-below-cap `409` + reset-all; `-1`-snapshot tightening is
      covered API-side in `instances_mock_test.rs`).
- [x] Exhausting a group pool and then stopping the violating instance allows
      a fresh launch to succeed; restarting a stopped instance once the pool is
      full again keeps it stopped with a quota rejection.
      → `stop releases quota and restart re-checks the pool` (green 2026-09-12).
- [x] Host caps configured in Admin Settings reject an over-cap launch with a
      host-scope `409` while an unlimited (`-1`) template request into a finite
      host cap is refused.
      → `host memory-cap rejection…` + `unlimited template request is
      refused…` (green 2026-09-12).
- [x] The `-1` convention round-trips through the UI: creating a template with
      `-1` cores/bandwidth (the unlimited toggle) persists and launches; an
      admin setting `-1` on a host cap / `host_instance_limit` and a group
      setting `-1` on its pool are saved and read back as unlimited; a `0`
      pool blocks launches through the pool editor.
      → `the -1 convention round-trips through the template form…` (create +
      persist + launch) + `a zero pool blocks launches…` (0-pool `409` +
      Settings-tab `-1` save/readback) (green 2026-09-12).
- [x] Suite runs against the booted dev stack (`pnpm run dev:nosudo`, launched
      via the documented `setsid` flow), tears down cleanly, and leaves no
      stray containers or routes.
      → `Quota E2E leaves no running instances behind…` + `afterAll` catalog
      assertions (green 2026-09-12).
