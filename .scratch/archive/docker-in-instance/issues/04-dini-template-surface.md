# 04 — DinI Template Surface (DB + API + UI)

**What to build:** Workspace managers can toggle "Enable Docker in Instance (DinI)" on a template. The value persists, round-trips through the template API, and is editable in the template form — where enabling it shows a sandbox indicator for `runsc` and a red high-risk warning for `runc`. A template with DinI off is indistinguishable from today.

**Blocked by:** 01 — Port-Pool Networking for Instances (migration ordering)

**Status:** done

- [x] Templates store `docker_in_instance`, defaulting to `false`.
- [x] Template create, read, and update accept and return `docker_in_instance`.
- [x] The template form has a single DinI toggle that serializes into the API payload and deserializes on edit.
- [x] With DinI on: `runsc` shows a sandbox-protection indicator; `runc` shows a high-risk warning before/while enabling.
- [x] Seam 4 DB and Seam 5 frontend tests are green (field default/persist, round-trip serialization, warning states).

## Notes

- Migration `m20260802_000014_add_docker_in_instance` adds `docker_in_instance BOOLEAN NOT NULL DEFAULT false` to `workspace_templates` (down: `DROP COLUMN`).
- Repo `create`/`update` bumped to 21 args (final `docker_in_instance: bool`); routes accept it with `#[serde(default)]` and `template_to_json` returns it.
- API tests: 3 new Seam-4 DB tests + 4 round-trip tests in `templates_test.rs`. Full suite: 430/430 passed twice, `check.sh` clean.
- Frontend: `docker_in_instance` added to `Template`, `dockerInInstance` to `TemplateFormState`/`createDirtySnapshot`; `TemplateAdvanced.svelte` gains the toggle + conditional indicators (`runsc` → "Sandboxed via gVisor", else → high-risk warning). `pnpm test` 154/154, `pnpm check` 0 errors.
