# Frontend

SvelteKit app in `apps/web/` (Svelte 5 runes, Tailwind v4, Skeleton UI, fully static SSG via `@sveltejs/adapter-static`).

## Key Settings

- `+layout.js` — `ssr = false`, `trailingSlash = 'always'`
- `svelte.config.js` — `adapter-static`, fallback `index.html`, `base: ''`
- `app.css` — Tailwind import + Skeleton `@theme` (wintry preset), dark `color-scheme`, root bg `#09090b`
- The app is a pure SPA: no server, no load functions — data is fetched client-side through `lib/api/client.ts`

## File Tree (source of truth)

```
src/
├── app.css                 # Tailwind + Skeleton theme, global styles
├── app.html
├── lib/
│   ├── api/
│   │   ├── client.ts            # fetch wrapper → ApiResult<T> = {data} | {error}
│   │   ├── template-actions.ts  # launchInstance, deleteTemplate
│   │   └── instance-actions.ts  # performAction (start/stop/pause/unpause), deleteInstance
│   ├── stores/
│   │   └── auth.ts              # Svelte auth store + derived isAdmin/isManager
│   ├── components/
│   │   ├── templates/TemplatePanel.svelte
│   │   ├── instances/KeepTimeLine.svelte
│   │   ├── forms/               # TemplateBasics/Resources/Advanced, EnvVarRows, VolumeRows
│   │   ├── vnc/                 # VncSession, VncViewer, Clipboard, Settings, StatusBar
│   │   └── ui/                  # Button, Modal
│   ├── countdown/               # CountdownOverlay.svelte + countdown.ts
│   ├── keepalive/keepalive.ts   # heartbeat scheduler
│   ├── templates/               # dashboard-view.ts, template-form.ts
│   ├── utils/                   # format.ts, template-icons.ts
│   ├── types.ts                 # shared TS types
│   └── vnc/                     # noVNC protocol core + shims (assets/ decoders/ renderers/ input/ util/)
├── routes/
│   ├── +layout.js               # ssr=false, trailingSlash
│   ├── +layout.svelte           # auth guard, nav shell
│   ├── +page.svelte             # Dashboard (instances/templates/sessions/users)
│   ├── dashboard-data.ts        # loadDashboard() → {configs, instances}
│   ├── login/+page.svelte
│   ├── instances/[id]/+page.svelte
│   ├── open/[token]/+page.svelte
│   └── kasmvnc/[token]/+page.svelte
└── tests/                       # vitest suites (10 files, 154 tests)
```

> Note: there is **no** `+page.ts` route logic. Data loading happens inside the `+page.svelte` scripts, delegating to plain modules like `dashboard-data.ts` and the `lib/api/*` action helpers.

## Routes

### `+layout.svelte` — auth guard + shell

- On mount, calls `auth.check()` (`GET /api/auth/me`); while pending shows a loading screen.
- If not authenticated (and not on `/login/`), redirects to `/login`.
- `showNav` is derived: hidden on `/`, `/login`, and `/kasmvnc/*` / `/open/*` (full-screen session pages get no chrome).

### `/` — Dashboard (`+page.svelte`)

Single-page dashboard driven by `window.location.hash` (`parseDashboardHash`/`serializeDashboardHash` from `lib/templates/dashboard-view.ts`):

| Tab | Shows | Access |
|-----|-------|--------|
| `#instances` | Own instance cards (status dot, persist badge, sleep countdown, actions) + Quick Launch template grid | everyone |
| `#templates` | `TemplatePanel` (list/editor) | Admin, Manager |
| `#sessions` | All-instances table with user/status filters | Admin, Manager |
| `#users` | User CRUD table | Admin, Manager |

- Loads data via `loadDashboard()` (`Promise.all` of `GET /api/templates` + `GET /api/instances`), then polls `GET /api/instances` every 5 s.
- Instance control (`canControlInstance`) mirrors server RBAC: owner, admin, or manager-over-`user`-owner.
- Launch modal offers `use_persistent` / `no_persistent` / `reset_persistent` (only shown for templates with `persistent_storage_path`), with a confirm dialog before reset.
- User tab: create/edit/delete via `lib/api/client` against `/api/users`.

### `/login` — `login/+page.svelte`

Calls `auth.login(username, password)`; on success `goto('/')`.

### `/instances/[id]` — instance detail + control

- Fetches `GET /api/instances/{id}`. If `running`, hard-redirects to `wrapperUrl(remote_type, access_token)` (`/kasmvnc/{token}/` for VNC, `/open/{token}/` otherwise).
- If `starting`, polls every 2 s until running. Action buttons per status (Start/Stop/Pause/Resume/Open/Delete) call `instance-actions.ts`.

### `/open/[token]` — non-VNC session wrapper

- Finds the instance by `access_token` from `GET /api/instances`.
- KasmVNC instances redirect to `/kasmvnc/{token}/`; ttyd/Jupyter render an `<iframe>` via `iframeSrc(remote_type, token, access_password)`:
  - jupyter → `/jupyter/{token}/lab?token=<access_password>`
  - ttyd → `/ttyd/{token}/`
- Overlays `CountdownOverlay` (auto-sleep / keep-time deadlines) and runs the keepalive heartbeat while running.

### `/kasmvnc/[token]` — VNC session viewer

- Pulls `access_password` from the matched instance and passes it to `VncSession`.
- Handles `starting` (poll) → `ready` transition, plus the same countdown/keepalive logic.
- Renders `VncSession` → `VncViewer` (noVNC `RFB` over `wss://<host>/kasmvnc/{token}/websockify`).

## Data Layer

### `lib/types.ts`

`Role` (`admin|manager|user`), `RemoteType` (`kasmvnc|ttyd|jupyter`), `TimeoutAction` (`remove|stop|pause`), `Template`, `Instance`, `VncSettings`, `ApiResult<T>`.

### `lib/api/client.ts`

Tiny fetch wrapper over `/api`:

```ts
export const api = {
  get:  <T>(path) => request<T>('GET',  path),
  post: <T>(path, body?) => request<T>('POST',  path, body),
  put:  <T>(path, body?) => request<T>('PUT',  path, body),
  delete: <T>(path) => request<T>('DELETE', path),
};
```

- `credentials: 'include'` (cookie auth), `Content-Type: application/json`
- Never reads the HttpOnly cookie itself — auth state comes from `GET /api/auth/me`
- Normalizes failures to `{ error }`; JSON parse failures → `{ error: 'Server error (N)' }`

### `lib/stores/auth.ts`

`writable<User | null>` store:

- `login(username, password)` → `POST /api/auth/login`, sets the user on success
- `logout()` → `POST /api/auth/logout`, clears
- `check()` → `GET /api/auth/me`, sets or nulls
- Derived: `isAuthenticated`, `isAdmin` (`role === 'admin'`), `isManager` (`admin` or `manager`)

### `lib/keepalive/keepalive.ts`

`startKeepalive(instanceId, opts)` posts `POST /api/instances/{id}/heartbeat` every 10 s **only while the tab/iframe is focused and visible** (`document.visibilityState === 'visible' && tabHasFocus()`, with cross-origin-iframe detection). Sends immediately on gaining focus. Returns a stop function.

### `lib/countdown/countdown.ts`

- `remainingMs(deadline, now)` / `formatRemaining` — HH:MM:SS countdown helpers
- `selectDeadline(auto_sleeps_at, timeout_action, keep_time_deadline, keep_time_action)` — picks the earliest deadline (keep-time wins if sooner); drives `CountdownOverlay`
- Severity thresholds: `WARNING_THRESHOLD_MS = 10 min`, `CRITICAL_THRESHOLD_MS = 60 s`
- `wrapperUrl(remoteType, token)` / `iframeSrc(remoteType, token, password)` — session URL builders

## noVNC Integration

- noVNC core files live in `lib/vnc/` (rfb.js, websock.js, display.js, input/, renderers/, decoders/, ...), with shim files in `lib/vnc/shims/`.
- `pako@1` is pinned — v3 broke the internal import paths noVNC uses.
- `VncViewer.svelte` instantiates `RFB` with a real hidden `<input>` element for `touchInput` (not `false`/`null`), and manually initializes `mouseButtonMapper` (null by default in rfb.js).
- KasmVNC quirks the UI works around: `VNCOPTIONS=-disableBasicAuth` disables websockify Basic auth (Traefik injects it instead), and the backend forces `-sslOnly`, so `proxy_pass https://` + `proxy_ssl_verify off` is required.

## Testing

`pnpm test` runs Vitest (jsdom/happy-dom) — **10 files, 154 tests**:

- `api-client`, `auth-store`, `template-actions`, `dashboard-view`, `template-form`, `template-panel`, `keepalive`, `keep-time-line`, `countdown`, `format`
- `tests/mocks/` provides `app-navigation` (`goto`) and `app-stores` (`page`) stubs

`pnpm check` runs `svelte-kit sync && svelte-check` (typecheck). Playwright E2E is configured but requires live containers to run.
