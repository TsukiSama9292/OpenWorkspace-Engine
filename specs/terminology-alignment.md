# Spec: Terminology Alignment — Config → Template, Workspace → Instance

## Problem Statement

The frontend and backend use different names for the same concepts, creating confusion for both users and developers:

- Backend calls it `config`, frontend UI calls it `template` — user clicks "+ New Template" but arrives at a page titled "New Config"
- Backend calls it `instance`, frontend UI calls it `workspace` — sidebar says "Workspaces" but section header inside says "Instances"
- Two sidebar tabs ("Workspaces" and "Instances") both show instance data, differing only in card vs table view
- The `Config` TypeScript interface uses field names (`cpu_cores`, `ram_bytes`) that don't match the API response (`cores`, `memory`), causing `undefined` values at runtime
- The `config_id` / `config_name` JSON fields in instance responses use the old terminology

## Solution

Rename all user-facing and internal references to use a consistent vocabulary:

| Concept | Canonical Name | Description |
|---------|---------------|-------------|
| Template | **Template** | A pre-configured settings bundle (image, resources, env vars) that users launch instances from |
| Instance | **Instance** | A running VNC container launched from a template |
| User | **User** | A person with an account (admin/manager/user roles) |

Sidebar tabs reorganized into 4 distinct views:

| Tab | Content | Visibility |
|-----|---------|------------|
| **Instances** | Personal instance cards (own only) | Everyone |
| **Templates** | Template cards with launch | Admin + Manager |
| **Sessions** | Full instance table (all users) with filters | Admin + Manager |
| **Users** | User table with CRUD | Admin + Manager |

## User Stories

1. As a user, I want the sidebar to say "Instances" so I know it shows my running instances
2. As a user, I want the sidebar to say "Templates" so I know it shows launchable templates
3. As a user, I want the sidebar to say "Sessions" so I know it shows the admin view of all instances
4. As a user, I want to click "+ New Template" and see a page titled "New Template" (not "New Config")
5. As a user, I want to see template resource info (cores, memory) on template cards without `undefined` values
6. As a user, I want the login page to say "access your personal instance" (not "workspace")
7. As a user, I want the instance detail page to show "Template ID" (not "Config ID")
8. As a user, I want the instance detail page to show the template name (not "config_name")
9. As a user, I want the VNC launch dialog to say "Choose how to open this instance" (not "workspace")
10. As a user, I want the dashboard subtitle to say "Pick a template to spin up a new instance" (not "workspace")
11. As a developer, I want the API to return `"templates"` (not `"configs"`) in list responses
12. As a developer, I want the API to return `"template"` (not `"config"`) in single-object responses
13. As a developer, I want the API to return `"template_id"` (not `"config_id"`) in instance responses
14. As a developer, I want the API to return `"template_name"` (not `"config_name"`) in instance responses
15. As a developer, I want the DB table to be called `workspace_templates` (not `workspace_configs`)
16. As a developer, I want the DB column to be called `template_id` (not `config_id`)
17. As a developer, I want the Rust struct to be called `WorkspaceTemplate` (not `WorkspaceConfig`)
18. As a developer, I want the API route to be `/api/templates` (not `/api/configs`)
19. As a developer, I want the frontend type to be called `Template` (not `Config`)
20. As a developer, I want the frontend action file to be called `template-actions.ts` (not `config-actions.ts`)
21. As a developer, I want the create form components to be called `TemplateBasics.svelte`, `TemplateResources.svelte`, `TemplateAdvanced.svelte`
22. As a developer, I want the frontend route to be `/templates/new/` (not `/configs/new/`)
23. As a developer, I want test files to be called `templates_test.rs` (not `configs_test.rs`)
24. As a developer, I want all test assertions to use the new terminology (`body["template"]`, `body["templates"]`)
25. As a developer, I want CSS classes to use `.instance-grid` (not `.workspace-grid`)
26. As a developer, I want the storage path hint to say `{template_name}` (not `{workspace_name}`)
27. As a developer, I want the API response field for cores to be `cores` (not `cpu_cores`) — matching the backend
28. As a developer, I want the API response field for memory to be `memory` (not `ram_bytes`) — matching the backend

## Implementation Decisions

### Decision 1: DB Table and Column Renames

**Tables:**
- `workspace_configs` → `workspace_templates`
- `workspace_instances` stays as `workspace_instances` (already correct)

**Columns in `workspace_instances`:**
- `config_id` → `template_id`

**Migration strategy:** Since this is a development-stage project with no production data, we will modify the existing migration `m20260723_000004_split_config_instance.rs` in-place rather than creating a new migration. This keeps the migration history clean.

### Decision 2: Rust Backend Renames

**Module:**
- `pub mod workspace_config` → `pub mod workspace_template`

**Structs:**
- `WorkspaceConfig` → `WorkspaceTemplate`
- `WorkspaceConfigRepository` → `WorkspaceTemplateRepository`
- `CreateConfigRequest` → `CreateTemplateRequest`
- `UpdateConfigRequest` → `UpdateTemplateRequest`

**Functions:**
- `config_to_json` → `template_to_json`
- `list_configs` → `list_templates`
- `create_config` → `create_template`
- `get_config` → `get_template`
- `update_config` → `update_template`
- `delete_config` → `delete_template`
- `create_container_from_config` → `create_container_from_template` (in `docker.rs`)

**JSON response keys:**
- `"configs"` → `"templates"`
- `"config"` → `"template"`
- `"config_id"` → `"template_id"`
- `"config_name"` → `"template_name"`

**Route paths:**
- `/api/configs` → `/api/templates`
- `/api/configs/{id}` → `/api/templates/{id}`

**File rename:**
- `apps/api/src/routes/workspace/configs.rs` → `apps/api/src/routes/workspace/templates.rs`

**What NOT to rename:**
- `ContainerConfig` struct in `docker.rs` (Docker container config, not workspace template)
- `run_config`, `exec_config` fields (Docker run/exec config, not workspace template)
- KasmVNC YAML string `allow_environment_variables_to_override_config_settings`

### Decision 3: Frontend TypeScript Renames

**Types:**
- `Config` → `Template`
- `ConfigFormState` → `TemplateFormState`

**Functions:**
- `submitConfig` → `submitTemplate`
- `deleteConfig` → `deleteTemplate`
- `launchInstance(configId)` → `launchInstance(templateId)` (param name only)

**API paths in code:**
- `'/configs'` → `'/templates'`
- `` `/configs/${configId}` `` → `` `/templates/${templateId}` ``

**File renames:**
- `apps/web/src/lib/api/config-actions.ts` → `apps/web/src/lib/api/template-actions.ts`
- `apps/web/src/routes/configs/new/+page.svelte` → `apps/web/src/routes/templates/new/+page.svelte`
- `apps/web/src/routes/configs/new/config-create.ts` → `apps/web/src/routes/templates/new/template-create.ts`
- `apps/web/src/lib/components/forms/ConfigBasics.svelte` → `apps/web/src/lib/components/forms/TemplateBasics.svelte`
- `apps/web/src/lib/components/forms/ConfigResources.svelte` → `apps/web/src/lib/components/forms/TemplateResources.svelte`
- `apps/web/src/lib/components/forms/ConfigAdvanced.svelte` → `apps/web/src/lib/components/forms/TemplateAdvanced.svelte`

### Decision 4: Frontend UI String Renames

**Login page:**
- "access your personal workspace" → "access your personal instance"
- "Sign in to access your workspace" → "Sign in to access your instance"

**Dashboard:**
- "Pick a template to spin up a new workspace." → "Pick a template to spin up a new instance."
- "Loading workspaces..." → "Loading instances..."
- "Choose how to open this workspace." → "Choose how to open this instance."
- "No templates yet. Create one to get started." → stays (already says "templates")

**Sidebar tabs:**
- "Workspaces" → "Instances"
- "Instances" (admin tab) → "Sessions"
- "Templates" stays
- "Users" stays

**Instance detail page:**
- "Config ID" → "Template ID"
- `{instance.config_id}` → `{instance.template_id}`
- `instance.config_name || 'Unknown config'` → `instance.template_name || 'Unknown template'`

**Create form page:**
- "New Config" → "New Template"
- "Create Config" → "Create Template"

**CSS classes:**
- `.workspace-grid` → `.instance-grid`

**Storage path hint:**
- `{workspace_name}` → `{template_name}`

### Decision 5: Frontend TypeScript Interface Field Fix

The `Config`/`Template` interface must match the actual API response shape:

```typescript
export interface Template {
    id: string;
    name: string;
    image: string;
    cores: number;        // was cpu_cores — must match API response
    memory: number;       // was ram_bytes — must match API response (bytes)
    // ... other fields
}
```

The `Instance` interface field renames:
```typescript
export interface Instance {
    // ...
    template_id: string;    // was config_id
    template_name: string;  // was config_name
    // ...
}
```

### Decision 6: Permission Method Naming

In `auth.rs`:
- `can_manage_templates()` → rename to `can_manage_templates()` (already correct, matches new terminology)

## Testing Decisions

### Test File Renames
- `apps/api/tests/configs_test.rs` → `apps/api/tests/templates_test.rs`

### Test Function Renames
All test functions containing "config" in their name:
- `test_create_config` → `test_create_template`
- `test_list_configs` → `test_list_templates`
- `test_get_config` → `test_get_template`
- `test_update_config` → `test_update_template`
- `test_delete_config` → `test_delete_template`
- `test_create_config_requires_auth` → `test_create_template_requires_auth`
- `test_config_to_json_fields_in_response` → `test_template_to_json_fields_in_response`
- etc. (all ~35 test functions)

### Test Assertion Updates
All JSON access patterns in tests:
- `body["config"]` → `body["template"]`
- `body["configs"]` → `body["templates"]`
- `body["config"]["id"]` → `body["template"]["id"]`
- `body["config_name"]` → `body["template_name"]`
- `body["config_id"]` → `body["template_id"]`

### Test API Path Updates
All test HTTP calls:
- `"/api/configs"` → `"/api/templates"`
- `"/api/configs/{id}"` → `"/api/templates/{id}"`

### Test Variable Renames
All local variables in tests:
- `config_repo` → `template_repo`
- `config_id` → `template_id`
- `config_resp` → `template_resp`
- `config_name` → `template_name`
- `create_test_config` → `create_test_template`

### Verification
After all changes, first check for zero warnings, then run the full test suite:
```bash
cd apps/api && cargo test --no-run 2>&1 | grep -i warning
cd apps/api && cargo test --no-run --features docker 2>&1 | grep -i warning
cd apps/api && apps/api/scripts/run_tests.sh
cd apps/web && pnpm test
cd apps/web && pnpm check
```

## Out of Scope

- Renaming `ContainerConfig` struct in `docker.rs` (Docker container config, unrelated)
- Renaming `run_config`, `exec_config` fields (Docker run/exec config, unrelated)
- Renaming KasmVNC video codec config references (`codecs.js`, `rfb.js`, `display.js`)
- Renaming `vnc_trafik.rs` filename typo (separate concern)
- Changing the `workspace_instances` table name (already correct)
- Changing the `instances` API route (already correct)

## Further Notes

The "Sessions" tab name for the admin instance table is a deliberate choice to distinguish it from the personal "Instances" tab. This creates a clear mental model:
- **Instances** = your personal dashboard (card view)
- **Sessions** = admin overview of all sessions (table view)

The rename touches ~400+ individual edits across ~30 files. The recommended approach is to use batch find-and-replace with careful attention to:
1. Not renaming Docker-related `config` references
2. Updating both Rust and TypeScript simultaneously
3. Running tests after each major section (DB, backend, frontend)
