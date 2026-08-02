#!/bin/bash
# 一鍵設定 gVisor (runsc) 為 Docker 執行期：
#  1. 若 runsc 未安裝，下載官方 release 到 RUNSC_INSTALL_DIR (預設 /usr/local/bin)。
#  2. 將 runsc runtime 項目合併進 /etc/docker/daemon.json (JSON merge，保留既有鍵，備份到 .bak)。
#  3. 重新載入/重啟 Docker daemon 套用變更。
# 可重複執行 (idempotent)：已滿足的步驟會略過，第二次執行不會再次載入 daemon。
#
# 測試用覆寫環境變數 (皆不會影響正式行為的預設值)：
#   DOCKER_DAEMON_JSON   daemon.json 路徑        (預設 /etc/docker/daemon.json)
#   RUNSC_INSTALL_DIR    runsc 安裝目錄          (預設 /usr/local/bin)
#   RUNSC_VERSION        runsc release 版本      (預設 latest)
#   SKIP_RUNSC_INSTALL=1  略過 runsc 下載安裝
#   SKIP_DAEMON_RELOAD=1  略過 daemon 重新載入
#   NO_SUDO=1             不經 sudo 執行檔案操作 (供非 root 測試)

set -euo pipefail

DOCKER_DAEMON_JSON="${DOCKER_DAEMON_JSON:-/etc/docker/daemon.json}"
RUNSC_INSTALL_DIR="${RUNSC_INSTALL_DIR:-/usr/local/bin}"
RUNSC_BIN="${RUNSC_INSTALL_DIR%/}/runsc"
RUNSC_VERSION="${RUNSC_VERSION:-latest}"
SKIP_RUNSC_INSTALL="${SKIP_RUNSC_INSTALL:-0}"
SKIP_DAEMON_RELOAD="${SKIP_DAEMON_RELOAD:-0}"
NO_SUDO="${NO_SUDO:-0}"

SUDO_PREFIX=""
if [ "$NO_SUDO" = "0" ] && [ "$(id -u)" != "0" ]; then
    SUDO_PREFIX="sudo"
fi

# 對應 uname -m → gVisor release 的 GOARCH 目錄名。
host_arch() {
    case "$(uname -m)" in
        x86_64|amd64) echo "amd64" ;;
        aarch64|arm64) echo "arm64" ;;
        armv7l) echo "armv7" ;;
        i686|i386) echo "386" ;;
        *) echo "amd64" ;;
    esac
}

runsc_is_installed() {
    [ -x "$RUNSC_BIN" ]
}

# registered_runsc_bin — 輸出 daemon.json 中已註冊的 runsc path (若有)，否則不輸出。
registered_runsc_bin() {
    python3 - "$DOCKER_DAEMON_JSON" <<'PY'
import json, sys
try:
    with open(sys.argv[1], encoding="utf-8") as f:
        data = json.load(f)
except Exception:
    sys.exit(0)
runtimes = data.get("runtimes")
path = runtimes.get("runsc").get("path") if isinstance(runtimes, dict) and isinstance(runtimes.get("runsc"), dict) else None
if isinstance(path, str) and path:
    print(path)
PY
}

install_runsc() {
    local reg target arch url
    reg="$(registered_runsc_bin)"
    if [ -n "$reg" ] && [ -x "$reg" ]; then
        echo "> runsc 已存在於 $reg，略過下載。"
        return
    fi
    if [ -n "$reg" ]; then
        target="$reg"
    else
        target="$RUNSC_BIN"
    fi
    if [ -x "$target" ]; then
        echo "> runsc 已存在於 $target，略過下載。"
        return
    fi
    arch="$(host_arch)"
    url="https://storage.googleapis.com/gvisor/releases/release/${RUNSC_VERSION}/${arch}/runsc"
    echo "> 下載 runsc ($RUNSC_VERSION/$arch) → $target"
    $SUDO_PREFIX mkdir -p "$(dirname "$target")"
    $SUDO_PREFIX curl -fsSL -o "$target" "$url"
    $SUDO_PREFIX chmod +x "$target"
}

# merged_daemon_json <path> — 輸出將 runsc runtime 項目合併後的 daemon.json 內容 (不寫檔)。
# 若 runsc 已存在則保留其 path 與既有 runtimeArgs，只補上必要旗標；<path> 不存在
# 或不是合法 JSON 時視為空物件。
merged_daemon_json() {
    RUNSC_BIN="$RUNSC_BIN" python3 - "$1" <<'PY'
import json, os, sys

path = sys.argv[1]
try:
    with open(path, encoding="utf-8") as f:
        data = json.load(f)
except (FileNotFoundError, json.JSONDecodeError):
    data = {}
runtimes = dict(data.get("runtimes", {})) if isinstance(data.get("runtimes"), dict) else {}
existing = runtimes.get("runsc")
required_args = ["--net-raw", "--allow-packet-socket-write"]
if isinstance(existing, dict):
    entry = dict(existing)
    args = entry.get("runtimeArgs")
    entry["runtimeArgs"] = list(args) if isinstance(args, list) else []
    for arg in required_args:
        if arg not in entry["runtimeArgs"]:
            entry["runtimeArgs"].append(arg)
    runtimes["runsc"] = entry
else:
    runtimes["runsc"] = {
        "path": os.environ["RUNSC_BIN"],
        "runtimeArgs": list(required_args),
    }
data["runtimes"] = runtimes
json.dump(data, sys.stdout, indent=2)
print()
PY
}

# daemon_json_already_applied <path> <merged> — 比較兩者的 JSON 語意是否相同
# (忽略格式差異)，已套用時回傳 0。
daemon_json_already_applied() {
    local path="$1" merged="$2"
    if [ ! -e "$path" ]; then
        return 1
    fi
    if ! MERGED="$merged" python3 - "$path" <<'PY' 2>/dev/null; then
import json, os, sys
try:
    current = json.load(open(sys.argv[1], encoding="utf-8"))
except Exception:
    sys.exit(1)
merged = json.loads(os.environ["MERGED"])
sys.exit(0 if current == merged else 1)
PY
        return 1
    fi
    return 0
}

# write_merged_daemon_json — 套用合併結果。有實際變更時回傳 0 (需 reload)，否則回傳 1。
write_merged_daemon_json() {
    local dir merged
    dir="$(dirname "$DOCKER_DAEMON_JSON")"
    merged="$(merged_daemon_json "$DOCKER_DAEMON_JSON")"

    if daemon_json_already_applied "$DOCKER_DAEMON_JSON" "$merged"; then
        echo "> $DOCKER_DAEMON_JSON 已包含 runsc runtime，略過。"
        return 1
    fi

    if [ -e "$DOCKER_DAEMON_JSON" ]; then
        if [ ! -e "$DOCKER_DAEMON_JSON.bak" ]; then
            $SUDO_PREFIX cp "$DOCKER_DAEMON_JSON" "$DOCKER_DAEMON_JSON.bak"
        fi
    else
        $SUDO_PREFIX mkdir -p "$dir"
    fi

    printf '%s\n' "$merged" | $SUDO_PREFIX tee "$DOCKER_DAEMON_JSON" > /dev/null
    return 0
}

reload_docker() {
    if ! $SUDO_PREFIX systemctl reload docker 2>/dev/null; then
        $SUDO_PREFIX systemctl restart docker
    fi
    echo "> Docker daemon 已重新載入。"
}

main() {
    if [ "$SKIP_RUNSC_INSTALL" != "1" ]; then
        install_runsc
    fi

    if write_merged_daemon_json; then
        if [ "$SKIP_DAEMON_RELOAD" != "1" ]; then
            echo "> 已更新 $DOCKER_DAEMON_JSON，重新載入 Docker daemon..."
            reload_docker
        fi
    fi
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
    main "$@"
fi
