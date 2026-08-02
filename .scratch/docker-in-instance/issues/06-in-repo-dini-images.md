# 06 — In-Repo DinI Images and Entrypoint Contract

**What to build:** Tenants launching a DinI template get a working in-instance Docker daemon that is ready before the main service starts. The images for the three remote types are built and version-controlled in this repo, so the API↔image contract (env var name, tmpfs mount, `dockerd` flags) cannot drift.

**Blocked by:** 05 — DinI Runtime Provisioning

**Status:** ready-for-agent

- [ ] A shared entrypoint + Dockerfiles for the KasmVNC / ttyd / Jupyter variants build in this repo.
- [ ] With `OW_DOCKER_IN_INSTANCE=true`, the entrypoint starts `dockerd --iptables=false --ip6tables=false --data-root=/var/lib/docker`, polls readiness via `docker info` (15 s timeout; logs and exits non-zero on failure), then execs the main service.
- [ ] Without the env var, the entrypoint behaves exactly as today.
- [ ] Template default images point at the new in-repo images, and the deploy flow builds them.
