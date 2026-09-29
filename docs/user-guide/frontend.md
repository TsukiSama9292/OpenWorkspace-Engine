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

A navigation rail shows the pages you're allowed to use — readable labels by
default, collapsible to a narrow strip (your choice is remembered), and a
drawer behind a menu button on phones. A running-session count rides next to
the session entries, and the bottom shows your real name and tier instead of
a placeholder. Everyone sees **Instances**; management and admin pages appear
only if your group grants the permission (see [RBAC](rbac.md)).

The **Instances** page reads top to bottom in three stages: a greeting hero
with one search box for everything, your quota usage, and totals for running,
stopped, and available templates; your sessions as rich cards above; and the
template catalog below. The search box filters both sessions and templates at
once, with running/stopped chips for sessions.

| Page | What it shows | Who can see it |
|------|---------------|----------------|
| **Instances** | Greeting hero with search, your session cards, and the template catalog | Everyone |
| **Templates** | The template catalog and editor | Template creators and admins |
| **Sessions** | A table of all sessions with status and user filters | Session managers and admins |
| **Volumes** | Orphaned persistent data, with a double-confirmed thorough cleanup | User managers and admins |
| **Groups** | Group management and template whitelists | Admins |
| **Users** | User accounts, group memberships, and personal ceilings | User managers and admins |
| **Monitor** | Host CPU/RAM/disk and per-instance resource usage with sparklines | Groups with the monitoring flag (see [RBAC](rbac.md)) |
| **Logs** | The audit trail: who did what, when, and from where | Groups with the audit-logs flag (see [RBAC](rbac.md)) |
| **Settings** | Server-wide options (e.g. the host instance limit) | Admins |

## Launching a session

1. On the **Instances** page, pick a template from the catalog (or
   create one on the **Templates** page). Each catalog card shows what the
   template is, what it costs in processor/memory/graphics/storage, and a
   **Launch** button — locked templates explain why and whom to ask instead.
2. In the launch dialog, choose how your data is handled — **Use persistent
   storage** (default), **No persistent storage**, or **Reset persistent
   storage** (which asks you to confirm). See [Persistent Storage](persistent-storage.md).
3. Click **Launch**. If the platform refuses — a template you can't use, or a
   limit reached — it shows you exactly why (see [RBAC](rbac.md)).

## Managing your sessions

Each session card names its source template, shows its state plus any
remaining time budget in plain words, and leads with a single **Open** action
— the rest (pause/stop/start, logs, removal) lives behind a **More** menu.
Removal always asks for confirmation:

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

## Monitoring the host

The **Monitor** page (for admins and groups granted the monitoring flag) answers
"what is happening on the box right now":

- Three **host cards** — CPU, RAM, Disk — are full interactive charts of the
  last 24 hours. They start showing the newest data and keep updating while the
  page is open; hover a point for the exact value + time, click to pin a
  readout, or drag across a chart to zoom into a range (with the average / max
  / min of the selection shown live). A "back to now" button returns to the
  live view after you zoom around.
- An **Active Instances** table lists every running / starting / paused session
  with its owner, template, runtime, uptime, and live CPU % / RAM usage (each
  with a small sparkline you can hover for a value and click to pin). Paused
  (auto-slept) sessions are greyed out with a `[paused]` badge; stopped and
  failed sessions are not listed. Columns can be sorted (e.g. by RAM) to find
  the worst offender.
- Each row has a **Detail** button that opens the instance's own CPU and memory
  charts as full interactive views, so you can trace a memory leak or a CPU
  spin on a single workload. The charts resolve automatically: fine-grained
  (15 s) points inside the last hour, five-minute averages further back.
- Granting or revoking the flag is done per group in the group editor (see
  [RBAC](rbac.md)).

## Viewing the audit trail

The **Logs** page (for admins and groups granted the audit-logs flag) records
"who did what" on the box:

- Every meaningful action is logged with the actor, the action, its target,
  whether it succeeded, the caller's IP, and the time. This includes sign-ins
  and failed sign-ins, session lifecycle events (create / start / stop / pause /
  resume / delete, including auto-sleep), template and group and user edits,
  registry and settings changes, and denied-access attempts by signed-in users.
- Use the filter bar to narrow the list — by event type, actor, target, outcome,
  or a date range. The six controls sit in an evenly aligned grid with the
  date-range pair together; the **Apply** / **Clear** buttons and the entry
  count live on their own action row, so controls never shift around when the
  window narrows. The list is newest-first and pages through long histories
  automatically with a **Load more** control.
- Times are shown compactly (for example `2026-08-08 15:14`); hover a time to
  see the full date and time. On narrow windows the caller IP column hides so
  the table keeps its layout instead of overflowing.
- Edit events expand with a small chevron button (keyboard-operable) to show
  exactly which fields changed and their before and after values. Sensitive
  values (passwords, secrets, tokens, keys, credentials) are always shown as
  `[REDACTED]`.
- The trail is kept for 90 days and then pruned automatically.

## Reading a session's output

Every session card has a **Logs** button (for the owner, admins, and group
managers entitled to control the session) that opens the session's console
output — the last 200 lines by default:

- Turn on **follow** to stream new output live while the session runs. While
  follow is active the view stays pinned to the newest line; scrolling up to
  read older output pauses follow (the status text says so), and scrolling back
  to the bottom resumes it.
- Long lines can be read in two ways: **Wrap** keeps lines flowing within the
  panel (default), or turn it off to keep terminal column alignment with
  horizontal scrolling.
- Each line is numbered, and stdout (blue) is visually separated from stderr
  (red) by a colored edge so errors stand out.
- The **A−** / **A+** controls adjust the text size to your display, and your
  choice is remembered for every session's logs.
- The panel opens large by default, with a **Fullscreen** toggle for long
  output. Long instance names are truncated in the header (full name on hover).
- When a stopped or paused session's logs are opened, the panel shows the tail
  plus a clear "session ended" state with the reason, so a dead session is
  still debuggable.
- Color codes in tool output are rendered as colors. Closing the panel stops
  the stream.

## Related docs

- [System Architecture](architecture.md) — how sessions are created and connected
- [RBAC](rbac.md) — who can see and control what
- [Persistent Storage](persistent-storage.md) — what happens to your data
- [Remote Authentication](remote-auth.md) — how sessions are secured
