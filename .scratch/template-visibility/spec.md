# Template Visibility

Feature slug: `template-visibility`

## Problem Statement

Whether a user can launch a template is decided solely by the union of group
whitelists (`group_templates`). There is no per-template way for an owner to
say "this template is available to everyone" or "this template is off-limits
even if a group allows it". The template catalog is browsable by every logged-in
user, but launch permission has no template-level override above the group
grants — an owner who wants broad reach must add many groups to the whitelist,
and an owner who wants to take a template down must remove it from every group.

## Solution

Add a `visibility` field to templates with three values:

- `public` — every authenticated user can launch it, regardless of group
  whitelist.
- `private` (default) — only users whose groups are whitelisted on the template
  can launch it (current behavior).
- `hidden` — nobody can launch it: not whitelisted users, not the owner, not
  admins. No bypass. The owner/admin can still edit the field to bring it back.

Visibility only affects launch authorization; the catalog is still shown to
everyone unchanged. It is set with the same permission as any other template
field (owner with `can_create_template`, or admin). Existing instances are
unaffected — visibility governs future launches only.

## User Stories

1. As a template owner, I want to mark my template public, so that every user
   in the workspace can launch it without needing a group grant.
2. As a template owner, I want to mark my template private, so that only users
   in my whitelisted groups can launch it.
3. As a template owner, I want to mark my template hidden, so that nobody can
   launch it even if a group allows it.
4. As an admin, I want existing templates to stay private after upgrade, so
   that deploying this feature doesn't silently open or lock existing
   templates.
5. As a user with no group grants on a public template, I want to launch it, so
   that publishing a template makes it available to everyone.
6. As a user whose group is whitelisted on a hidden template, I want my launch
   attempt to be rejected, so that hidden is an absolute off-switch.
7. As an admin, I want launch attempts on a hidden template to be rejected for
   me too, so that there is no launch bypass for hidden content.
8. As a user, I want the Quick Launch cards and Templates panel badges to show
   "May launch" / "Not allowed" according to the visibility override, so that
   the UI matches what the backend will accept.
9. As a user, I want the dashboard "Allowed templates" count to reflect what I
   can actually launch (including public templates, excluding hidden ones), so
   that the number is meaningful.
10. As a template owner, I want to set visibility with the same edit permission
    as other fields, so that no extra admin round-trips are needed.
11. As a developer, I want new templates to default to private, so that the
    existing behavior is preserved until someone opts into public/hidden.
12. As a user, I want to launch a public template only if I'm still under my
    instance ceiling and the host limit, so that public visibility grants
    permission but not an unlimited quota.

## Implementation Decisions

1. **DB schema** — add a `visibility` column to `workspace_templates`, NOT NULL,
   default `'private'`. New migration `m20260803_000021_add_template_visibility.rs`
   following the existing `mYYYYMMDD_NNNNNN_snake_case.rs` pattern. The sea-orm
   entity model and the `WorkspaceTemplate` struct plus its `From` impl gain the
   field. Values are constrained to the three allowed strings by Rust-side
   validation (not a DB CHECK constraint); an invalid value on create/update is
   rejected by the API.

2. **API contract** — `template_to_json` emits `"visibility"` with one of
   `public` / `private` / `hidden`. `CreateTemplateRequest` and
   `UpdateTemplateRequest` accept `visibility` with a serde default of
   `private`, so existing clients that omit the field keep working. Invalid
   values produce a client error. A Rust enum wrapping the three literals is
   used at the API boundary so the default and validation are centralized.

3. **BE launch gating — one seam** — extend `pre_flight` in `effective_context.rs`
   with a `template_visibility` argument. Check order becomes: visibility first,
   then the whitelist, then the instance ceiling, then the host ceiling.
   - `hidden` → `PreflightReject::TemplateHidden { requested_template_id }`.
   - `public` → skip the whitelist check.
   - `private` → current whitelist check (`TemplateNotAllowed`).
   `activate` in `activation.rs` passes `request.template.visibility`
   (the `ActivationRequest` already carries the `&WorkspaceTemplate`).
   `allowed_template_ids` in the effective context stays a pure group-union base;
   visibility is applied as an override at the launch gate only. Ceiling checks
   are unchanged — public grants permission, not quota.

4. **FE launch gating** — `mayLaunchTemplate(ctx, template)` in `permissions.ts`
   becomes: `public` → true; `hidden` → false; `private` →
   `ctx.allowed_template_ids.includes(template.id)`. All current call sites
   (Quick Launch cards, Templates panel badges) consume this predicate, so no
   other gating changes are needed.

5. **FE model and form** — the `Template` type gains
   `visibility: 'public' | 'private' | 'hidden'`. The template form gains a
   visibility selector (default `private` for new templates); create/update
   payloads carry it. `TemplateFormState` and the load/submit/update helpers
   pass it through.

6. **Dashboard count label** — `allowedTemplateLabel` on the dashboard is
   computed as the number of loaded templates for which `mayLaunchTemplate` is
   true, instead of `allowed_template_ids.length`, so the count reflects the
   override.

7. **Defaults** — migration default `private`, new-template form default
   `private`, API serde default `private`. All three preserve current behavior.

8. **Edit permission** — visibility is set under the existing template edit
   permission (owner with `can_create_template`, or admin). No new permission
   rule.

9. **No bypass** — `hidden` blocks everyone, including owner and admins; there
   is no `is_admin` bypass in the launch gate anywhere.

10. **No retroactive effect** — changing a template's visibility affects future
    launches only; existing instances keep running.

## Testing Decisions

- **What makes a good test** — launch authorization is tested at the pure gate
  (`pre_flight`) and at the FE predicate (`mayLaunchTemplate`), plus one
  end-to-end test proving the API enforces the same rule, so FE and BE cannot
  drift. Only the accepted/rejected outcome is asserted, never internals.
- **BE unit — `effective_context.rs`** — extend the existing `#[cfg(test)]`
  module with a visibility matrix: public launches with an empty whitelist;
  public still respects the ceiling checks; hidden rejects even when
  whitelisted; private keeps current behavior. Prior art: the existing
  `pre_flight` tests in that file.
- **FE unit — `permissions.test.ts`** — visibility matrix for
  `mayLaunchTemplate` (public/private/hidden × whitelisted/not). Prior art: the
  existing `mayLaunchTemplate` tests.
- **E2E — `flat_rbac_e2e_test.rs`** — a new test that (1) creates a template and
  launches it as a user with no group grants after marking it public, (2) marks
  it hidden and asserts launch is rejected for a whitelisted user, and (3)
  asserts an admin is also rejected on the hidden template. Prior art:
  `test_flat_rbac_2_tiers_end_to_end`.
- **Migration/defaults** — a `db_test.rs` assertion that upgrading leaves
  existing rows at `private`, plus E2E coverage that new templates created
  without the field are private. Prior art:
  `flat_rbac_migration_tolerates_custom_groups_with_system_names`.

## Out of Scope

- Per-group visibility overrides (a different visibility per group).
- Changing the catalog display/filtering by visibility.
- Visibility-aware instance controls (already governed by owner/tier rules).
- A DB-level CHECK constraint or Postgres enum type.
- Audit logging of visibility changes.

## Further Notes

- The three values are modeled as a string column with Rust-side validation; a
  small Rust enum at the API boundary centralizes the default and validation.
- Tickets will follow the three-ticket pattern: `01-be-template-visibility`,
  `02-fe-template-visibility`, `03-int-end-to-end`.
