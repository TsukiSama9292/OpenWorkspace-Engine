#!/bin/bash
# Unit-style test for docker-runtime-gvisor.sh — verifies the idempotent
# runsc/daemon.json merge logic. Requires only bash + python3.
# No sudo, no Docker, no network.

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SCRIPT="$ROOT/scripts/docker-runtime-gvisor.sh"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

run_script() {
    NO_SUDO=1 \
    SKIP_RUNSC_INSTALL=1 \
    SKIP_DAEMON_RELOAD=1 \
    DOCKER_DAEMON_JSON="$TMP/daemon.json" \
    RUNSC_INSTALL_DIR="$TMP/bin" \
    bash "$SCRIPT"
}

# 1. Creates the file when absent, with the correct runsc runtime entry.
run_script
python3 - "$TMP/daemon.json" <<'PY' || { echo "FAIL: 1"; exit 1; }
import json, sys
d = json.load(open(sys.argv[1]))
r = d["runtimes"]["runsc"]
assert r["path"].endswith("/bin/runsc"), r
assert r["runtimeArgs"] == ["--net-raw", "--allow-packet-socket-write"], r
assert len(d) == 1, d
PY
echo "ok: creates file with runsc runtime when absent"

# 2. Merges into an existing file, preserving other keys; backs up the original.
cat > "$TMP/daemon.json" <<'JSON'
{"live-restore": true, "runtimes": {"nvidia": {"path": "/usr/bin/nvidia-container-runtime"}}}
JSON
cp "$TMP/daemon.json" "$TMP/original.json"
run_script
python3 - "$TMP/daemon.json" <<'PY' || { echo "FAIL: 2a"; exit 1; }
import json, sys
d = json.load(open(sys.argv[1]))
assert d["live-restore"] is True, d
assert d["runtimes"]["runsc"]["runtimeArgs"] == ["--net-raw", "--allow-packet-socket-write"]
assert d["runtimes"]["nvidia"]["path"] == "/usr/bin/nvidia-container-runtime", d
PY
[ -f "$TMP/daemon.json.bak" ] || { echo "FAIL: 2b backup not created"; exit 1; }
cmp -s "$TMP/daemon.json.bak" "$TMP/original.json" || { echo "FAIL: 2c backup != original"; exit 1; }
echo "ok: merges preserving existing keys, backs up original"

# 3. Idempotent re-run: no rewrite, backup not overwritten.
cp "$TMP/daemon.json.bak" "$TMP/bak-before.json"
cp "$TMP/daemon.json" "$TMP/merged-before.json"
run_script
cmp -s "$TMP/daemon.json.bak" "$TMP/bak-before.json" || { echo "FAIL: 3a backup overwritten"; exit 1; }
cmp -s "$TMP/daemon.json" "$TMP/merged-before.json" || { echo "FAIL: 3b file rewritten"; exit 1; }
echo "ok: re-run is a no-op"

# 4. runsc presence detection (skip-install logic, no network needed).
mkdir -p "$TMP/bin"
printf '#!/bin/sh\n' > "$TMP/bin/runsc"
chmod +x "$TMP/bin/runsc"
(
    export RUNSC_INSTALL_DIR="$TMP/bin"
    # shellcheck disable=SC1090
    source "$SCRIPT"
    runsc_is_installed || { echo "FAIL: 4 present binary not detected"; exit 1; }
)
rm "$TMP/bin/runsc"
(
    export RUNSC_INSTALL_DIR="$TMP/bin"
    # shellcheck disable=SC1090
    source "$SCRIPT"
    runsc_is_installed && { echo "FAIL: 4 absent binary detected as installed"; exit 1; }
    [ "$(host_arch)" != "" ] || { echo "FAIL: 4 host_arch empty"; exit 1; }
)
echo "ok: runsc presence detection"

echo "ALL TESTS PASSED"
