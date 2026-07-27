# Web UI

SvelteKit application for the OpenWorkspace Engine dashboard and VNC viewer.

## Commands

```bash
pnpm dev          # dev server on :5173
pnpm build        # static build → build/
pnpm check        # svelte-check (typecheck)
pnpm test         # vitest (37 tests)
```

## Tech Stack

- **SvelteKit** — `adapter-static`, `ssr = false`, `trailingSlash = 'always'`
- **Svelte 5** — runes (`$state`, `$props`, `$bindable`, `onMount`, `onDestroy`)
- **Tailwind CSS v4** — utility classes + `@tailwindcss/vite`
- **Skeleton UI v5** — `@skeletonlabs/skeleton` + `@skeletonlabs/skeleton-svelte`, theme `wintry`
- **TypeScript** — strict mode
- **Vitest** — jsdom environment, `@testing-library/svelte`
- **pako v1** — zlib compression (pinned, v3 broke noVNC imports)

## Design: Aurora Ambient & Spatial Modern

- **Accent**: Indigo `#6366f1`
- **Background**: Dark `#09090b` with `backdrop-filter: blur` glassmorphism
- **Font**: Plus Jakarta Sans
- **Cards**: Glassmorphism with `border-top` highlight edges
- **No Tailwind on login/dashboard** — pure scoped `<style>` blocks with Aurora Ambient design

## File Structure

```
src/
├── app.html                          # HTML shell, <html data-theme="wintry">
├── app.css                           # Tailwind + Skeleton + wintry theme import
│
├── lib/
│   ├── types.ts                      # Shared TypeScript interfaces
│   │                                 #   User, Config, Instance, VncSettings, ApiResult<T>
│   │
│   ├── api/                          # API layer (typed HTTP client + action modules)
│   │   ├── client.ts                 #   api.get/post/put/delete — returns ApiResult<T>
│   │   ├── config-actions.ts         #   launchInstance(), deleteConfig()
│   │   └── instance-actions.ts       #   performAction(id, action), deleteInstance(id)
│   │
│   ├── stores/                       # Svelte stores
│   │   ├── auth.ts                   #   Auth token + user store
│   │   └── vnc.ts                    #   VNC settings store (quality, compression, etc.)
│   │
│   ├── utils/
│   │   └── format.ts                 # Consolidated formatters + form helpers
│   │
│   ├── components/
│   │   ├── ui/                       # Reusable UI primitives (Tailwind-only)
│   │   │   ├── Modal.svelte          #   Generic modal dialog
│   │   │   └── Button.svelte         #   Styled button
│   │   │
│   │   ├── forms/                    # Config creation form sub-components
│   │   │   ├── ConfigBasics.svelte   #   Name, description, image
│   │   │   ├── ConfigResources.svelte#   CPU, RAM, GPU, disk
│   │   │   ├── ConfigAdvanced.svelte #   Env vars, volumes, network
│   │   │   ├── EnvVarRows.svelte     #   Dynamic KEY=VALUE rows
│   │   │   └── VolumeRows.svelte     #   Dynamic host:container mount rows
│   │   │
│   │   └── vnc/                      # VNC viewer components
│   │       ├── VncViewer.svelte      #   RFB connection + auto-retry (30×, 1s, 5s timeout)
│   │       ├── VncSession.svelte     #   Wraps VncViewer + StatusBar + Clipboard + Settings
│   │       ├── StatusBar.svelte      #   Draggable sidebar: status, clipboard, Ctrl+Alt+Del, etc.
│   │       ├── Clipboard.svelte      #   Bidirectional clipboard panel
│   │       └── Settings.svelte       #   Quality/compression/viewOnly/scaleViewport
│   │
│   └── vnc/                          # Vendored KasmVNC noVNC library (~15K lines)
│       ├── rfb.js                    #   RFB protocol client (DO NOT MODIFY)
│       ├── websock.js                #   WebSocket wrapper
│       ├── display.js                #   Canvas display rendering
│       ├── codecs.js                 #   Video codec pipeline
│       ├── mousebuttonmapper.js      #   Mouse button remapping
│       ├── input/                    #   Keyboard, gesture, pointer handlers
│       ├── decoders/                 #   Tight, Hextile, RRE, KasmVideo, etc.
│       ├── renderers/                #   WebGL, Canvas2D
│       ├── output/                   #   Smartcard
│       ├── util/                     #   EventTarget, cursor, logging
│       └── shims/                    #   Port relay worker, module shims
│
├── routes/                           # SvelteKit file-based routing
│   ├── +layout.js                    #   ssr=false, trailingSlash='always'
│   ├── +layout.svelte                #   Nav bar (hidden on /, /login/, /vnc/)
│   ├── +page.svelte                  #   Dashboard — immersive full-viewport
│   ├── dashboard-data.ts             #   Dashboard data loader (configs, instances)
│   │
│   ├── login/
│   │   ├── +page.svelte              #   Aurora Ambient login (Plus Jakarta Sans)
│   │   └── login.ts                  #   Login logic (credentials, token)
│   │
│   ├── configs/
│   │   ├── new/
│   │   │   ├── +page.svelte          #   Config creation form template
│   │   │   └── config-create.ts      #   Config form logic + submission
│   │   └── [id]/
│   │       ├── +page.svelte          #   Config detail template
│   │       ├── config-data.ts        #   Config detail data loader
│   │       └── config-actions.ts     #   Instance launch/delete (re-exported from $lib)
│   │
│   ├── instances/
│   │   └── [id]/
│   │       ├── +page.svelte          #   Instance detail template
│   │       ├── instance-data.ts      #   Instance data loader
│   │       └── instance-actions.ts   #   Instance start/stop/pause/delete
│   │
│   ├── vnc/
│   │   └── [token]/
│   │       └── +page.svelte          #   Thin orchestrator → VncSession
│   │
│   └── admin/
│       └── users/
│           ├── +page.svelte          #   Admin users template
│           └── users-data.ts         #   Admin data loader
│
└── tests/                            # Unit tests (vitest)
    ├── api-client.test.ts            #   API client tests
    ├── auth-store.test.ts            #   Auth store tests
    ├── format.test.ts                #   Formatter + form helper tests
    ├── vnc-store.test.ts             #   VNC store tests
    └── mocks/
        ├── app-navigation.ts         #   Mock $app/navigation
        └── app-stores.ts             #   Mock $app/stores
```

## Route Convention

| File | Purpose |
|------|---------|
| `+page.svelte` | **Template only** — HTML/Svelte markup |
| `*.ts` | **Logic** — data loading, actions, form handling |
| `+page.ts` | **Removed** — caused SSR 500 errors (`$app/navigation` in universal load) |

## VNC Reconnection

`VncViewer.svelte` implements auto-retry for when KasmVNC isn't ready yet:

- **30 retries** max, **1 second** between attempts
- **5 second timeout** — if RFB doesn't reach `connected` state, force retry
- `detachRfb()` removes all event listeners from old RFB before disconnecting (prevents ghost retries)
- `destroyed` flag prevents retries after component teardown
- Constructor throws → caught, scheduled for retry
- All events (`connect`, `disconnect`, `error`, `credentialsrequired`) cleared on `onDestroy`

## KasmVNC Integration

- nginx proxies WebSocket at `/vnc/{token}/websockify` → `https://kasm:6901/websockify`
- `proxy_ssl_verify off` required (KasmVNC hardcodes `-sslOnly`)
- `RFB` constructor needs real hidden `<input>` for `touchInput` (not `false`/`null`)
- `MouseButtonMapper` must be instantiated manually after RFB creation
- `pako@1` pinned — v3 broke internal import paths

## Static Build

`adapter-static` produces fully static files in `build/`. Docker Compose bind-mounts `build/` into nginx at `/usr/share/nginx/html`. After rebuilding: `docker compose restart nginx`.
