# Spec: apps/web Full Refactor

## Problem Statement

The `apps/web` SvelteKit application has accumulated technical debt that makes it difficult to maintain and extend:

- **Mixed module formats**: Some files are `.ts`, others are `.js` — no consistent type safety across the codebase.
- **Scattered CSS**: Route-specific `.css` files (6 total) and `<style>` blocks in components coexist with Tailwind utility classes, creating competing styling systems with no clear ownership.
- **Flat, unorganized `lib/`**: `vnc-components/` sits alongside `stores/` with no domain grouping. Utility functions (`formatMemory`) are duplicated across 3 route-local `utils.js` files.
- **Fat VNC page**: The `/vnc/[token]/` route page owns all VNC state, settings, and composition logic (168 lines), making it hard to modify VNC behavior without touching the route.
- **Redundant theme system**: A custom `theme.js` store toggles `data-theme` between `dark`/`light`, while the app already imports Skeleton UI's theme system (which uses the same `data-theme` attribute with richer tokens).
- **Unused dependencies**: `@skeletonlabs/skeleton-svelte` is imported in `layout.css` but no Skeleton components are used in any `.svelte` file. The `layout.css` file itself appears unused (conflicts with `app.css`).

The result is a codebase where adding a simple feature (e.g., a new form field on the config page) requires touching multiple files across different conventions, with no type safety to catch mistakes.

## Solution

A bottom-up, module-by-module refactor of all application code in `apps/web/src/` that:

1. Establishes a single CSS strategy: Tailwind CSS v4 utility classes inline in templates + Skeleton UI component classes, with a single `app.css` entry point. All separate `.css` files and `<style>` blocks are deleted.
2. Converts all application JavaScript to TypeScript with proper type annotations.
3. Reorganizes `lib/` into domain-based modules: `api/`, `stores/`, `components/{ui,vnc}/`, `utils/`.
4. Extracts VNC state management into a `VncSession.svelte` wrapper + `lib/stores/vnc.ts`, reducing the VNC route page to a thin orchestrator.
5. Activates the Skeleton "wintry" theme via a hardcoded `data-theme="wintry"` on `<html>` in `app.html`, removing the runtime theme toggle.
6. Extracts form sub-components for the config creation flow, keeping native Svelte bindings without adding a form library.
7. Consolidates duplicated utility functions into a single `lib/utils/format.ts`.

The vendored KasmVNC library (`lib/vnc/`) is left untouched — it is third-party protocol code.

## User Stories

### Module Reorganization

1. As a developer, I want `lib/` organized into domain modules (`api/`, `stores/`, `components/`, `utils/`) so that I can find related code without searching the entire tree.
2. As a developer, I want a single `lib/api/client.ts` module that exports typed HTTP methods (`get<T>`, `post<T>`, `put<T>`, `delete<T>`) so that API calls are type-safe and consistent.
3. As a developer, I want a single `lib/utils/format.ts` that exports `formatMemory()` and any future utility functions so that utilities are not duplicated across routes.
4. As a developer, I want shared UI primitives in `lib/components/ui/` (Modal, Button, etc.) so that common patterns are not re-implemented per route.
5. As a developer, I want VNC-related components in `lib/components/vnc/` (separated from `lib/vnc/` vendored code) so that the boundary between app code and vendored protocol code is clear.

### TypeScript Conversion

6. As a developer, I want all `.js` files in `lib/` and `routes/` converted to `.ts` so that the compiler catches type errors at build time.
7. As a developer, I want Svelte component props typed with `interface` declarations so that component contracts are explicit and refactoring is safe.
8. As a developer, I want store types defined (e.g., `User`, `VncSettings`) so that state shape is documented and validated by the compiler.

### CSS and Theming

9. As a developer, I want all styling done via Tailwind utility classes in templates so that there is one styling convention across the entire app.
10. As a developer, I want Skeleton UI component classes used for complex UI primitives (buttons, cards, modals) so that the UI is visually consistent and accessible.
11. As a developer, I want all route-specific `.css` files deleted and their styles inlined as Tailwind classes so that style rules are co-located with their components.
12. As a developer, I want all `<style>` blocks removed from `.svelte` files so that there is no competing scoping system alongside Tailwind.
13. As a user, I want the application to use the Skeleton "wintry" theme activated via `<html data-theme="wintry">` so that the UI has a consistent, polished appearance.
14. As a developer, I want the `app.html` to contain the `data-theme` attribute so that the theme is applied before first paint with no flash of unstyled content.

### VNC Viewer Architecture

15. As a developer, I want a `VncSession.svelte` wrapper component that owns VNC connection state (settings, connection status, clipboard) so that the route page is thin and VNC logic is encapsulated.
16. As a developer, I want VNC settings state managed in `lib/stores/vnc.ts` (a Svelte store) so that VNC components communicate via store subscriptions rather than prop drilling.
17. As a developer, I want the `/vnc/[token]/` route page to be a thin orchestrator that extracts the token and renders `<VncSession>` so that adding VNC features doesn't require modifying the route.
18. As a user, I want the VNC viewer to maintain the same functionality (clipboard sync, settings panel, status bar, fullscreen) after the refactor so that the refactoring is invisible.

### Forms

19. As a developer, I want the config creation form extracted into sub-components (`<ConfigBasics>`, `<ConfigResources>`, `<ConfigAdvanced>`) so that each section is independently readable and maintainable.
20. As a developer, I want dynamic key-value rows (env vars, volumes) extracted into reusable sub-components so that the pattern can be reused elsewhere.
21. As a developer, I want forms to use native Svelte `bind:value` without an external form library so that the dependency surface stays small.

### Auth and Navigation

22. As a user, I want the login page to function identically after the refactor so that authentication is not disrupted.
23. As a user, I want the navbar to remain hidden on `/login/` and `/vnc/` routes so that the UX is preserved.
24. As a developer, I want the auth store to have strict TypeScript types for `User` (`{ id, username, role: 'admin' | 'user' }`) so that role-based access checks are type-safe.

### Admin and Dashboard

25. As an admin user, I want the user management table to render correctly with Tailwind styling after the refactor so that admin functionality is preserved.
26. As a user, I want the dashboard to display config and instance cards with the same information and layout after the refactor.
27. As a user, I want the config detail page to show all configuration fields (image, CPU, RAM, GPU, volumes, env vars) after the refactor.
28. As a user, I want the instance detail page to show status, control buttons (start/stop/pause/resume), and VNC access links after the refactor.

### Dependency and Build

29. As a developer, I want `@skeletonlabs/skeleton-svelte` and `@skeletonlabs/skeleton` properly imported in `app.css` so that Skeleton's component classes and theme tokens are available app-wide.
30. As a developer, I want the unused `layout.css` deleted so that there are no conflicting CSS imports.
31. As a developer, I want `svelte-check` to pass with no errors after the refactor so that type safety is verified.
32. As a developer, I want `vitest` tests to pass after the refactor so that behavioral correctness is verified.

## Implementation Decisions

### Module Structure

The `lib/` directory will be reorganized into:

- `lib/api/client.ts` — Typed HTTP client wrapping `fetch` with `credentials: 'include'`, returning `{ data?: T, error?: string }` discriminated results.
- `lib/stores/auth.ts` — Svelte writable store with `login()`, `logout()`, `check()` methods. Type: `User | null`.
- `lib/stores/vnc.ts` — Svelte writable store for VNC session state (quality, compression, viewOnly, clipboard sync, connection status).
- `lib/components/ui/` — Reusable UI primitives: `Modal.svelte`, `Button.svelte`, etc.
- `lib/components/vnc/` — VNC-specific components: `VncSession.svelte`, `VncViewer.svelte`, `StatusBar.svelte`, `Clipboard.svelte`, `Settings.svelte`.
- `lib/utils/format.ts` — Consolidated utility functions (formatMemory, etc.).
- `lib/vnc/` — Vendored KasmVNC library. **Untouched.**

### CSS Strategy

- Single entry point: `app.css` containing `@import "tailwindcss"` and Skeleton imports (`@import '@skeletonlabs/skeleton'`, `@import '@skeletonlabs/skeleton-svelte'`, `@import '@skeletonlabs/skeleton/themes/wintry'`).
- All component styling via Tailwind utility classes in `.svelte` templates.
- Skeleton component classes used for complex UI (buttons, cards, modals, badges).
- All route-specific `.css` files deleted.
- All `<style>` blocks in `.svelte` files removed.

### Theme Activation

The Skeleton "wintry" theme is activated by hardcoding `data-theme="wintry"` on the `<html>` element in `app.html`. No JavaScript runtime theme switching. The existing `lib/stores/theme.js` is deleted.

### VNC Component Architecture

A new `VncSession.svelte` component wraps the VNC viewer experience. It:
- Owns VNC settings state via `lib/stores/vnc.ts`
- Composes `VncViewer`, `StatusBar`, `Clipboard`, `Settings` as children
- Handles WebSocket URL construction, fullscreen toggle, clipboard sync
- Exposes `sendCtrlAltDel()`, `reconnect()` etc. to the parent route

The `/vnc/[token]/` route page extracts the token and renders `<VncSession {token} />` — approximately 10-15 lines.

### Form Component Extraction

The config creation form (`/configs/new/`) is split into:
- `<ConfigBasics>` — Name, Description, Image fields
- `<ConfigResources>` — CPU, RAM, GPU, Docker Registry, Persistent Storage
- `<ConfigAdvanced>` — Collapsible section containing hostname, DNS, SHM, network mode, env vars, post-start command, volumes
- `<EnvVarRows>` — Dynamic key-value row component (reusable)
- `<VolumeRows>` — Dynamic host→container row component (reusable)

Forms use native Svelte `bind:value`. No external form library.

### TypeScript Types

Key types to define:
- `User`: `{ id: string; username: string; role: 'admin' | 'user' }`
- `Config`: `{ id: string; name: string; description: string; image: string; cpu_cores: number; ram_bytes: number; gpu_count: number; ... }`
- `Instance`: `{ id: string; name: string; config_id: string; status: 'running' | 'stopped' | 'paused' | 'error'; vnc_token?: string; ... }`
- `VncSettings`: `{ quality: number; compression: number; viewOnly: boolean; clipboardSync: boolean; scaleViewport: boolean }`
- `ApiResult<T>`: `{ data?: T; error?: string }`

### Execution Order

1. **Foundation**: `app.html` (add `data-theme="wintry"`), `app.css` (Tailwind + Skeleton imports), delete `layout.css`
2. **Utils + Stores**: Consolidate utils into `lib/utils/format.ts`, delete `theme.js`, type `auth.ts`
3. **API layer**: Move `lib/api.ts` → `lib/api/client.ts` with full types
4. **Shared components**: Build `lib/components/ui/Modal.svelte`, `Button.svelte`
5. **VNC components**: Refactor into `lib/components/vnc/`, create `VncSession.svelte` + `lib/stores/vnc.ts`
6. **Routes**: Refactor each route one by one — convert to Tailwind, delete CSS, wire new imports
7. **Verify**: Run `svelte-check` and `vitest` after each module

## Testing Decisions

### Testing Approach

- **Unit tests** for pure utility functions (`lib/utils/format.ts`) — these are the easiest to test and verify correctness of shared logic.
- **Component tests** for Svelte components using `@testing-library/svelte` — verify rendering and user interaction behavior, not implementation details.
- **Store tests** for `lib/stores/auth.ts` and `lib/stores/vnc.ts` — verify state transitions (login/logout, settings changes).
- **No route-level integration tests** in this refactor — the app uses `adapter-static` with `ssr = false`, making route testing require a full browser environment. Playwright E2E exists but requires live VNC containers.

### What Makes a Good Test

- Tests verify external behavior (what the user sees/does), not internal implementation (how the component achieves it).
- Tests use the actual component API (props, events) rather than reaching into internals.
- Tests are independent — no shared state between test cases.
- Mocking is limited to network requests (`fetch`) — components are tested with real DOM via `jsdom`/`happy-dom`.

### Prior Art

- API tests exist in `apps/api/tests/` using Rust integration test patterns — these are unaffected by this refactor.
- The web app has vitest configured (`vitest.config.js` implied by `pnpm test`) with `jsdom` environment and `@testing-library/svelte` + `happy-dom` setup.
- No existing web tests exist to migrate — this refactor establishes the testing baseline.

### Modules to Test

1. `lib/utils/format.ts` — Pure function tests for `formatMemory()` edge cases (bytes, KB, MB, GB, TB).
2. `lib/api/client.ts` — HTTP client tests with mocked `fetch` (verify credentials, error handling, JSON parsing).
3. `lib/stores/auth.ts` — State transition tests (login success/failure, logout, auth check).
4. `lib/stores/vnc.ts` — Settings update tests.
5. `lib/components/ui/Modal.svelte` — Open/close behavior, keyboard dismissal.
6. `lib/components/vnc/VncViewer.svelte` — Props rendering (mock RFB constructor).

## Out of Scope

- **Vendored VNC library** (`lib/vnc/`): 42 files of KasmVNC/noVNC protocol code. Not touched.
- **Runtime theme switching**: The theme is hardcoded. Adding a dark/light toggle is a future feature.
- **API backend** (`apps/api/`): Unaffected by this refactor.
- **Playwright E2E tests**: Requires live VNC containers, not part of this refactor.
- **New features**: This is a pure refactoring — no new functionality is added.
- **Tailwind CSS v4 migration**: Already using Tailwind v4 (`@tailwindcss/vite`). No version change needed.
- **Svelte 5 migration**: Already on Svelte 5 (`^5.56.7`). Using runes where applicable.
- **Build/deploy changes**: Docker Compose and nginx configuration are unaffected.
- **Performance optimization**: This refactor is about code organization, not runtime performance.

## Further Notes

### Risk Mitigation

- **Incremental execution**: Each module is verified (`svelte-check`, `vitest`) before moving to the next. If a step breaks, it's isolated to that module.
- **Vendored code isolation**: The `lib/vnc/` boundary is sacred. Any import from app code into vendored code (via shims) is preserved exactly.
- **CSS migration**: Converting `<style>` blocks to Tailwind is the highest-risk step. Each component should be visually verified after conversion.

### Files Deleted (Complete List)

- `src/routes/layout.css`
- `src/routes/dashboard.css`
- `src/routes/configs/new/new-config.css`
- `src/routes/configs/[id]/config-detail.css`
- `src/routes/instances/[id]/instance-detail.css`
- `src/lib/stores/theme.js`
- `src/routes/utils.js`
- `src/routes/configs/[id]/utils.js`
- `src/routes/configs/new/utils.js`

### Files Created (Complete List)

- `src/lib/api/client.ts`
- `src/lib/stores/vnc.ts`
- `src/lib/components/ui/Modal.svelte`
- `src/lib/components/ui/Button.svelte`
- `src/lib/components/vnc/VncSession.svelte`
- `src/lib/utils/format.ts`
