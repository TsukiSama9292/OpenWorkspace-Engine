# 08 — DinI Smoke Test

**What to build:** An operator-run script that proves Docker-in-Instance works end-to-end on both runtimes after a host upgrade or image change — the same shape as the existing bandwidth smoke test.

**Blocked by:** 06 — In-Repo DinI Images and Entrypoint Contract, 07 — gVisor Host Provisioning Script

**Status:** ready-for-agent

- [ ] Runs against both `runsc` and `runc` instances.
- [ ] Verifies `dockerd` becomes ready (via `docker info`) within 15 s under `OW_DOCKER_IN_INSTANCE=true`.
- [ ] Verifies a nested `--network=host` container (e.g. nginx) is reachable at `localhost` inside the instance.
- [ ] Verifies a nested container bind-mounting the persistent home directory writes through to the host.
- [ ] Reports a clear pass/fail per check and a non-zero exit on any failure.
