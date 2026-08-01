# 05 — Frontend: launch modal persistence option + form root-dir + persistent badge

**What to build:** a User can choose the persistence mode when launching, Template owners set a persistent root directory (labelled as such), and a persistent Instance is visually distinguishable — while keeping the payload contract from ticket 03.

**Blocked by:** 03 — Launch route: persistence mode + one-persistent-Instance rule

**Status:** done

- [x] Launch modal (next to the current-tab / new-tab selector) gains a 資料持久化 select: 使用資料持久化 / 不使用資料持久化 (default) / 重置資料持久化; the chosen mode is sent with the launch request and no client-supplied host path is sent
- [x] `use_persistent` / `reset_persistent` both set `mount_persistent = true` on the outgoing payload
- [x] Template form's persistent-storage field is relabelled 持久化根目錄 and its placeholder/hint no longer advertises `{template_name}` / `{user_id}` template variables (root dir only, placeholders appended by the API); create + edit prefill from the Template
- [x] Persistent Instance cards show a 持久 badge when `mount_persistent` is true
- [x] Frontend tests (`template-form`, dashboard view) green; `pnpm check` 0 errors
