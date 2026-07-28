# Terminology

This project uses three canonical domain concepts.

## Core Concepts

| Concept | Canonical Name | Description |
|---------|---------------|-------------|
| Template | **Template** | A pre-configured settings bundle (image, resources, env vars) that users launch instances from |
| Instance | **Instance** | A running VNC container launched from a template |
| User | **User** | A person with an account (admin/manager/user roles) |

## Sidebar Tabs

| Tab | Content | Visibility |
|-----|---------|------------|
| **Instances** | Personal instance cards (own only) | Everyone |
| **Templates** | Template cards with launch | Admin + Manager |
| **Sessions** | Full instance table (all users) with filters | Admin + Manager |
| **Users** | User table with CRUD | Admin + Manager |

## Mapping (old → new)

| Old (backend) | Old (frontend) | Canonical |
|---------------|----------------|-----------|
| `WorkspaceConfig` (Rust struct) | `Config` (TS interface) | **Template** |
| `configs` (API path `/api/configs`) | `config-actions.ts` | **templates** |
| `workspace_configs` (DB table) | — | **workspace_templates** |
| `config_id` (DB column / JSON key) | `config_id` (TS field) | **template_id** |
| `config_name` (JSON key) | `config_name` (TS field) | **template_name** |
| — | `Workspaces` (sidebar tab) | **Instances** |
| — | `Instances` (admin sidebar tab) | **Sessions** |
| — | `workspace` (login/dashboard strings) | **instance** |
| — | `workspace-grid` (CSS class) | **instance-grid** |
| — | `{workspace_name}` (storage hint) | **{template_name}** |
| `cpu_cores` (Rust field) | `cpu_cores` (TS field) | **cores** |
| `ram_bytes` (Rust field) | `ram_bytes` (TS field) | **memory** |
