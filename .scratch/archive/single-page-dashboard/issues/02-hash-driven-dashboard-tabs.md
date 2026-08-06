# 02 — Hash-driven dashboard tabs

**What to build:** The dashboard reads and writes its current view through the URL hash, so the selected tab survives a page refresh, browser back/forward moves within the dashboard without any page load, and each tab is deep-linkable.

**Blocked by:** 01 — Shared template-form module & dashboard view helpers

**Status:** resolved

- [x] Sidebar tab clicks update the URL hash — no full page navigation occurs
- [x] Changing the hash (browser back/forward, manual edit, deep link) switches the dashboard tab accordingly
- [x] Empty or unknown hash lands on the Instances tab
- [x] Refreshing the page restores the previously selected tab
