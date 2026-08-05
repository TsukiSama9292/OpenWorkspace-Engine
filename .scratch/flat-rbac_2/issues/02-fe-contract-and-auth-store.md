# 02 — Contract types, auth store, tier-aware UI, system-group rules, policy cleanup

**Track:** frontend

**What to build:** The entire flat-rbac_2 frontend in one continuous pass, built against the backend schema delivered in ticket 01. One continuous pass: contract layer first, then the tier-aware views and system-group presentation, ending with the policy-editor cleanup — `svelte-check` and the Vitest suite green before the integration ticket starts.

**Contract types and auth store:** TypeScript types for the effective-context payload carry derived admin status and tier (replacing the `is_system_admin` boolean), the five permission flags, the effective ceiling, allowed templates, and group memberships; the `Group` type gains `kind`; the user-policy payload drops the personal template-whitelist field. Typed API client functions match the pinned endpoints (`/auth/me`, group list, user policy update). The auth store reads derived admin status and the flag helpers from the effective context, and any other consumers of the renamed/changed fields are updated so the app stays type-green. The signed-in admin sees their own effective privileges, including the Admin group's grants.

**User-management UI:** The user-management view is tier-aware. Edit and Delete actions on a user are hidden when the target's tier is at or above the actor's tier (owner-self and root exceptions preserved), and instance stop/delete actions are hidden for instances owned by users of equal or higher tier. The Create User modal gains a multi-select group picker: it lists only the groups the actor may assign (groups of tier strictly below the actor's tier), defaults to the User group selected, and sends the chosen memberships with the create request.

**Group panel system rules:** System groups are visibly marked by their kind. The delete and rename controls are not offered for system groups. The Admin group's permission flags render read-only (on) and the User group's flags render read-only (off); only the Manager group's flags can be edited. The `max_instances` field stays editable for all three system groups.

**Policy editor cleanup:** The user-policy editor matches the group-only template authorization from ticket 01: the "personal template whitelist" section is removed, leaving the personal ceiling and group-membership assignment editable. The contract types no longer reference a personal template whitelist, and its API-client call is gone.

All new UI follows the existing design language so the app stays visually consistent (taste-consistent layout, matching the existing panels).

**Blocked by:** 01-be-system-groups-and-effective-context

**Status:** ready-for-agent

- [ ] Contract types + API client pinned to the backend shapes; auth store consumes derived admin status/tier/flag helpers
- [ ] User-management view hides edit/delete/instance-control on equal-or-higher-tier targets; Create User modal has a tier-filtered multi-select group picker defaulting to User
- [ ] Group panel marks system groups, omits delete/rename, locks Admin/User flags, keeps max editable
- [ ] Policy editor no longer shows or sends a personal template whitelist
- [ ] `pnpm check` zero errors, Vitest suite green, no dead code
