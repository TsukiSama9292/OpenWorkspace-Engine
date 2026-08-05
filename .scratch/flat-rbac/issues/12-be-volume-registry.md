# 12 — Persistent-volume registry

**Track:** backend

**What to build:** The `persistent_volumes` registry and its lifecycle. Every persistent launch upserts a row keyed by the resolved host path with the instance owner; deleting the last active instance referencing a path flips the row to `orphaned`; deleting a user nulls the owner but keeps the row. A list endpoint (admins + `can_manage_users`) returns the orphaned volumes, and a double-confirmed cleanup endpoint removes the host directory/volume and then the row. No deletion path ever removes the host directory automatically.

**Blocked by:** 10-be-group-membership-api

**Status:** ready-for-agent

- [ ] Launch upserts the registry row; deleting the last referencing instance flips it to `orphaned`
- [ ] Deleting a user leaves the volume as an orphaned row with a null owner
- [ ] The orphaned list is available only to `is_system_admin` and `can_manage_users`
- [ ] Cleanup requires the confirmed action and removes the directory/volume plus the row; nothing auto-deletes host data
