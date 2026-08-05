# 15 — End-to-end integration

**Track:** integration

**What to build:** The full-stack verification that the pair of tracks works together against the real backend: an admin creates a group with a template whitelist and `max_instances`, assigns a member with a personal ceiling and whitelist; the member launches a whitelisted template, is refused a non-whitelisted one (`403`), hits their ceiling (`409`), and a same-group manager controls their instance; deleting the instance orphans its persistent volume, which an admin cleans up after confirmation. The full test suite (Rust integration + Vitest) runs green.

**Blocked by:** 13-fe-contract-cleanup, 14-be-contract-drop

**Status:** ready-for-agent

- [ ] The end-to-end scenario passes against the real backend and UI
- [ ] Whitelist (`403`) and ceiling (`409`) rejections surface correctly in the UI
- [ ] Orphaned-volume lifecycle works end to end
- [ ] Full Rust and frontend test suites pass
