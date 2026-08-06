Status: completed

# Container Runtime Selection for Workspace Templates

## Problem Statement

Workspace containers are currently created without specifying a Docker runtime, meaning Docker's default runtime (`runc`) is always used. Administrators who want to run certain workloads under gVisor (`runsc`) for stronger sandboxing have no way to configure this — the runtime is invisible at every layer: env var, database, API, and UI.

The platform is used to share one Linux box among multiple developers, so sandboxing quality matters. Without runtime selection, all containers have equivalent isolation, and there is no way to opt particularly sensitive or untrusted workloads into a stronger sandbox without forking infra.

## Solution

Add a per-template `container_runtime` field, backed by an `OW_CONTAINER_RUNTIME` environment variable on the API server that serves as the system-wide default. Templates specify either `"docker"` (don't set a Docker runtime — use the daemon default, typically `runc`) or `"runsc"` (use gVisor). The frontend provides a dropdown in the advanced form section, and the Rust API passes the value through to Docker's `HostConfig.runtime`.

## User Stories

1. As an **infrastructure admin**, I want to set `OW_CONTAINER_RUNTIME=runsc` on the API server, so that all workspace containers use gVisor by default without touching each template.
2. As a **workspace manager**, I want to override `container_runtime` on a specific template to `"runsc"`, so that high-sensitivity workloads get stronger sandboxing even when the system default is `"docker"`.
3. As a **workspace manager**, I want the dropdown to offer "Default" and "runsc" options, so that I don't need to know Docker runtime names by heart.
4. As a **workspace manager**, I want `container_runtime` to appear in the advanced section of the template form, so that the main form stays uncluttered.
5. As a **workspace manager**, I want `container_runtime` to appear in template create and edit forms, so that I can set it at creation time and change it later.
6. As an **API consumer**, I want `container_runtime` included in the JSON response for every template, so that I can audit runtime settings across all templates.
7. As an **API consumer**, I want to send `container_runtime` when creating or updating a template, so that I can manage runtime programmatically.
8. As a **platform developer**, I want the runtime-to-HostConfig mapping to be a pure function, so that it is trivially testable without Docker or a database.
9. As a **platform developer**, I want the env var parsing to follow the same pattern as existing settings, so that the codebase stays consistent.

## Implementation Decisions

### 1. Environment variable
- Name: `OW_CONTAINER_RUNTIME`
- Default value: `"docker"`
- Parsed in `Settings::from_env()` alongside existing fields like `DOCKER_NETWORK`
- Value `"docker"` means "do NOT set `HostConfig.runtime` — let the Docker daemon use its own default"

### 2. Database schema
- Column: `container_runtime VARCHAR(64) NOT NULL DEFAULT 'docker'`
- Added to `workspace_templates` table via a new SeaORM migration (sequence `000009`)
- Stored as a plain varchar, not in the `run_config` JSON blob — it is a first-class template property

### 3. Rust data model

**`ContainerConfig` struct** gains a new field:
```
pub runtime: Option<String>,
```
- `None` → use `"docker"` behavior (omit HostConfig.runtime)
- `Some("runsc")` → set HostConfig.runtime to `"runsc"`
- `Some("docker")` → omit HostConfig.runtime (explicit override to daemon default, distinct from "inherit env var")

**`WorkspaceTemplate` (public type)** gains:
```
pub container_runtime: String,
```
Default `"docker"` in the public struct, mapped from the SeaORM entity.

**`Settings` struct** gains:
```
pub container_runtime: String,
```
Default `"docker"`, read from env var `OW_CONTAINER_RUNTIME`.

### 4. API contract
- `POST /api/templates` — accepts optional `container_runtime` (defaults to `"docker"` if omitted)
- `PUT /api/templates/{id}` — accepts `container_runtime`
- `GET /api/templates` and `GET /api/templates/{id}` — response includes `"container_runtime": "docker"|"runsc"`

### 5. Runtime resolution (who wins)

```
template.container_runtime
  if set and non-empty  →  use it directly
  if empty/unset        →  use settings.container_runtime (the env var)
```

The caller (`instances.rs`) resolves this when constructing `ContainerConfig`:
```
container_config.runtime = if template.container_runtime.is_empty() {
    Some(state.settings.container_runtime.clone())
} else {
    Some(template.container_runtime.clone())
}
```

Then in `create_container_from_template`:
```
host_config.runtime = match config.runtime.as_deref() {
    None | Some("docker") => None,      // omit runtime → daemon default
    Some(other) => Some(other.to_string()),
}
```

### 6. Pure helper function
```
fn runtime_to_host_config(value: &str) -> Option<String>
```
Encapsulates the "docker/empty → None, runsc → Some('runsc')" mapping. Lives in `docker.rs`.

### 7. Frontend
- `TemplateFormState` gains `containerRuntime: string` (default `""`)
- Dropdown with two options: "Default" (value `""`) and "runsc" (value `"runsc"`)
- Placed in the "Advanced" section of the form, near `networkMode`
- `submitTemplate()` sends `container_runtime` as a top-level field (not inside `run_config`)
- `Template` TypeScript type gains `container_runtime: string`

## Testing Decisions

### What makes a good test
- Test the external behavior: what runtime ends up in `HostConfig`, not which internal fields get set
- Test the mapping function as a pure input/output transform
- Test that the API exposes the field correctly in JSON
- Test that the DB persists and retrieves the field correctly

### Test seams (preferred order)

| Seam | File | Type | What it tests |
|---|---|---|---|
| 1 | `docker.rs` (`#[cfg(test)]`) | Pure function unit test | `runtime_to_host_config("docker") → None`, `runtime_to_host_config("runsc") → Some("runsc")`, `runtime_to_host_config("") → None` |
| 2 | `core/settings.rs` (`#[cfg(test)]`) | Settings unit test | `OW_CONTAINER_RUNTIME` default `"docker"`, custom value parsed, missing field uses default |
| 3 | `db_test.rs` (integration) | Repository integration test | `WorkspaceTemplateRepository::create()` with `container_runtime`, `update()` with new value, `find_by_id()` returns it |
| 4 | `db_test.rs` (integration) | Entity From impl test | `workspace_template::Model → WorkspaceTemplate` conversion includes `container_runtime` field for both `Some("runsc")` and `None` |
| 5 | `templates_test.rs` (integration) | API integration test | `POST /api/templates` response JSON includes `container_runtime`, `GET /api/templates/{id}` includes it, `PUT /api/templates/{id}` updates it |

### Prior art
- Settings env var tests: `settings.rs` lines 59–230
- Entity `From` impl tests: `db_test.rs` lines 679–806 (`config_model_from_converts_all_fields`, `config_model_from_null_optionals`)
- Repository tests: `db_test.rs` lines 147–321 (create, find, update, delete template)
- API template tests: `templates_test.rs` (create, get, list, update, delete)

## Out of Scope

- **Per-instance runtime override**: runtime is a template-level property, set once per workspace type. Instances inherit it from their template.
- **Runtime validation against Docker daemon**: if the Docker daemon doesn't have `runsc` registered, Docker itself will reject the creation. We don't pre-validate this at the API level.
- **Per-container runtime in `docker_raw.rs`**: The simple `POST /api/docker/containers/create` pass-through is unchanged — it is a raw Docker API proxy for debugging, not part of the workspace system.
- **Additional runtimes beyond `runsc`** (e.g., `kata`, `nvidia`): the dropdown only offers `"Default"` and `"runsc"`, but the env var and API accept any string, so arbitrary runtimes can be set directly via the API or env var.
- **kasmvnc.yaml `runtime_configuration` changes**: The existing KasmVNC YAML config has a `runtime_configuration` section (for VNC servers, not Docker runtimes) — unrelated to this change.

## Further Notes

The value `"docker"` in this context means "do not specify a Docker runtime — let the Docker daemon use its own default." This is slightly ambiguous because Docker's default OCI runtime is named `runc`, but the term `"docker"` is used here because (a) user requested it, (b) it maps well to the concept "standard Docker behavior, no sandbox escape," and (c) Docker daemon administrators can configure a different default runtime in `/etc/docker/daemon.json` without our system fighting them.
