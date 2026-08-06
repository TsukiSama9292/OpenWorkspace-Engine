# 04 — Entry points route to wrapper

**What to build:** opening a ttyd or Jupyter Instance always lands on the wrapper page, so the countdown is always visible. The dashboard's Open button and the instance-detail page's automatic redirect point ttyd/Jupyter Instances at `/open/{token}/` (using the shared `wrapperUrl` helper) instead of the proxied URL directly. VNC Instances keep opening on their own page (`/kasmvnc/{token}/`).

**Blocked by:** 03 — ttyd/Jupyter wrapper page

**Status:** completed

- [x] Dashboard Open button for ttyd/Jupyter Instances opens `/open/{token}/`; VNC unchanged
- [x] Instance-detail page auto-redirect for ttyd/Jupyter goes to `/open/{token}/`; VNC unchanged
- [x] URL construction uses the shared `wrapperUrl` helper (no duplicated logic)
- [x] `pnpm check` and `pnpm build` pass
