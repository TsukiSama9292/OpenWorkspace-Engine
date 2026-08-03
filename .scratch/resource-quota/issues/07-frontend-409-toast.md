# 07 — Frontend 409 quota toast/modal

**What to build:** the client surfaces quota refusals understandably. When the API returns `409 Conflict` with the structured `quota` body, the frontend renders a human-readable error toast/modal built from the payload — which scope was hit (`user_instance`, `user_cpu`, `user_ram`, `host_instance`, `host_dedicated_cpu`, `host_dedicated_ram`, `host_shared_cpu`, `host_shared_ram`) plus current / limit / requested — so the user knows why the launch was refused and what to do about it. From a user's perspective: after this ticket, hitting a limit shows a clear, structured message instead of a generic failure.

**Blocked by:** 06 — Wire pre-flight into launch & start.

**Status:** ready-for-agent

- [ ] The client detects the `quota` payload on a `409` and renders a structured toast/modal from `error` + `quota` fields.
- [ ] Quota refusals on both launch and restart surface the same message treatment.
- [ ] The message names the limit and the current usage so the user can act (stop an instance or request a raise).
- [ ] Frontend tests cover rendering for each `scope` and a `409` without a `quota` body (falls back to the generic error path).
