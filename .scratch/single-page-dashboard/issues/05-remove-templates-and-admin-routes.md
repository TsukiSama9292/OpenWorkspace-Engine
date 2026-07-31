# 05 — Remove templates & admin routes

**What to build:** The separate template and admin pages are deleted entirely — the dashboard is the only place these flows live, and nothing references the removed routes.

**Blocked by:** 03 — TemplatePanel with in-tab editor (its in-page actions replace the links to these routes)

**Status:** resolved

- [x] The templates and admin route directories are deleted
- [x] No remaining link or redirect references the deleted routes
- [x] Build and existing tests pass; the login, instance detail, and VNC viewer routes are unaffected
