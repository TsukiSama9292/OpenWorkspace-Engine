#!/bin/bash
# 確保主機的 inotify 上限夠大，讓 Traefik 的 file provider 能建立 watcher。
#
# 背景：fs.inotify.max_user_instances 是 per-uid 的上限。Traefik 容器以 root
#       執行，而 root 的配額常被 dockerd/containerd 用滿；一旦用滿，
#       inotify_init 會回 EMFILE ("too many open files")，Traefik 無法監看
#       /etc/traefik/dynamic，dev/prod 的路由都不會載入 (http://localhost 404)。
#
# 行為 (idempotent)：
#  1. 讀取目前的 sysctl 值。
#  2. 目前值已 >= 目標值 → 略過，不寫入。
#  3. 否則寫入 runtime 值，並在 CHANGED 時持久化到 SYSCTL_CONF。
#
# 測試用覆寫環境變數 (皆不會影響正式行為的預設值)：
#   NO_SUDO=1           不經 sudo 執行寫入 (供非 root 測試)
#   SYSCTL_CONF         持久化設定檔路徑     (預設 /etc/sysctl.d/99-inotify.conf)
#   INSTANCES_TARGET    目標 max_user_instances (預設 1024)
#   WATCHES_TARGET      目標 max_user_watches   (預設 1048576)

set -euo pipefail

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
    VERBOSE=0
    for arg in "$@"; do
        case "$arg" in
            --verbose|-v) VERBOSE=1 ;;
        esac
    done
fi
VERBOSE="${VERBOSE:-0}"

log() {
    if [ "$VERBOSE" -eq 1 ]; then
        echo "$@"
    fi
}

NO_SUDO="${NO_SUDO:-0}"
SYSCTL_CONF="${SYSCTL_CONF:-/etc/sysctl.d/99-inotify.conf}"
INSTANCES_TARGET="${INSTANCES_TARGET:-1024}"
WATCHES_TARGET="${WATCHES_TARGET:-1048576}"

SUDO_PREFIX=""
if [ "$NO_SUDO" = "0" ] && [ "$(id -u)" != "0" ]; then
    SUDO_PREFIX="sudo"
fi

# proc_sys <key> — 把 sysctl key (fs.inotify.max_user_instances) 轉成 /proc/sys 路徑。
proc_sys() {
    echo "/proc/sys/${1//./\/}"
}

# current_value <key> — 輸出目前值 (純數字)，讀取失敗則為空。
current_value() {
    cat "$(proc_sys "$1")" 2>/dev/null || true
}

# ensure_at_least <key> <target> — 未達標才寫入，並記錄 CHANGED=1。
CHANGED=0
ensure_at_least() {
    local key="$1" target="$2" current
    current="$(current_value "$key")"
    if [ -z "$current" ]; then
        echo "> 警告：無法讀取 $(proc_sys "$key")，略過。" >&2
        return 0
    fi
    if [ "$current" -ge "$target" ]; then
        log "> $key = $current 已 >= $target，無需變更。"
        return 0
    fi
    log "> $key = $current < $target，寫入 $target ..."
    if [ -n "$SUDO_PREFIX" ]; then
        $SUDO_PREFIX sh -c 'echo "$2" > "$1"' sh "$(proc_sys "$key")" "$target"
    else
        echo "$target" > "$(proc_sys "$key")"
    fi
    CHANGED=1
}

# persist_conf — 將兩個 inotify 鍵以目標值寫進 SYSCTL_CONF (同鍵覆寫，保留其它鍵)。
persist_conf() {
    local tmp
    tmp="$(mktemp)"
    if [ -f "$SYSCTL_CONF" ]; then
        grep -vE '^\s*(fs\.inotify\.max_user_instances|fs\.inotify\.max_user_watches)\s*=' \
            "$SYSCTL_CONF" > "$tmp" || true
    fi
    printf 'fs.inotify.max_user_instances = %s\nfs.inotify.max_user_watches = %s\n' \
        "$INSTANCES_TARGET" "$WATCHES_TARGET" >> "$tmp"
    if [ -n "$SUDO_PREFIX" ]; then
        $SUDO_PREFIX install -m 0644 "$tmp" "$SYSCTL_CONF"
    else
        install -m 0644 "$tmp" "$SYSCTL_CONF"
    fi
    rm -f "$tmp"
}

ensure_at_least fs.inotify.max_user_instances "$INSTANCES_TARGET"
ensure_at_least fs.inotify.max_user_watches "$WATCHES_TARGET"

if [ "$CHANGED" = "1" ]; then
    persist_conf
    log "> 已持久化到 $SYSCTL_CONF"
else
    log "> 兩項皆已達標，未做任何變更。"
fi
