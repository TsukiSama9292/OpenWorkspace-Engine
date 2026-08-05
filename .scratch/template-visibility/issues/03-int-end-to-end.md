# 03 — Template visibility end-to-end

**Track:** integration

**What to build:** Full-stack verification of the template-visibility story against the real backend and real Docker, after both tickets land. A template marked `public` is launchable by a user whose groups are not whitelisted on it. A template marked `hidden` is rejected for a user whose group is whitelisted on it, and for an admin — no bypass. A template created without the field is `private`, so the group whitelist governs exactly as before. The full Rust integration suite and the Vitest suite run green with zero residue.

**Blocked by:** 02-fe-template-visibility

**Status:** ready-for-agent

- [ ] Public template launches for a user with no group grants (real Docker)
- [ ] Hidden template launch rejected for a whitelisted user and for an admin (real Docker)
- [ ] New template without the field is private — group whitelist governs
- [ ] Full Rust suite + Vitest suite green, zero residue
