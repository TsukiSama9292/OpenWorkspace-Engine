# 03 — Fuzzer harness hardening + end-to-end verification

**Track:** integration

**What to build:** The fuzz harness can no longer silently self-destruct. `pnpm run security:api` gains a pre-flight guard that aborts if `schemathesis.toml` no longer disables unexpected-method probing (the exact configuration regression behind the 08:16 admin-deletion incident), and a post-run integrity check that snapshots admin integrity (the `admin` user exists and is a member of the Admin group) plus template/instance row counts before fuzzing and verifies them unchanged afterwards — failing loudly with a message identifying the damaged resource if anything moved. Both guards are provably live via mutation-style verification, and the whole flow stays green against the running dev stack on top of the backend and frontend tickets. *Hardened in code-review:* the pre-flight check is section-scoped to `[phases.coverage]` (closing the relocate-to-a-dead-section bypass), the post-run check always runs even when a pass fails hard, and an unreadable count endpoint surfaces as an integrity failure instead of an "N/A == N/A" silent pass.

**Blocked by:** 02 — Admin account & group protection: UI mirror

**Status:** completed

- [x] Pre-flight guard: the script dies with an actionable message if an active `unexpected-methods = []` is not inside `[phases.coverage]`, before any fuzzing runs
- [x] Post-run integrity check: admin exists and is `is_admin` before and after both passes; `workspace_templates` / `workspace_instances` counts unchanged; mismatch dies naming the damaged resource (admin missing / no longer admin / template count / instance count / unreadable state)
- [x] Mutation 1 (config): temporarily remove `unexpected-methods = []` from `schemathesis.toml` → script dies at the pre-flight guard; relocate it to a dead `[unused.*]` section → still dies; revert → green
- [x] Mutation 2 (state): snapshot, then delete the `admin` row directly in the dev DB mid-run → post-run check dies identifying admin; restore → green
- [x] `pnpm run security:api` passes both fuzz passes (seed 20260101) with the guards armed; dev DB left intact (admin still `is_admin`)
