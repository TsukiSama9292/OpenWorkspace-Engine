# 08 — DinI Smoke Test

**What to build:** An operator-run script that proves Docker-in-Instance works end-to-end on both runtimes after a host upgrade or image change — the same shape as the existing bandwidth smoke test.

**Blocked by:** 06 — In-Repo DinI Images and Entrypoint Contract, 07 — gVisor Host Provisioning Script

**Status:** done

- [x] Runs against both `runsc` and `runc` instances.
- [x] Verifies `dockerd` becomes ready (via `docker info`) within 15 s under `OW_DOCKER_IN_INSTANCE=true`.
- [x] Verifies a nested `--network=host` container (e.g. nginx) is reachable at `localhost` inside the instance.
- [x] Verifies a nested container bind-mounting the persistent home directory writes through to the host.
- [x] Reports a clear pass/fail per check and a non-zero exit on any failure.

## Notes

- **Script:** `scripts/dini_smoke_test.sh`, mirroring `apps/api/scripts/apply_bw_smoke.sh` (`set -euo pipefail`, `trap cleanup EXIT`, prereq checks, `==>` progress + explicit PASS/FAIL, non-zero exit on any failure). Operator-run, needs only host docker access (no sudo). Overridable via `RUNTIMES`, `IMAGE`, `HOME_MOUNT`, `DOCKER_DAEMON_JSON`, `PORT`. It provisions each instance exactly like the API does for a DinI-on container: `--network bridge`, `--privileged`, `/var/lib/docker` tmpfs (`exec,mode=755`), persistent-home bind mount, `OW_DOCKER_IN_INSTANCE=true`.
- **Host-prereq check:** when `runsc` is in the runtime list the script verifies `/etc/docker/daemon.json` has the Docker-in-gVisor `runtimeArgs` (`--net-raw`, `--allow-packet-socket-write`), failing with a hint to run `sudo bash scripts/docker-runtime-gvisor.sh`. This is the exact misconfiguration the original runsc failures traced back to.
- **Nested reachability check** uses a `busybox:1` `--network=host` httpd on a per-run port, polled from inside the instance (the spec's "e.g. nginx" is an example; busybox avoids an extra image pull).
- **Empirical findings this ticket surfaced (all resolved):**
  1. Host runsc `runtimeArgs` were only `--nvproxy`; Docker v29 in gVisor requires `--net-raw --allow-packet-socket-write` (per `references_repo/gvisor/g3doc/user_guide/tutorials/docker-in-gvisor.md`). Applied to the host via `scripts/docker-runtime-gvisor.sh`.
  2. A `sudo -n`-based bootstrap dies under `runsc` — gVisor does not honor the SUID bit by default (`allow-suid=false`), so `sudo` reports `effective uid is not 0` before reaching sudoers. Fixed without any new host flag by making the `_dini` images root-first (start dockerd as root, `setpriv`-drop to the service user), matching the official recipe; see issue 06 Notes.
  3. `fuse-overlayfs` is broken for nested containers in the gVisor sandbox (`transport endpoint is not connected`); under `runsc` the bootstrap selects `overlay2` on the tmpfs instead (see issue 06 Notes).
  4. **Instances on `ow-network` break in-instance DNS under `runsc`.** Docker's embedded resolver does not bind at `127.0.0.11:53` inside a runsc sandbox, so any container on a user-defined bridge gets `nameserver 127.0.0.11` in `resolv.conf` and cannot resolve at all — even `curl` fails in the instance, and nested `docker run hello-world` dies with `lookup ... on 127.0.0.11:53: connection refused`. Fixed (in this iteration) by moving all instances to the default `bridge` network (spec decision 1 revision): the API no longer derives `network_mode` from `DOCKER_NETWORK`, instances stay off `ow-network`, and port publishing/health/Traefik paths are unchanged (they already target the `172.17.0.1` gateway). The smoke script pins `--network bridge` so it keeps matching the API path.

   > **Superseded by `.scratch/archive/docker-in-instance_2`.** The default-`bridge` fix above was itself replaced: sharing `docker0` left instances (and nested `--network=host` services) peer-reachable by other tenants, so the successor spec moves every instance to a dedicated `/30` bridge and solves the runsc DNS break by rewriting `/etc/resolv.conf` in-image from `OW_DNS` (the images already implement the rewrite; see issue 06). Consequently the smoke script's `--network bridge` pin no longer matches the API's launch path; the successor isolation + DNS smoke test is tracked in `docker-in-instance_2` issue 08.
- **Live run:** `./scripts/dini_smoke_test.sh` → 3/3 PASS on both `runc` and `runsc` (dockerd ready 1–2 s; nested host-network reachable; write-through reached the host). Negative prereq path returns exit 1 with the actionable message. (At the time the smoke test ran on the default `bridge` network, which is why it always passed while the live `ow-network` instance failed DNS — see the superseded note above.)
