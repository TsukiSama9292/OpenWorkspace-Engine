# 13 — Frontend contract cleanup

**Track:** frontend

**What to build:** The final alignment of the web app with the dropped legacy columns. Any remaining references to `role`, per-user quota fields, and `allocation_mode` are removed from types, stores, components, and tests, and the frontend types exactly match the post-contract API responses. `svelte-check` and the Vitest suite pass cleanly.

**Blocked by:** 11-fe-orphaned-volumes

**Status:** ready-for-agent

- [ ] No `role`, quota-field, or `allocation_mode` references remain in the web app
- [ ] Frontend types match the post-contract API responses
- [ ] `pnpm check` and `pnpm test` are fully green
