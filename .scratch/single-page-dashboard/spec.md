Status: resolved

# Single-Page Dashboard — Absorb Templates & Admin Routes

## Problem Statement

The dashboard at `/` is already a tabbed single-page shell (`instances` / `templates` / `sessions` / `users`), but three flows still force a full page navigation away from it:

1. Creating or editing a template navigates to `/templates/new` or `/templates/[id]/edit`, then `goto('/')` on save — two full page loads for one edit, plus the form state lives in route-scoped modules that must be duplicated between the two routes.
2. User management exists twice: a fully working inline Users tab inside the dashboard, and a separate `/admin/users` route that is dead code (zero inbound links anywhere in the app).
3. The dashboard keeps its view in in-memory state only, so a refresh resets to the Instances tab, browser back/forward cannot move within the dashboard, and there is no deep link for "edit this template".

The result is perceptible page-switch latency for the most common management tasks, dead code, and a view that forgets where you were.

## Solution

Make the dashboard the single-page home for all management: absorb template create/edit into the `templates` tab as an in-place view swap (list ↔ editor), and delete the `templates` and `admin` route directories entirely. Drive the dashboard view from a URL hash (`#templates/new`, `#templates/edit/<id>`, `#users`, …) so refresh, back/forward, and deep links all work without any full page load. Add an unsaved-changes guard on the template editor.

Routes that remain standalone: `/login`, `/instances/[id]` (deep-link target when launching in a new tab), `/kasmvnc/[token]` (full-screen VNC viewer).

## User Stories

1. As a manager, I want to create a template without leaving the dashboard so that I don't wait for a page switch.
2. As a manager, I want to edit a template without leaving the dashboard so that I don't wait for a page switch.
3. As a manager, I want the templates tab to swap between the list and the editor in place so that the sidebar and context remain visible.
4. As a manager, I want the template editor to reuse the existing Basics / Resources / Advanced form sections so that the form behavior I already know is unchanged.
5. As a manager, I want a "Back" control in the editor so that I can return to the templates list without saving.
6. As a manager, I want my in-progress editor to stay alive if I hit the browser Back button so that I can navigate within the SPA without losing work.
7. As a manager, I want the dashboard tab to survive a page refresh so that I land where I left off.
8. As a manager, I want a deep link like `#templates/edit/<id>` to open the editor directly so that I can share the exact view with a colleague.
9. As a manager, I want to save a new template and land back on the refreshed templates list so that I can immediately see my new template.
10. As a manager, I want to save an edit and land back on the refreshed templates list so that the updated values are immediately visible.
11. As a manager, I want saving to preserve my scroll position so that the list doesn't jump around after an edit.
12. As a manager, I want cancelling the editor to return to the list without re-fetching so that cancelling is instant.
13. As a manager, I want a confirmation prompt if I try to leave the editor with unsaved changes so that I don't lose my work by accident.
14. As a manager, I want the unsaved-changes guard to cover the Cancel button, sidebar tab switches, browser back/forward, and refresh/close so that no path silently discards my edits.
15. As a manager, I want expanding/collapsing the Advanced section to not count as an unsaved change so that I can peek at advanced options without triggering the guard.
16. As a manager, I want to manage users through the existing Users tab so that I never need a separate admin page.
17. As an admin, I want to create, edit, and delete users from the Users tab so that I retain full user management without the dead `/admin` route.
18. As a user, I want the templates list and launch flow to behave exactly as before so that the merge does not change my access.
19. As any authenticated user, I want the dashboard to switch between Instances, Templates, Sessions, and Users tabs without any page load so that navigation is instant.
20. As any authenticated user, I want browser back/forward to move between dashboard tabs so that the SPA feels like a native app.
21. As any authenticated user, I want the default dashboard view to be the Instances tab so that the common case is unchanged.
22. As a user, I want to keep launching workspaces from templates so that the Quick Launch experience is unaffected.
23. As a manager, I want the editor to handle a template that no longer exists (deleted meanwhile) by showing an error with a way back so that I'm not stuck on a broken form.
24. As a manager, I want the existing per-card Delete action on templates to keep working in place so that deletion remains instant.

## Implementation Decisions

### Route Removal

- Delete `routes/templates/` and `routes/admin/` entirely (including `users-data.ts`). Confirmed dead: `/admin` has zero inbound references anywhere in the app; the dashboard Users tab is the live implementation.
- Keep `routes/login/`, `routes/instances/[id]/`, `routes/kasmvnc/[token]/` unchanged.
- No nginx/static-adapter changes: deleted routes simply stop generating pages; the dashboard hash views are purely client-side.

### Hash-Based Dashboard View State

The dashboard view is driven by a URL hash as the single source of truth:

- `#instances` (default, also when no hash or unknown hash)
- `#templates`
- `#templates/new`
- `#templates/edit/<id>`
- `#sessions`
- `#users`

Tab and editor changes write the hash; `hashchange` is listened to; `activeTab` and editor state are derived from the hash. Serialize/parse are pure helpers (`parseDashboardHash`, `serializeDashboardHash`) so they are unit-testable. On mount the current hash selects the initial tab.

### TemplatePanel Component

Extract the whole `templates` tab — list, view-swap editor, refetch, hash sync, and dirty guard — into a self-contained `TemplatePanel` component under `lib/components/`. The dashboard stays a thin tab shell. The panel owns:

- The templates list (current card grid, icon, metrics, Edit / Delete actions).
- The editor view: a header with title (`New Template` / `Edit Template`) and a Back control, rendering the existing `TemplateBasics` / `TemplateResources` / `TemplateAdvanced` form sections (already in `lib/components/forms/`).
- `openCreate()` / `openEdit(id)` actions invoked by the in-page "+ New Template" and "Edit" buttons (replacing the two anchor links that point at the deleted routes).

Instances, Sessions, and Users tabs are not refactored in this change.

### Shared Logic Relocation

Merge the two route-scoped modules into one `lib/templates/template-form.ts` module:

- `createInitialFormState` (create mode initial state)
- `formStateFromTemplate` (edit mode initial state from a fetched template)
- `loadTemplate(id)`
- `submitTemplate(state)` / `updateTemplate(id, state)` — no longer call `goto('/')`; the panel performs its own close + refetch
- `DEFAULT_IMAGES`

`lib/utils/format.ts` and the form components are already in `lib/` and are unchanged. The two templates' bodies are built identically; only the HTTP verb and initial state differ, so a single module avoids the existing cross-import between the two route files.

### Post-Save / Cancel Behavior

- Save (create or edit): on success, close the editor, return to the templates list, refetch `/templates` once so the list reflects server truth, and preserve scroll position. No page navigation.
- Cancel / Back control: close the editor, return to the list, no refetch.
- Load error in edit mode (template deleted meanwhile): show an error state with a way back to the list.

### Unsaved-Changes Guard

- **Dirty definition:** a normalized snapshot of the persisted fields (all form fields except `showAdvanced`, `loading`, `error`) compared with the initial snapshot — empty initial state for create, the loaded template's values for edit. Comparison is deep (nested `envVars` / `volumeMappings` arrays). Numeric fields are normalized (coerced/`parseInt`) before comparison so empty or unchanged number inputs don't cause false positives.
- **Intercepted leave paths (all four):**
  1. Editor Cancel / Back control
  2. Sidebar tab switch (the dashboard asks the panel whether leaving is allowed before changing the hash)
  3. Browser back/forward — a `hashchange` handler prompts if the change leaves the editor; if declined, the previous hash is restored
  4. Refresh/close — native `beforeunload` handler (browser's own dialog)
- **Mechanism:** native `confirm()` for the three in-app paths (consistent with the existing native `confirm()` delete dialogs), native `beforeunload` for the browser path. No custom dialog is introduced.

### Sidebar / Layout

- Sidebar tab clicks now write the hash (instead of only mutating in-memory tab state), passing through the panel's leave-guard.
- The root layout's nav visibility behavior is unchanged; it simply no longer has templates/admin routes to special-case.

### API

No backend changes. All required endpoints already exist: `GET/POST/PUT/DELETE /templates`, `GET /templates/:id`, `GET/POST/PUT/DELETE /users`.

## Testing Decisions

### Test Philosophy

Test external behavior at the module seam — pure logic, not Svelte internals. Prefer the highest seam that exercises real behavior without a browser: pure functions that encode the state transitions (dirty detection, hash parse/serialize, form round-trip) and the API-call shape through the existing mocked `api` client.

### Modules Tested

- **`template-form.ts`**: `createInitialFormState` → `submitTemplate` builds the correct body through the mocked `api` client; `formStateFromTemplate` → `updateTemplate` round-trip preserves fields; numeric normalization in dirty snapshots (empty/unchanged number fields are not dirty).
- **Hash helpers**: `parseDashboardHash` / `serializeDashboardHash` round-trip for every view; unknown/empty hash falls back to `#instances`.
- **Dirty comparison**: create mode with only `showAdvanced` toggled is not dirty; any persisted field change is dirty; nested env var/volume edits are detected.

### Prior Art

Follow `src/tests/format.test.ts` (pure-function unit tests) and `src/tests/api-client.test.ts` (mocked `fetch`/`api` client assertions) — the same structure and conventions.

### Not Unit-Tested

The view-swap rendering, hash↔component wiring, and the confirmation dialogs are verified by manual/browser testing in the dev server, since no component-level test seams currently exist in the repo.

## Out of Scope

- `/login`, `/instances/[id]`, `/kasmvnc/[token]` — unchanged routes and behavior.
- Refactoring the Instances, Sessions, or Users tabs out of the root dashboard component.
- A custom themed confirmation dialog to replace native `confirm()` / `beforeunload`.
- Persisting in-progress editor state across tab switches (switching tabs deliberately abandons the form after the guard).
- Changing access control / role gating (templates tab stays visible to all authenticated users, matching current behavior).
- Backend or API changes of any kind.
- The Sessions tab contents (Jupyter sessions) — untouched.

## Further Notes

- **Dead code removal is free:** `/admin/users` duplicates the Users tab with a strictly weaker UI, and nothing links to it. Removing it removes duplicated user-management logic and an admin-only route that the role-redesign spec's tab-visibility rules never referenced.
- **Latency win:** every dashboard-internal flow becomes a pure client-side view swap. The only remaining full page loads in the app are login, instance detail, and the VNC viewer.
- **Editor reachability:** deep links into `#templates/edit/<id>` work with the static adapter because hash state never touches the server; the trailing-slash / static SSG setup is unaffected.
- This spec is the consolidation of an interview; it supersedes the interim dark-theme/background work only in the sense that the editor UI will be rendered inside the dashboard and must keep the dashboard's neutral palette.
