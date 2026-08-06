# 14 — Backend contract: drop legacy columns and flip runtime default

**Track:** backend

**What to build:** The contract half of the wide refactor. A migration drops `users.role`, `users.instance_limit`, `users.max_cpu_cores`, `users.max_ram_bytes`, `workspace_templates.allocation_mode`, and the quota fields in `system_settings` (`max_cpu_cores`, `max_ram_bytes`, `shared_max_cpu`, `shared_max_ram`). The default `OW_CONTAINER_RUNTIME` flips from `"docker"` to `"runsc"`, so templates that do not pin a runtime resolve to gVisor. `docker_in_instance` and its sandboxed profile are untouched.

**Blocked by:** 12-be-volume-registry

**Status:** completed

- [ ] Migration drops all listed columns; `down` restores them
- [ ] The API compiles and runs with the legacy columns gone (no remaining reads)
- [ ] `apps/api/scripts/check.sh` produces zero warnings in both feature configurations
- [ ] The runtime default is `runsc`; runtime-resolution tests updated and green
