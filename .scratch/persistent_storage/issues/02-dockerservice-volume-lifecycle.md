# 02 — DockerService volume lifecycle

**What to build:** the Docker layer can prepare and clean up a persistent data directory for an Instance using the Local Bind-mounted Named Volume mechanism: an API-in-a-container cannot touch host files, so a short-lived `alpine --rm` helper container creates the empty host dir and `chown`s it to 1000:1000, then a named Volume (`driver: local`, `type=none`/`device`/`o=bind`) is created against that path. When the Instance container is created it mounts the **volume by name** at the per-remote-type home dir, letting Docker populate the image's built-in home files on first mount (no masking, no crash).

**Blocked by:** 01 — Persistent-volume pure-function module

**Status:** done

- [x] `DockerService` gains mockable methods (`prepare_persistent_volume(host_path, volume_name)` and `remove_persistent_volume(host_path, volume_name)`), with real implementations on `DockerClient` via Bollard. (Dropped `remote_type` param — the mount target is derived from remote_type at bind time in `create_container_from_template` via `persistent_container_target`.)
- [x] `prepare_persistent_volume` runs an alpine helper container that `mkdir -p`s the host dir and `chown 1000:1000`s it, then creates the local-bind named Volume (driver `local`, opts `type=none` / `device=<host_path>` / `o=bind`) using the volume name from ticket 01
- [x] `create_container_from_template` uses the **volume name** (not the raw host path) in `Binds`, targeting the per-remote-type home dir from ticket 01; behaviour unchanged when persistence is disabled
- [x] `remove_persistent_volume` runs a helper container to empty the host dir and calls Docker's volume-remove
- [x] Real-Docker integration tests (`docker_test.rs`) verify: first mount populates the image's built-in home files; a file written into home survives container recreation; a non-empty host dir is **not** re-populated; volume removal deletes the declaration
- [x] Zero warnings under both feature gates (`scripts/check.sh`)
