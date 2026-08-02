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
    local json="${1:-$TMP/daemon.json}"
    NO_SUDO=1 \
    SKIP_RUNSC_INSTALL=1 \
    SKIP_DAEMON_RELOAD=1 \
    DOCKER_DAEMON_JSON="$json" \
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

# 4. runsc already registered with a custom path + runtimeArgs:
#    merge preserves the entry and only adds the required flags.
REG_DIR="$TMP/registered"
mkdir -p "$REG_DIR"
cat > "$REG_DIR/daemon.json" <<'JSON'
{
  "data-root": "/mnt/M2_7400/docker",
  "storage-driver": "overlay2",
  "runtimes": {
    "runsc": {
      "path": "/usr/bin/runsc",
      "runtimeArgs": ["--nvproxy"]
    }
  }
}
JSON
cp "$REG_DIR/daemon.json" "$REG_DIR/orig-runsc.json"
run_script "$REG_DIR/daemon.json"
python3 - "$REG_DIR/daemon.json" <<'PY' || { echo "FAIL: 4a"; exit 1; }
import json, sys
d = json.load(open(sys.argv[1]))
r = d["runtimes"]["runsc"]
assert r["path"] == "/usr/bin/runsc", r
assert "--nvproxy" in r["runtimeArgs"], r
assert r["runtimeArgs"].count("--net-raw") == 1, r
assert r["runtimeArgs"].count("--allow-packet-socket-write") == 1, r
assert d["data-root"] == "/mnt/M2_7400/docker", d
assert d["storage-driver"] == "overlay2", d
PY
[ -f "$REG_DIR/daemon.json.bak" ] || { echo "FAIL: 4b backup not created"; exit 1; }
cmp -s "$REG_DIR/daemon.json.bak" "$REG_DIR/orig-runsc.json" || { echo "FAIL: 4c backup != original"; exit 1; }
echo "ok: preserves existing runsc path/runtimeArgs, adds required flags"

# 5. Idempotent: re-run with a registered runsc entry is a byte-level no-op.
cp "$REG_DIR/daemon.json" "$REG_DIR/runsc-merged.json"
cp "$REG_DIR/daemon.json.bak" "$REG_DIR/runsc-bak-before.json"
run_script "$REG_DIR/daemon.json"
cmp -s "$REG_DIR/daemon.json" "$REG_DIR/runsc-merged.json" || { echo "FAIL: 5a rewritten"; exit 1; }
cmp -s "$REG_DIR/daemon.json.bak" "$REG_DIR/runsc-bak-before.json" || { echo "FAIL: 5b backup overwritten"; exit 1; }
echo "ok: re-run with registered runsc is a no-op"

# 6. runsc presence detection (skip-install logic, no network needed).
mkdir -p "$TMP/bin"
printf '#!/bin/sh\n' > "$TMP/bin/runsc"
chmod +x "$TMP/bin/runsc"
(
    export RUNSC_INSTALL_DIR="$TMP/bin"
    # shellcheck disable=SC1090
    source "$SCRIPT"
    runsc_is_installed || { echo "FAIL: 6 present binary not detected"; exit 1; }
)
rm "$TMP/bin/runsc"
(
    export RUNSC_INSTALL_DIR="$TMP/bin"
    # shellcheck disable=SC1090
    source "$SCRIPT"
    runsc_is_installed && { echo "FAIL: 6 absent binary detected as installed"; exit 1; }
    [ "$(host_arch)" != "" ] || { echo "FAIL: 6 host_arch empty"; exit 1; }
)
echo "ok: runsc presence detection"

echo "ALL TESTS PASSED"
