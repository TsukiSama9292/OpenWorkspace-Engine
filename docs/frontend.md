# Frontend Architecture

## Overview

SvelteKit SPA with `adapter-static`, `ssr = false`, and `trailingSlash = 'always'`. Static build served by nginx container.

## Route Convention

Each route folder follows a strict separation:

| File | Purpose | Contains |
|------|---------|----------|
| `+page.svelte` | Template only | HTML/Svelte markup, event handlers call into `.ts` files |
| `+page.ts` | Logic/actions | Functions, stores, data fetching, form handling |
| `+layout.svelte` | Layout template | Navigation, global UI |
| `+layout.ts` | Layout logic | Auth checks, global data |

**Never** put business logic in `+page.svelte`. It should be a thin template that delegates to `.ts` files.

## Directory Structure

```
src/
├── lib/
│   ├── api/              # API client + action modules
│   │   ├── client.ts     # Typed HTTP client (credentials: 'include')
│   │   ├── instance-actions.ts
│   │   └── template-actions.ts
│   ├── components/
│   │   ├── ui/           # Reusable UI primitives
│   │   │   ├── Modal.svelte
│   │   │   ├── Button.svelte
│   │   │   └── Clipboard.svelte
│   │   └── vnc/          # VNC-specific components
│   │       ├── VncSession.svelte
│   │       ├── VncViewer.svelte
│   │       ├── StatusBar.svelte
│   │       ├── Clipboard.svelte
│   │       └── Settings.svelte
│   ├── stores/
│   │   └── auth.ts       # Auth store with isAdmin/isManager derived
│   ├── types.ts          # Shared TypeScript types
│   └── utils/
│       └── format.ts     # Formatting utilities
├── routes/
│   ├── +page.svelte      # Dashboard
│   ├── +layout.svelte    # Root layout (auth guard)
│   ├── login/            # Login page
│   ├── instances/[id]/   # Instance detail
│   ├── vnc/[token]/      # VNC viewer
│   └── users/            # User management (admin)
└── app.html              # HTML shell (data-theme="wintry")
```

## CSS Strategy

**No Tailwind in component templates.** All styling uses scoped `<style>` blocks with Aurora Ambient design tokens:

- Background: `#09090b` (zinc-950)
- Card: `rgba(255,255,255,0.03)` with `backdrop-filter: blur(12px)`
- Accent: `#6366f1` (indigo-500)
- Text: `#fafafa` (zinc-50)
- Border: `rgba(255,255,255,0.06)`
- Highlight: `border-top: 1px solid rgba(99,102,241,0.3)`

Tailwind is only used in `Modal.svelte` and `Button.svelte` (reusable UI primitives).

## Type Definitions (`lib/types.ts`)

```typescript
export type Role = 'admin' | 'manager' | 'user';

export interface User {
    id: string;
    username: string;
    role: Role;
    created_at: string;
}

export interface Instance {
    id: string;
    name: string;
    instance_number: number;
    container_id: string;
    status: 'running' | 'stopped' | 'error';
    owner_id: string;
    owner_username: string;
    owner_role: Role;
    vnc_token: string;
    vnc_password?: string;
    created_at: string;
}
```

## API Client (`lib/api/client.ts`)

Typed HTTP client with `credentials: 'include'` for cookie auth:

```typescript
export async function api<T>(path: string, init?: RequestInit): Promise<T> {
    const res = await fetch(`/api${path}`, {
        ...init,
        credentials: 'include',
        headers: { 'Content-Type': 'application/json', ...init?.headers },
    });
    if (!res.ok) throw new Error(`API error: ${res.status}`);
    return res.json();
}
```

## Auth Store (`lib/stores/auth.ts`)

```typescript
export const auth = writable<{ user: User | null; loading: boolean }>({...});
export const isAdmin = derived(auth, ($auth) => $auth.user?.role === 'admin');
export const isManager = derived(auth, ($auth) => $auth.user?.role === 'manager');
```

## VNC Components

### VncSession.svelte

Wraps `VncViewer` + `StatusBar` + clipboard/settings panels. Accepts `password` prop, passes to `VncViewer`.

### VncViewer.svelte

- Connects in `onMount` with initial password
- `$effect` watches `password` prop — reconnects when it changes
- 30-retry limit with 1s delay between retries
- 5-second connect timeout (forces retry if RFB never reaches `connected` state)
- `detachRfb()` removes all event listeners before disconnecting
- Constructor throws are caught + retried

### Password Delivery

VNC password is fetched from `GET /api/instances` on page mount, matched by `vnc_token`. Never stored in URL (`?pw=` removed).

## Build & Deploy

```bash
cd apps/web
pnpm build          # → build/ (static files)
pnpm test           # vitest (32 tests)
pnpm check          # svelte-check (typecheck)
```

Docker Compose bind-mounts `build/` into nginx at `/usr/share/nginx/html`.
