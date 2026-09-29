# Adding a recognizable, low-risk template family icon

When the dashboard catalog needs a new family mark (e.g. a new template
family), follow this exact pipeline. Goal: maximum recognizability at minimum
legal risk — official-shape monochrome, never full-color brand art, never an
unattributed download.

## 1. Check the license first (blocking)

- The ONLY accepted upstream for brand shapes is **Simple Icons**
  (`https://simpleicons.org/`): its path data is **CC0 1.0**, which covers
  *copyright*. Confirm the slug exists:
  `https://raw.githubusercontent.com/simple-icons/simple-icons/develop/icons/<slug>.svg`
  must return 200.
- **CC0 does NOT clear trademark.** Every brand shape stays the property of
  its owner (Canonical, PSF, …). If a slug is missing from Simple Icons, do
  NOT pull the logo from a random site — draw an in-house abstraction into
  `generic.svg` style instead and stop here.

## 2. Adapt to the unified treatment (required)

Fetch to `/tmp`, then transform — never commit the raw file:

- Drop `<title>…</title>`; set on `<svg>`:
  `fill="currentColor"` + `aria-label="<Name> family mark"` (keep `role="img"`).
- No other edits: no recoloring, no reshaping. The catalog cover supplies the
  per-family duotone; the mark itself stays monochrome so reverts are trivial.

## 3. Wire it in (all three, or it doesn't exist)

1. Save as `apps/web/static/icons/<family>.svg` (lowercase, matches the
   `TemplateFamily` member name).
2. Add the family to `FAMILY_ORDER` in `src/lib/dashboard/artwork.ts`
   (order matters — first substring match wins) and add the
   `family-<name>` cover treatment next to the other
   `.catalog-cover.family-*` rules in `src/routes/+page.svelte`.
3. Record it in `apps/web/static/icons/README.md`: source (Simple Icons +
   fetch date), trademark owner, and the standing warning that commercial
   redistribution must re-check that owner's trademark policy (Canonical's
   Ubuntu policy is the strictest — call it out by name when relevant).

## 4. Verify (all green, or revert)

- `pnpm check` clean; `pnpm test` full suite green (artwork tests assert every
  family resolves plus its `/icons/<family>.svg` src).
- `pnpm build` emits `build/icons/<family>.svg`; `curl` it from the dev
  server (both `:5173` and `:80`) for a 200.
- Eyeball the card: white 52px mark on the brightened duotone cover. If the
  shape is unreadable at 52px, the slug is wrong — pick a closer one or fall
  back to the in-house generic mark. Never "fix" legibility by importing a
  different-licensed file.
