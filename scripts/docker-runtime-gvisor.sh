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

install_runsc() {
    if runsc_is_installed; then
        echo "> runsc 已存在於 $RUNSC_BIN，略過下載。"
        return
    fi
    local arch url
    arch="$(host_arch)"
    url="https://storage.googleapis.com/gvisor/releases/release/${RUNSC_VERSION}/${arch}/runsc"
    echo "> 下載 runsc ($RUNSC_VERSION/$arch) → $RUNSC_BIN"
    $SUDO_PREFIX mkdir -p "$(dirname "$RUNSC_BIN")"
    $SUDO_PREFIX curl -fsSL -o "$RUNSC_BIN" "$url"
    $SUDO_PREFIX chmod +x "$RUNSC_BIN"
}

# merged_daemon_json <path> — 輸出將 runsc runtime 項目合併後的 daemon.json 內容 (不寫檔)。
# <path> 不存在或不是合法 JSON 時視為空物件。
merged_daemon_json() {
    RUNSC_BIN="$RUNSC_BIN" python3 - "$1" <<'PY'
import json, os, sys

path = sys.argv[1]
try:
    with open(path, encoding="utf-8") as f:
        data = json.load(f)
except (FileNotFoundError, json.JSONDecodeError):
    data = {}
runtimes = dict(data.get("runtimes", {}))
runtimes["runsc"] = {
    "path": os.environ["RUNSC_BIN"],
    "runtimeArgs": ["--net-raw", "--allow-packet-socket-write"],
}
data["runtimes"] = runtimes
json.dump(data, sys.stdout, indent=2)
print()
PY
}

# write_merged_daemon_json — 套用合併結果。有實際變更時回傳 0 (需 reload)，否則回傳 1。
write_merged_daemon_json() {
    local dir merged current
    dir="$(dirname "$DOCKER_DAEMON_JSON")"
    merged="$(merged_daemon_json "$DOCKER_DAEMON_JSON")"

    if [ -e "$DOCKER_DAEMON_JSON" ]; then
        current="$(cat "$DOCKER_DAEMON_JSON")"
        if [ "$current" = "$merged" ]; then
            echo "> $DOCKER_DAEMON_JSON 已包含 runsc runtime，略過。"
            return 1
        fi
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
