# 04 — Reset & remove cleanup

**What to build:** resetting persistence wipes the old data and starts fresh, and removing an Instance **preserves** its data directory and Volume declaration (only reset / wiping a broken `error` record deletes data) — so a later `use_persistent` launch can reuse the same data.

**Blocked by:** 02 — DockerService volume lifecycle, 03 — Launch route: persistence mode + one-persistent-Instance rule

**Status:** done

- [x] `reset_persistent` launch: call `remove_persistent_volume` (helper `rm -rf` host dir + `remove_volume` on the old declaration) **before** re-preparing the volume, so the next first-mount re-populates the image's built-in home files
- [x] `reset_persistent` is subject to the one-persistent-Instance rule (409 if a persistent Instance exists for the same (template, owner))
- [x] `delete_instance` **keeps** the host data dir and the Volume declaration (only the container / route / DB record are removed), so the data survives and can be reused
- [x] `prepare_persistent_volume` is idempotent: if the Volume declaration already exists (reuse after delete) it leaves the data untouched and reuses the existing Volume
- [x] Route tests (`instances_mock_test.rs`) green: reset ordering (remove-then-prepare), reset 409, delete does **not** call `remove_persistent_volume` (`.never()`)
- [x] Real-Docker tests green: `prepare_persistent_volume` reuse leaves data intact; lifecycle test re-launches `use_persistent` after delete and reuses the preserved volume
- [x] Zero warnings under both feature gates (`scripts/check.sh`)
