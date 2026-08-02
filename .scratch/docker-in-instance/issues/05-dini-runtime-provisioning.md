# 05 — DinI Runtime Provisioning

**What to build:** When a DinI-enabled template is launched, the instance gets the runtime-appropriate security configuration — `--privileged` under both `runsc` (safe, sandbox-confined) and `runc` (high-risk, warned in the UI), a `tmpfs` at `/var/lib/docker` with `exec`, and the `OW_DOCKER_IN_INSTANCE=true` environment variable — while DinI-off instances keep today's hardened defaults exactly.

**Blocked by:** 04 — DinI Template Surface, 01 — Port-Pool Networking for Instances

**Status:** ready-for-agent

- [ ] A pure function maps `(docker_in_instance, runtime)` to `{privileged, cap_drop, tmpfs, dind_env}`, covering all three security-matrix rows.
- [ ] Launch and start apply `privileged`, the `/var/lib/docker` tmpfs (`exec,mode=755`), and `OW_DOCKER_IN_INSTANCE=true` exactly when DinI is on; no capability drops are applied then.
- [ ] With DinI off, instances keep `privileged=false` and the `NET_RAW`/`NET_ADMIN` capability drops.
- [ ] Real-Docker integration test (Seam 3) confirms the config lands on the created container and that the `runsc` runtime pass-through is unchanged.
