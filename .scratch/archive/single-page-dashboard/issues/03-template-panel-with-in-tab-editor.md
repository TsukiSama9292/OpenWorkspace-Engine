# 03 — TemplatePanel with in-tab editor

**What to build:** The templates tab becomes a self-contained panel with an in-place list ↔ editor view swap: create and edit templates entirely inside the dashboard, save returns to a refreshed list, cancel returns instantly, and a deleted template shows a graceful error.

**Blocked by:** 01 — Shared template-form module & dashboard view helpers, 02 — Hash-driven dashboard tabs

**Status:** resolved

- [x] The templates list renders inside the panel with the existing card actions (Edit, Delete, launch) unchanged
- [x] "+ New Template" and "Edit" swap the tab content to the editor without page navigation, reusing the existing Basics / Resources / Advanced form sections
- [x] The hash reflects the editor view (`#templates/new`, `#templates/edit/<id>`) and direct deep links open the editor
- [x] Saving a create or edit closes the editor, returns to the list, refetches the templates, and preserves scroll position
- [x] Cancel/Back closes the editor and returns to the list without refetching
- [x] Editing a template that no longer exists shows an error with a way back to the list
