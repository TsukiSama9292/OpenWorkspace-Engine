# 05 — DinI Runtime Provisioning

**What to build:** When a DinI-enabled template is launched, the instance gets the runtime-appropriate security configuration — `--privileged` under both `runsc` (safe, sandbox-confined) and `runc` (high-risk, warned in the UI), a `tmpfs` at `/var/lib/docker` with `exec`, and the `OW_DOCKER_IN_INSTANCE=true` environment variable — while DinI-off instances keep today's hardened defaults exactly.

**Blocked by:** 04 — DinI Template Surface, 01 — Port-Pool Networking for Instances

**Status:** done

- [x] A pure function maps `(docker_in_instance, runtime)` to `{privileged, cap_drop, tmpfs, dind_env}`, covering all three security-matrix rows.
- [x] Launch and start apply `privileged`, the `/var/lib/docker` tmpfs (`exec,mode=755`), and `OW_DOCKER_IN_INSTANCE=true` exactly when DinI is on; no capability drops are applied then.
- [x] With DinI off, instances keep `privileged=false` and the `NET_RAW`/`NET_ADMIN` capability drops.
- [x] Real-Docker integration test (Seam 3) confirms the config lands on the created container and that the `runsc` runtime pass-through is unchanged.

## Notes

- `dini_security_profile(docker_in_instance, runtime)` in `apps/api/src/docker.rs` returns `DiniSecurityProfile { privileged, cap_drop, tmpfs, dind_env }`. Off → `privileged=false`, `cap_drop=[NET_RAW, NET_ADMIN]`, no tmpfs, no env. On (both `runsc` and `runc`) → `privileged=true`, `cap_drop=None`, `tmpfs /var/lib/docker:exec,mode=755`, `OW_DOCKER_IN_INSTANCE=true`. `runtime` is part of the mapping (why elevated is acceptable) though both on-rows emit identical config.
- `ContainerConfig` gains `docker_in_instance: bool`; both `ContainerConfig` builders — launch (`instances.rs`) and `build_and_create_container` (start/reset path) — pass `template.docker_in_instance`. The host config and env are derived from the profile in `create_container_from_template`.
- Tests: 3 Seam-1 lib tests (all three matrix rows), 3 Seam-3 tests in `docker_test.rs` — DinI-off keeps hardened defaults, DinI-on lands `privileged`/tmpfs/env on the created container (inspected), and `runsc` runtime pass-through is verified by actually booting a gVisor container (gated on `runsc_supported()`). Full suite: 436/436, `check.sh` clean.
