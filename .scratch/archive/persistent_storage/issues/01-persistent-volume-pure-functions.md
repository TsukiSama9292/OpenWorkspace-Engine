# 01 — Persistent-volume pure-function module

**What to build:** a pure, Docker-free module (mirroring `network_qos.rs`) that is the single source of truth for the three persistence decisions every other ticket relies on: the per-Instance host data path, its derived Docker Volume name, and the in-container mount target. It validates the path so no user-supplied or template-supplied value can escape the configured root.

**Blocked by:** None — can start immediately

**Status:** completed

- [ ] `resolve_persistent_host_path(root, template_name, owner_user_id)` builds `{root}/{template_name}/{user_id}` and returns an error for: relative root (not `/`-prefixed), a `..` segment, empty segments, or injection characters in any component
- [ ] `persistent_volume_name(resolved_host_path)` returns a stable, unique, Docker-legal name (lowercase, no `/`, length < 255) derived only from the host path — so Template renames never change an existing Instance's volume
- [ ] `persistent_container_target(remote_type)` returns `/home/kasm-user` for kasmvnc and `/home/ow_user` for ttyd/jupyter
- [ ] Unit tests cover: valid path assembly, relative/`..`/injection rejection, `persistent_storage_path = NULL` → disabled, volume-name stability + uniqueness + legality, per-remote-type mount targets
- [ ] Zero warnings under both default and `docker` feature gates (`scripts/check.sh`)
