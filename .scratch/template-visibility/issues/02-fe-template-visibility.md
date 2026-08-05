# 02 — Visibility selector, launch gating, dashboard count

**Track:** frontend

**What to build:** The entire template-visibility frontend in one continuous pass, built against the backend schema delivered in ticket 01 — `svelte-check` and the Vitest suite green before the integration ticket starts. A template owner can pick a visibility for a template in the editor, and every user sees "May launch" / "Not allowed" and the dashboard allowed-template count that match the backend's launch gate.

**Contract and form:** The `Template` type gains a `visibility` field (`public` | `private` | `hidden`). The template form gains a visibility selector that defaults to `private` for new templates; the form state and the load/submit/update helpers carry the value through create and update payloads, and the editor can change it under the same edit permission as the other fields.

**Launch gating:** The `mayLaunchTemplate` predicate applies the visibility override: `public` → always allowed, `hidden` → never allowed, `private` → group-whitelist membership as today. All existing call sites — the Quick Launch cards and the Templates panel badges — consume this predicate, so their "May launch" / "Not allowed" rendering follows automatically.

**Dashboard count:** The "Allowed templates" count on the dashboard is computed from the loaded templates for which the predicate is true (public included, hidden excluded), instead of the raw group-whitelist count, so the number reflects the override.

All new UI follows the existing design language so the app stays visually consistent.

**Blocked by:** 01-be-template-visibility

**Status:** ready-for-agent

- [ ] `Template` type carries `visibility`; form has a visibility selector defaulting to `private`; create/update payloads carry it
- [ ] `mayLaunchTemplate` applies public/hidden/private override; Quick Launch cards and Templates panel badges reflect it
- [ ] Dashboard "Allowed templates" count reflects actually-launchable templates
- [ ] `permissions.test.ts` visibility matrix + form default tests green
- [ ] `pnpm check` zero errors, Vitest suite green, no dead code
