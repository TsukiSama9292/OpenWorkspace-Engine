# 04 — Frontend dropdown

**What to build:** The create/edit template forms gain a "Runtime" dropdown in the Advanced section with two options: "Default" (inherits from system env var) and "runsc" (gVisor). The value is sent as a top-level API field, not inside `run_config`.

**Blocked by:** 02 — API expose & repository persistence

**Status:** completed

- [ ] Add `container_runtime: string` (default `""`) to `TemplateFormState` in `template-create.ts`
- [ ] Add `<select>` with options "Default" (`""`) and "runsc" (`"runsc"`) to the Advanced section of the create template form
- [ ] Add the same control to the edit template form
- [ ] `submitTemplate()` (or equivalent) sends `container_runtime` as a top-level POST/PUT body field, not nested in `run_config`
- [ ] Update `Template` TypeScript type in `types.ts` to include `container_runtime: string`
- [ ] Wire `container_runtime` from API response back into the form state when editing an existing template
