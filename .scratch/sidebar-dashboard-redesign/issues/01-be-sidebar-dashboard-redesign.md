# 01 — Backend baseline: prove no API change is needed

**Track:** backend

**What to build:** From the user's perspective, nothing visibly changes on the backend in this round. The dashboard redesign reuses the existing effective context, session collection, and template catalog exactly as they behave today, so launches, lifecycle actions, permission checks, ceilings, and denial explanations keep working untouched while the frontend is rebuilt on top.

**Blocked by:** None — can start immediately.

**Status:** ready-for-agent

- [ ] No migration is added and no route contract changes; the API surface behaves as before the round.
- [ ] The Rust zero-warning gate passes on both feature sets with no output.
- [ ] The full API test suite passes, giving the frontend a known-good base.
