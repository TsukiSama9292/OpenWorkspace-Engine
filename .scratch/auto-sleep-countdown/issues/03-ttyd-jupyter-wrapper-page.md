# 03 — ttyd/Jupyter wrapper page

**What to build:** a platform-owned page at `/open/{token}/` that wraps ttyd and Jupyter Lab sessions so they can show the countdown. It looks up the Instance by its access token: when `running`, it embeds the original interface in an iframe (ttyd → its web UI; jupyter → `/lab?token=…`) with the countdown overlay on top — the overlay reuses the component from the countdown module and inherits its re-sync behaviour; when `starting`, it shows a waiting state and polls until running; when paused/stopped, it shows「已暫停／已停止」with a link back to the dashboard. The iframe is same-origin so the existing auth (Traefik-injected header for ttyd, URL token for jupyter) keeps working.

**Blocked by:** 01 — Auto-sleep deadline API contract, 02 — Countdown module + VNC overlay

**Status:** ready-for-agent

- [x] Visiting `/open/{token}/` for a running ttyd Instance shows its terminal in an iframe with the countdown overlay
- [x] Visiting `/open/{token}/` for a running jupyter Instance shows Jupyter Lab in an iframe with the countdown overlay; existing token auth works
- [x] `starting` shows a waiting state and transitions automatically to the iframe once running
- [x] paused/stopped shows「已暫停／已停止」and a link back to the dashboard
- [x] Overlay behaves as in ticket 02 (colors, action caption, 已到期, re-sync) without re-implementation
- [x] No Traefik/nginx configuration changes required; `pnpm check` and `pnpm build` pass
