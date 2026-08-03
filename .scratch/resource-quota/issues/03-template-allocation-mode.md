# 03 — Template `allocation_mode`: schema, API, form selector

**What to build:** templates express whether they are hard-reservation (`dedicated`) or overcommittable (`shared`). The `workspace_templates` table gains an `allocation_mode` column backfilled to `shared` for existing templates. Template create/update/list/single all carry the field. Only Admin may set `dedicated`; Manager template management is constrained to `shared` and gets `403` otherwise. Changing a template's mode while it has active instances (running / starting / paused) is rejected with `409`, so accounting can never mix old and new modes under one template. From the manager's perspective: after this ticket, they can mark heavy workloads as dedicated (Admin only) and see each template's mode everywhere it is shown, and the template form offers the choice.

**Blocked by:** None — can start immediately.

**Status:** ready-for-agent

- [ ] `workspace_templates.allocation_mode` exists with existing rows backfilled to `shared`.
- [ ] Template create/update/list/single responses include `allocation_mode`; create/update accept it.
- [ ] Only Admin can create/update a template with `allocation_mode = dedicated`; Manager requests return `403`; User has no template management.
- [ ] Updating `allocation_mode` on a template with any active instance returns `409` and does not change the stored value; it succeeds when no active instances remain.
- [ ] The template form offers `shared` / `dedicated`; the `dedicated` option is rendered only for Admin.
- [ ] API tests cover the mode permission boundary and the active-instance guard; frontend tests cover the selector round-trip and the Admin-only option.
