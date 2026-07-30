# 02 — API expose & repository persistence

**What to build:** The REST API exposes `container_runtime` on template CRUD endpoints, and the repository persists it to and from the database. After this ticket, any client (curl, frontend, script) can set and read the runtime for each template.

**Blocked by:** 01 — Schema, Settings & pure function

**Status:** ready-for-agent

- [ ] Add `container_runtime: String` (default `"docker"`) to `CreateTemplateRequest` in `templates.rs`
- [ ] Add `container_runtime: String` to `UpdateTemplateRequest` in `templates.rs`
- [ ] Thread `container_runtime` through `WorkspaceTemplateRepository::create()` — accept and store the field
- [ ] Thread `container_runtime` through `WorkspaceTemplateRepository::update()` — accept and update the field
- [ ] Ensure `template_to_json()` serializes `container_runtime` in all template responses
- [ ] API integration tests in `templates_test.rs`: POST creates with `container_runtime`, GET returns it, PUT updates it, null/omitted defaults to `"docker"`
- [ ] Repository integration tests in `db_test.rs`: create with explicit `container_runtime`, update changes it, find returns correct value
- [ ] Entity `From` impl tests: `Some("runsc")` and `None` both produce correct `WorkspaceTemplate.container_runtime`
