# 07 — gVisor Host Provisioning Script

**What to build:** A one-command host setup that installs `runsc` and registers it with the Docker daemon, so `runsc`-backed DinI instances work on any machine. Re-running it is a no-op.

**Blocked by:** None — can start immediately.

**Status:** done

- [x] The script is idempotent: skips already-satisfied steps (runsc installed, `runsc` runtime already registered).
- [x] Installs `runsc` to `/usr/local/bin/runsc` (host-appropriate architecture) when missing.
- [x] Merges a `runsc` runtime entry with `runtimeArgs: ["--net-raw", "--allow-packet-socket-write"]` into the Docker daemon config without overwriting existing keys, backing up the original file.
- [x] Reloads or restarts the Docker daemon to apply the change.
- [x] Wired into the same init flow as the existing host network provisioning.

## Notes

- New `scripts/docker-runtime-gvisor.sh`: `host_arch()` (uname → gVisor GOARCH dir), `runsc_is_installed()`, `install_runsc()` (downloads official release for host arch), `merged_daemon_json()` (python3 JSON merge, tolerates missing/invalid file), `write_merged_daemon_json()` (backs up to `.bak` only once, skips when already applied), `reload_docker()` (systemctl reload, falls back to restart).
- Sudo only applied when non-root; override env vars keep the merge testable without sudo/Docker/network (`DOCKER_DAEMON_JSON`, `RUNSC_INSTALL_DIR`, `RUNSC_VERSION`, `SKIP_RUNSC_INSTALL`, `SKIP_DAEMON_RELOAD`, `NO_SUDO`).
- Wired into `pnpm run init` (package.json) alongside `docker-network.sh`.
- Merge is entry-preserving: if a `runsc` entry already exists in daemon.json (custom `path` and/or existing `runtimeArgs`, e.g. `--nvproxy`), it is kept and only the missing `--net-raw` / `--allow-packet-socket-write` flags are appended. `install_runsc` respects a registered `path` (skips download if the registered binary exists, else installs to that path).
- Idempotency compares parsed JSON semantics (not raw text), so re-running never rewrites or re-reloads for formatting differences.
- Test: `scripts/test-docker-runtime-gvisor.sh` (RED → GREEN) — creates file when absent, preserves existing keys + backs up original, preserves a pre-registered runsc entry (path + `--nvproxy`) while adding required flags, idempotent re-run is a byte-level no-op, runsc presence detection. All pass.
