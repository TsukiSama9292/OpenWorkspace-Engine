# 03 — End-to-end: quota lifecycle against the real stack

**Track:** integration

**What to build:** a full-stack Playwright test proving both sides work
together against a running dev stack with real Docker containers — the quota
feature's happy path and its rejections, end to end: launching into a billing
group, hitting each quota layer and reading the rendered rejection, managing
quotas in the UI, and restart behaviour once a quota is exhausted.

**Blocked by:** 02 — Frontend: quota UI (billing-group picker, layered Groups
tab, `0`/`-1` forms)

**Status:** ready-for-agent

## Acceptance criteria

- [ ] A multi-group user launches an instance through the UI, sees the billing
      picker defaulted to the highest-cap group, and the launched instance
      reports the chosen billing group and its resource usage.
- [ ] A single-group user launches without a picker and the instance is
      attributed to their only group.
- [ ] Setting a finite group pool below an in-flight request produces a `409`
      rendered as the structured quota rejection notice with readable
      numbers — one test per layer reachable via the UI (personal cap / group
      pool / host cap), including a memory scope showing formatted bytes.
- [ ] A manager (non-admin) edits a lower-tier member's quota in the Groups
      tab and sees it take effect on the member's next launch attempt.
- [ ] An admin edits a group pool and uses the one-click member reset; the
      blocked-tightening `409` (active `-1` snapshot, or pool below a member
      quota) is asserted where exercised.
- [ ] Exhausting a group pool and then stopping the violating instance allows
      a fresh launch to succeed; restarting a stopped instance once the pool is
      full again keeps it stopped with a quota rejection.
- [ ] Host caps configured in Admin Settings reject an over-cap launch with a
      host-scope `409` while an unlimited (`-1`) template request into a finite
      host cap is refused.
- [ ] Suite runs against the booted dev stack (`pnpm run dev:nosudo`, launched
      via the documented `setsid` flow), tears down cleanly, and leaves no
      stray containers or routes.
