# Remote Access Authentication

## Overview

Every session you open — a desktop, a terminal, or a notebook — is protected by
**its own credentials**. The platform issues each session a unique secret and a
unique web address, and the reverse proxy proves the session's identity
**server-side** on every connection. Your browser never has to handle a second
login prompt, and the secret never has to travel to your browser at all.

## The three session types

| Session type | What you get | How access is checked |
|--------------|--------------|------------------------|
| **KasmVNC** | A full desktop GUI | The proxy presents the session's secret automatically |
| **ttyd** | A terminal in a browser tab | The proxy presents the session's secret automatically |
| **Jupyter** | A Jupyter Lab notebook | The session carries its own secret in its address |

## Per-instance credentials

Each session is created with two pieces of information, generated fresh at
launch:

- **The session address token** — a long, random, unguessable string that forms
  the unique part of the session's web address. Two sessions never share one.
- **The session secret** — an even longer random password. It is stored only on
  the server. For desktops and terminals the proxy presents it automatically,
  so it **never appears in the address bar**; for Jupyter it is embedded once in
  the notebook address (Jupyter's own model requires it).

These are per-instance — not per-user, not shared across sessions. A user's
other sessions know nothing about this one's secret.

## What happens when you open a session

1. You log in to the dashboard, which keeps a secure session cookie. This cookie
   carries only your identity — it proves *who you are*, not what you may do
   (permissions are re-checked from the database on every request, see
   [RBAC](rbac.md)).
2. When your session is running, the dashboard hands you its unique address.
3. You open the address. The proxy checks that you are authenticated, then
   connects you to the session's container through a secure tunnel, presenting
   the session's secret on your behalf.
4. Inside the container, the session verifies the secret and the tunnel opens —
   the desktop, terminal, or notebook appears in your browser.

Only people who are logged in and authorized to see the session (you, an admin,
or a manager of your group) can open its address. See [RBAC](rbac.md) for the
exact control rules.

## Security model

| Layer | What it protects |
|-------|------------------|
| Per-session secret | Only the proxy can present it — a direct connection to the container is refused |
| Unguessable session address | Session addresses cannot be guessed or enumerated |
| Network isolation | Each session lives on its own isolated network; sessions cannot reach each other |
| Server-side injection | The secret is never exposed to the browser or typed by the user |

### What a leaked address or cookie can and can't do

- **An attacker who guesses a session's network address** still can't connect —
  the container refuses connections that lack the session's secret.
- **An attacker who learns a session's web address** still needs a logged-in,
  authorized account to open it.
- **An attacker who steals your session cookie** can open sessions you're
  allowed to control — the same as you being logged in. Sessions you don't
  control are not reachable.
- **A server administrator** who reads the database can always recover the
  secret — that is true of any server, and is not a client-side weakness.

## Related docs

- [System Architecture](architecture.md) — how sessions are created and connected
- [RBAC](rbac.md) — who may control a session
- [Persistent Storage](persistent-storage.md) — what happens to your data
