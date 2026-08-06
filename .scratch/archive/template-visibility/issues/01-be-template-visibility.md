# 01 — Template visibility field, API contract, launch-gate override

**Track:** backend

**What to build:** The entire template-visibility backend in one continuous pass — schema, API contract, and the launch-gate override — landing with the full API suite green and zero compiler warnings. A template owner can mark a template `public` (every authenticated user can launch it regardless of group whitelist), `private` (current behavior: group whitelist governs), or `hidden` (nobody can launch it — not whitelisted users, not the owner, not admins; no bypass).

**Migration:** Add a `visibility` column to `workspace_templates`, NOT NULL with default `'private'`, following the existing `mYYYYMMDD_NNNNNN_snake_case.rs` pattern. Existing rows land at `private`, preserving current behavior after upgrade. The sea-orm entity model and the `WorkspaceTemplate` struct plus its `From` impl gain the field.

**API contract:** Template JSON responses include `visibility` (`public` | `private` | `hidden`). Create and update requests accept `visibility` with a serde default of `private`, so clients that omit it keep working; invalid values are rejected as a client error. A small Rust enum wraps the three literals at the API boundary so the default and validation are centralized.

**Launch-gate override (single seam):** The pure `pre_flight` function gains a `template_visibility` argument. Check order: visibility first, then the whitelist, then the instance ceiling, then the host ceiling. `hidden` rejects with a dedicated reject reason before the whitelist is consulted; `public` skips the whitelist check; `private` keeps the current whitelist check. The launch activation path passes the template's visibility through — the activation request already carries the template. `allowed_template_ids` in the effective context stays a pure group-union base; visibility is applied only at the launch gate. Ceiling checks are unchanged — `public` grants permission, not quota.

**Testing:** Pure-unit tests at the `pre_flight` seam covering the visibility matrix — public launches with an empty whitelist; public still respects ceiling checks; hidden rejects even when the whitelist would allow it; private keeps current behavior. A migration test asserts existing rows land at `private`. Run `apps/api/scripts/run_tests.sh` and `apps/api/scripts/check.sh` — full suite green with zero residue.

**Blocked by:** None — can start immediately.

**Status:** completed

- [ ] Migration adds `visibility` (NOT NULL, default `private`); entity, struct, and `From` impl extended; existing rows land at `private`
- [ ] Template JSON emits `visibility`; create/update accept it (default `private`, invalid → client error); Rust enum centralizes default + validation
- [ ] `pre_flight` applies the override: hidden rejects (no bypass), public skips the whitelist, private unchanged; ceilings still enforced for public
- [ ] `pre_flight` visibility-matrix unit tests + migration-default test land green
- [ ] Full API suite green, zero warnings, zero residue
