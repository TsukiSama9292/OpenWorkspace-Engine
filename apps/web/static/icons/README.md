# Template family marks

Self-hosted vector marks for the dashboard template catalog
(`familyIconSrc` in `src/lib/dashboard/artwork.ts` → `/icons/<family>.svg`).

- **Sources:** `ubuntu`, `python`, `pytorch`, `rust`, `jupyter` are the
  official brand paths from [Simple Icons](https://simpleicons.org/)
  (path data under CC0 1.0; fetched 2026-09-29), rendered here in a single
  monochrome treatment (`fill="currentColor"`) over the per-family catalog
  cover. `generic.svg` (fallback) is drawn in-house, CC0.
- **Trademarks — please read:** the shapes remain the trademarks of their
  respective owners (Canonical / Python Software Foundation / PyTorch /
  Rust Foundation / Project Jupyter). They are used here solely to identify
  the technology inside a template, with no implication of endorsement.
  If you redistribute this project commercially, review each owner's
  trademark policy first (notably Canonical's Ubuntu IP policy, which is
  the strictest of the set) and be ready to swap a mark for the generic
  fallback.
- **Families:** `ubuntu`, `python`, `pytorch`, `rust`, `jupyter`, `generic`
  (fallback for unknown template names). To add a family: add the name to
  `FAMILY_ORDER` in `artwork.ts`, drop `<family>.svg` here, and add the
  `family-<name>` cover treatment in `src/routes/+page.svelte`.
- **Serving:** copied verbatim to `build/icons/` by `adapter-static` and
  served with `Cache-Control: public, immutable` (30d) by the nginx
  `location /icons/` block in `apps/web/Dockerfile`. No external network
  dependency; air-gapped hosts render identically.
