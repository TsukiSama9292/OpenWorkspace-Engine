# 03 — End-to-end integration

**Track:** integration

**What to build:** Full-stack verification of the flat-rbac_2 story against the real backend and real Docker, after both tickets land. An admin is a member of the seeded Admin group with all permissions and unlimited ceiling; a manager starts with all flags in the Manager group but cannot edit, delete, or re-assign Admin or fellow-Manager accounts, and cannot control their instances; a user in the User group launches only templates whitelisted on their groups and is capped at one running instance. Template authorization is group-only: a newly created template is admin-usable by default, an admin is blocked (403) from a template the Admin group is not whitelisted on, and a member's access flows from a group whitelist after the admin whitelists a group and adds them to it. The effective ceiling uses the max rule (personal never lowers below group; unlimited wins). The create-user flow assigns the User group by default. The contract drop is verified (legacy column and table gone). The full Rust integration suite and the Vitest suite run green with zero residue.

**Blocked by:** 02-fe-contract-and-auth-store

**Status:** ready-for-agent

- [ ] Admin/Manager/User tiers behave as specced across accounts, policies, groups, and instances (real Docker)
- [ ] Group-only template auth verified end to end: default admin grant, 403 without it, access via group whitelist
- [ ] Max-rule ceiling and create-user group default verified
- [ ] Contract drop verified: legacy column and table gone
- [ ] Full Rust suite + Vitest suite green, zero residue
