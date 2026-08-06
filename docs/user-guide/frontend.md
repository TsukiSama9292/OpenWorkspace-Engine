# The Browser UI

## Overview

Everything happens in a single-page web app. There is no separate app to
install — open the platform's web address, log in, and you're on the dashboard.
The UI is fully client-rendered: actions never require a page reload, so launch,
start, stop, and open all feel instant.

## Logging in

- The platform is reachable at its web address; unauthenticated visitors are
  sent to the login page.
- Log in with your username and password. Your session lasts a week, and you
  can log out at any time.
- There is no public sign-up: accounts are created by an administrator.

## The dashboard

A sidebar shows the pages you're allowed to use. Everyone sees **Instances**;
management and admin pages appear only if your group grants the permission
(see [RBAC](rbac.md)).

| Page | What it shows | Who can see it |
|------|---------------|----------------|
| **Instances** | Your sessions as cards, plus the quick-launch template grid | Everyone |
| **Templates** | The template catalog and editor | Template creators and admins |
| **Sessions** | A table of all sessions with status and user filters | Session managers and admins |
| **Volumes** | Orphaned persistent data, with a double-confirmed thorough cleanup | User managers and admins |
| **Groups** | Group management and template whitelists | Admins |
| **Users** | User accounts, group memberships, and personal ceilings | User managers and admins |
| **Settings** | Server-wide options (e.g. the host instance limit) | Admins |

## Launching a session

1. On the **Instances** page, pick a template from the quick-launch grid (or
   create one on the **Templates** page).
2. In the launch dialog, choose how your data is handled — **Use persistent
   storage** (default), **No persistent storage**, or **Reset persistent
   storage** (which asks you to confirm). See [Persistent Storage](persistent-storage.md).
3. Click **Launch**. If the platform refuses — a template you can't use, or a
   limit reached — it shows you exactly why (see [RBAC](rbac.md)).

## Managing your sessions

Each session card shows its status (running / starting / paused / stopped /
error), a persistence badge, a live countdown of any time budget, and the
actions available in that state:

- **Start / Stop** — stop shuts the container down but keeps your data and
  setup; start brings it back on the same address.
- **Pause / Resume** — pause suspends the session (uses almost no CPU); resume
  continues it.
- **Open** — jump straight into the running session.
- **Delete** — removes the session. Persistent data is kept (only a reset
  erases it).

## Inside a session

- **Desktops (KasmVNC)** open a full browser-based screen with clipboard
  support.
- **Terminals (ttyd)** and **notebooks (Jupyter)** open in a tabbed page.
- Every session page shows the auto-sleep / idle countdown and warns you before
  a deadline. While the page is open and focused, the session's idle clock is
  refreshed, so an active viewer never gets reclaimed.
- Your session's address is unique and stable across stops and restarts — you
  can bookmark it.

## Related docs

- [System Architecture](architecture.md) — how sessions are created and connected
- [RBAC](rbac.md) — who can see and control what
- [Persistent Storage](persistent-storage.md) — what happens to your data
- [Remote Authentication](remote-auth.md) — how sessions are secured
