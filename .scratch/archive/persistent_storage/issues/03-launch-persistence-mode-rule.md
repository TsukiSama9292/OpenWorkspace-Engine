# 03 — Launch route: persistence mode + one-persistent-Instance rule + server-side path

**What to build:** launching an Instance lets the user pick a persistence mode (use / no / reset), the client no longer supplies any host path, and the API resolves + stores the host path itself. A single-persistence rule rejects a second persistent Instance on the same (Template, owner), and a Template with no configured root dir degrades gracefully to non-persistent.

**Blocked by:** 01 — Persistent-volume pure-function module, 02 — DockerService volume lifecycle

**Status:** done

- [x] `LaunchInstanceRequest` gains a persistence mode (`use_persistent` / `no_persistent` / `reset_persistent`, default `no_persistent`); the client-supplied `resolved_volume_host_path` field is removed and any incoming value is ignored. (`mount_persistent` is retained as a backward-compat boolean — when no `persistence` mode is sent, `mount_persistent: true` maps to `use_persistent`.)
- [x] API resolves the host path server-side via ticket 01 using the Template's `persistent_storage_path`, Template name, and the authenticated User, and persists it on the Instance record (alongside `mount_persistent`)
- [x] When the mode is `use_persistent` or `reset_persistent` and a `mount_persistent = true` Instance already exists for the same (template, owner) → 409 with a clear error message; `no_persistent` is never blocked. Enforced via new `WorkspaceInstanceRepository::find_persistent_by_template_and_owner`.
- [x] `use_persistent` launch: prepare volume via ticket 02 (helper + volume + create container), Instance lands in `starting` → `running` with `mount_persistent = true` and the resolved path stored. `reset_persistent` also prepares the volume (same as `use_persistent`) at this stage — the remove-then-re-prepare ordering and the wipe belong to ticket 04; the 409-on-reset rule is live now.
- [x] Template with `persistent_storage_path = NULL` degrades to `no_persistent` behaviour (no volume, no rejection). An invalid (non-absolute / traversal) configured root is rejected with 400.
- [x] Route tests (`instances_mock_test.rs`) green: 409 rule (second persistent launch for same template+owner, including `reset_persistent`), 409 is per-owner, ignored client path, degraded NULL-root, successful persistent launch persists the server-resolved path and passes it to `prepare_persistent_volume` with the derived volume name
- [x] Zero warnings under both feature gates (`scripts/check.sh`)
- [x] Launch failure at any stage (helper container / Volume / container creation) marks the Instance `error` and keeps the DB record — volume prep now runs *after* `instance_repo.launch`, so a helper/Volume failure no longer 500s with no record; the response mirrors the existing container-creation error path (200, `instance.status = "error"`, `docker_error` field)
- [x] A broken (`error`) persistent Instance can be replaced: the 409 one-persistent rule is skipped for `error`-state records, the stale record is deleted, and the new launch wipes + re-prepares
- [x] Restart re-declares a lost Volume: `start_instance` calls new `DockerService::ensure_persistent_volume` before creating a container for a persistent Instance (re-creates the local-bind declaration via `create_volume`, never re-populates data) — no silent plain named volume
- [x] Migration backfill: on restart of a legacy `mount_persistent = true` Instance with empty `resolved_volume_host_path`, the API resolves + persists the path (spec §補充) before ensuring the Volume and mounting it

**Notes:**
- Real-Docker route test `test_launch_persistent_resolves_path_server_side` (`docker_lifecycle_test.rs`) launches with `persistence: "use_persistent"` on a template with `persistent_storage_path`, asserts the stored path is `{root}/{template_name}/{owner_id}` and the client path is ignored, then deletes the instance and removes the volume + host dir.
- The two legacy `docker_lifecycle_test.rs` tests were rewritten: `test_launch_with_mount_persistent` → `test_launch_persistent_null_root_degrades_and_ignores_client_path`; `test_launch_with_resolved_volume_path` → `test_launch_persistent_resolves_path_server_side`.
- The docker_lifecycle templates must set `cores: 0, memory: 0` explicitly — the API defaults (2 cores / 4GB) trigger the documented cgroup-permission start failure in this environment.
- `db_test.rs` calls the repo `launch` directly, so its `mount_persistent` / `resolved_volume_host_path` assertions are unaffected.
- Gap fixes (found by code review) landed after the original T03 work: `instances.rs` `launch_instance` reordered (record created before volume prep), `launch_error_response` helper (shared by volume-prep and container failures), `start_instance` backfill + `create_container_for_start` helper, `db.rs::update_resolved_volume_host_path`, `docker.rs::ensure_persistent_volume` + shared `create_local_bind_volume`. New tests: `instances_mock_test.rs` (volume-prep failure keeps error record, reset remove-failure, reset replaces broken instance, start backfill + ensure, restart re-declares lost volume, ensure failure → 500) and `docker_test.rs::test_ensure_persistent_volume_redeclares_lost_volume`.
