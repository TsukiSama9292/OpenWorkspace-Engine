# 05 — Launch rejection UX and removal of quota / allocation-mode UI

**Track:** frontend

**What to build:** The launch flow renders the structured pre-flight rejections (a `403` template-not-allowed and a `409` ceiling violation with its numbers) instead of the legacy quota toast. The per-user quota modal and its fields (`instance_limit`, `max_cpu_cores`, `max_ram_bytes`) are removed, and `allocation_mode` is removed from the template form.

**Blocked by:** 03-fe-auth-store

**Status:** completed

- [ ] Whitelist and ceiling rejections render the structured reason and numbers from the response body
- [ ] Quota modal and per-user quota fields are gone from user management
- [ ] `allocation_mode` no longer appears in the template form; template form tests updated
- [ ] `pnpm check` and `pnpm test` stay green
