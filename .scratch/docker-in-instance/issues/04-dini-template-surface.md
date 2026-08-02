# 04 — DinI Template Surface (DB + API + UI)

**What to build:** Workspace managers can toggle "Enable Docker in Instance (DinI)" on a template. The value persists, round-trips through the template API, and is editable in the template form — where enabling it shows a sandbox indicator for `runsc` and a red high-risk warning for `runc`. A template with DinI off is indistinguishable from today.

**Blocked by:** 01 — Port-Pool Networking for Instances (migration ordering)

**Status:** ready-for-agent

- [ ] Templates store `docker_in_instance`, defaulting to `false`.
- [ ] Template create, read, and update accept and return `docker_in_instance`.
- [ ] The template form has a single DinI toggle that serializes into the API payload and deserializes on edit.
- [ ] With DinI on: `runsc` shows a sandbox-protection indicator; `runc` shows a high-risk warning before/while enabling.
- [ ] Seam 4 DB and Seam 5 frontend tests are green (field default/persist, round-trip serialization, warning states).
