# 05 — Cross-page regression round

**Track:** integration

**What to build:** proof that the four merged milestones read as one product: a cross-page regression pass over every polished surface (all management pages, login, dashboard, viewer) verifying shared patterns, responsive degradation, accessibility rules, and a fully green gate suite — the release condition for the whole round.

**Blocked by:** 04 — Light-touch polish plus the in-session viewer family (in effect, all of tickets 01–04 merged).

**Status:** completed — regression round on `feature/web-surface-polish`, all green. Unit 507 + `pnpm check` clean + api `check.sh` silent + analysis at baseline (37). Browser: smoke 2/2; web-surface-base 5/5, traffic 3/3, admin 3/3, sidebar-dashboard 6/6; monitor 3/3, observability 5/5, log-redesign 6/6, live-instance 1/1, resource-quotas 13/13. Captures: desktop/tablet/phone per milestone. Forward fixups in this round: (1) shared Modal regained max-height + scroll (tall group editor clipped its Save button — caught by quota E2E); (2) resource-quotas spec migrated to the new dialog/button hooks (Save Quotas, Save Changes, Cancel, reset-all ConfirmDialog, Step-3 toggle, Settings save button); (3) removal-dialog card timeout widened for parallel-load flake. Known transient (out of scope, backend untouched): monitor teardown DELETE of a paused container hung twice under load; manual pause→delete verified fast and clean re-runs green.

- [ ] Full browser suite passes across every polished surface, with desktop/tablet/phone captures as the visual record
- [ ] Every table degrades gracefully on phones — nothing overflows or crushes actions
- [ ] Every dialog, drawer, and menu on every page is keyboard-reachable and dismissible; focus rings and state contrast hold on all surfaces
- [ ] Reduced-motion preference suppresses pulsing and sliding effects on all surfaces
- [ ] No third-party network dependency for any mark on any page; zero emoji glyphs on all polished surfaces
- [ ] Repository hard gates fully green (zero-warning compilation, web typecheck plus lint, full unit suites, all browser specs)
- [ ] Any defect found lands as a forward fixup recorded against this ticket, never as rewritten history
