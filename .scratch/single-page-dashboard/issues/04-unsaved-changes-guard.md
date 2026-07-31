# 04 — Unsaved-changes guard

**What to build:** When the template editor has unsaved changes, every leave path prompts for confirmation so in-progress edits are never silently discarded.

**Blocked by:** 03 — TemplatePanel with in-tab editor

**Status:** resolved

- [x] Cancel/Back, sidebar tab switch, browser back/forward, and refresh/close all trigger a confirmation when the form is dirty
- [x] Expanding/collapsing the Advanced section does not count as an unsaved change
- [x] Declining the prompt keeps you in the editor (the hash is restored for back/forward)
- [x] A clean form leaves without any prompt
