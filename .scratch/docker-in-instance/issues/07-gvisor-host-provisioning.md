# 07 — gVisor Host Provisioning Script

**What to build:** A one-command host setup that installs `runsc` and registers it with the Docker daemon, so `runsc`-backed DinI instances work on any machine. Re-running it is a no-op.

**Blocked by:** None — can start immediately.

**Status:** ready-for-agent

- [ ] The script is idempotent: skips already-satisfied steps (runsc installed, `runsc` runtime already registered).
- [ ] Installs `runsc` to `/usr/local/bin/runsc` (host-appropriate architecture) when missing.
- [ ] Merges a `runsc` runtime entry with `runtimeArgs: ["--net-raw", "--allow-packet-socket-write"]` into the Docker daemon config without overwriting existing keys, backing up the original file.
- [ ] Reloads or restarts the Docker daemon to apply the change.
- [ ] Wired into the same init flow as the existing host network provisioning.
