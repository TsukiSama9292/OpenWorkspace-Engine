#!/usr/bin/env bash
# Single cleanup script for OpenWorkspace Engine.
#
# Subcommands:
#   tests       remove containers/networks left behind by the API test suite
#               (any `ow-*` network except the shared control network
#               `ow-network`, plus `ow_test*`/`ow-vol-*` containers), the
#               persistent-storage named volumes (`ow-persist-*`), and any
#               per-instance route files in the dev traefik dynamic dir
#   volumes     remove persistent-storage Docker named volumes (`ow-persist-*`)
#   instances   remove per-instance networks (`ow-<instance-id>`) and their
#               containers
#   network     remove the shared control network `ow-network`
#   traefik     remove per-instance route files from the dev traefik dynamic dir
#   orphans     kill runsc / containerd-shim process trees no longer tracked by
#               Docker (docker ps -aq)
#   (none)      run all of the above
#
# Usage: ./scripts/cleanup.sh [--verbose] [tests|volumes|instances|network|traefik]

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
DYNAMIC_DIR="$REPO_ROOT/docker/openworkspace_dev/traefik/dynamic"
TEST_DYNAMIC_DIR="$REPO_ROOT/apps/api/target/traefik-dynamic"

INSTANCE_NET_PATTERN='^ow-[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'

VERBOSE=0
SUBCOMMAND="all"
for arg in "$@"; do
    case "$arg" in
        --verbose|-v) VERBOSE=1 ;;
        tests|volumes|instances|network|traefik|orphans|all) SUBCOMMAND="$arg" ;;
        *) echo "ERROR: unknown argument: $arg" >&2; exit 2 ;;
    esac
done

log() {
    if [ "$VERBOSE" -eq 1 ]; then
        echo "$@"
    fi
}

# Remove containers whose own NetworkSettings reference an `ow-*` network
# matching $1 (extended regex), or whose name starts with ow_test / ow-vol-.
# Containers are inspected container-side because stopped containers hold a
# stale reference to removed networks and no longer appear in
# `docker network inspect .Containers`.
sweep_containers() {
    local net_regex="$1"
    local cid cname nets keys k targets=""
    while IFS= read -r cid; do
        [ -z "$cid" ] && continue
        cname=$(docker inspect "$cid" --format '{{.Name}}' 2>/dev/null || true)
        cname=${cname#/}
        if [[ "$cname" == ow_test* || "$cname" == ow-vol-* ]]; then
            targets="${targets} $cid"
            continue
        fi
        nets=$(docker inspect "$cid" --format '{{json .NetworkSettings.Networks}}' 2>/dev/null || true)
        keys=$(echo "$nets" | grep -oE '"ow-[a-z0-9-]*"' || true)
        for k in $keys; do
            k="${k//\"/}"
            if [ "$k" != "ow-network" ] && echo "$k" | grep -qE "$net_regex"; then
                targets="${targets} $cid"
                break
            fi
        done
    done < <(docker ps -aq 2>/dev/null || true)

    targets=$(echo -e "${targets}" | sed '/^$/d' | sort -u)
    if [ -n "$targets" ]; then
        log "==> 強制移除容器:"
        for c in $targets; do
            log "    - $c"
            docker rm -f "$c" &>/dev/null || true
        done
    else
        log "    沒有找到符合條件的容器"
    fi
}

# Kill runsc / containerd-shim process trees whose container id is no longer
# known to Docker (docker ps -aq --no-trunc). These accumulate when the API
# test suite force-removes containers whose sandbox is deadlocked mid-boot:
# dockerd drops the metadata, but the sandbox never exits, so the shim + gofer
# + sandbox get reparented to PID 1 as orphans that `docker ps` can't see.
#
# Two guards keep this from killing live containers:
#   1. `docker ps -aq --no-trunc` — the default (truncated 12-char) IDs never
#      match the full 64-hex cid on a shim/sandbox cmdline, so *every* shim
#      would be misclassified as an orphan and killed (killing a shim kills
#      its container with exit 137 and no docker event).
#   2. A cid tracked by Docker is never touched. Note: a leaked sandbox's
#      containerd task dir may linger after `docker rm -f` (the task delete
#      never completes for a created-state runsc container), so the task dir
#      must NOT be used to protect a process — "docker still tracks this cid"
#      is the only authoritative guard.
#
# The host shell collects the target PIDs (any user can read /proc cmdlines),
# then a privileged host-PID-namespace container performs the kill. The PID
# list is passed explicitly — never scan-and-kill by container id inside the
# container, because the host shell's own cmdline contains those ids.
sweep_orphans() {
    local active pids=() pid args cid
    log "==> [orphans] 清除 Docker 已遺忘的 runsc / containerd-shim 進程樹..."
    active=$(docker ps -aq --no-trunc 2>/dev/null || true)
    if [ -n "$active" ]; then
        active=$(echo "$active" | sort -u)
    fi

    while read -r pid args; do
        cid=""
        case "$args" in
            *containerd-shim*)
                cid=$(echo "$args" | sed -n 's/.* -id \([0-9a-f]\{64\}\).*/\1/p')
                ;;
            runsc-sandbox* | runsc-gofer*)
                cid=$(echo "$args" | grep -oE '[0-9a-f]{64}$' | head -n1)
                ;;
        esac
        [ -n "$cid" ] || continue
        if [ -z "$active" ] || ! echo "$active" | grep -qx "$cid"; then
            pids+=("$pid")
            log "    orphan pid=$pid cid=$cid"
        fi
    done < <(ps -eo pid,args --no-headers 2>/dev/null \
        | grep -E 'containerd-shim|runsc-(sandbox|gofer)' || true)

    if [ ${#pids[@]} -eq 0 ]; then
        log "    沒有孤兒進程。"
        return 0
    fi

    local targets
    targets=$(printf '%s\n' "${pids[@]}" | sort -u | tr '\n' ' ')
    log "    透過 privileged host-PID 容器 kill: $targets"
    docker run --rm --privileged --pid=host busybox:1 sh -c "kill -9 $targets 2>/dev/null" >/dev/null 2>&1 || true
    log "    已 kill ${#pids[@]} 個孤兒進程。"
}

# Remove all networks matching $1 (extended regex), optionally excluding
# names matching $2. Containers attached to them are force-removed first.
remove_networks() {
    local pattern="$1"
    local exclude="${2:-}"
    local net
    if [ -n "$exclude" ]; then
        mapfile -t NETS < <(docker network ls --format '{{.Name}}' | grep -E "$pattern" | grep -v -E "$exclude" || true)
    else
        mapfile -t NETS < <(docker network ls --format '{{.Name}}' | grep -E "$pattern" || true)
    fi

    if [ ${#NETS[@]} -eq 0 ]; then
        log "    沒有找到符合條件的網路，無須清理。"
        return 0
    fi

    for net in "${NETS[@]}"; do
        local attached
        attached=$(docker network inspect "$net" --format '{{range $id, $v := .Containers}}{{$id}} {{end}}' 2>/dev/null || true)
        if [ -n "$attached" ]; then
            log "    強制移除網路 '$net' 上的容器: $attached"
            docker rm -f $attached &>/dev/null || true
        fi
        if docker network rm "$net" >/dev/null 2>&1; then
            log "  - 已移除網路: $net"
        else
            echo "> 錯誤：無法移除網路 '$net'，請檢查是否有其他資源鎖定該網路。" >&2
        fi
    done
}

# Remove per-instance route files from a traefik dynamic dir, keeping the
# static config files. Defaults to the dev dynamic dir; the test suite uses
# its own dedicated dir (see cleanup_tests).
cleanup_traefik() {
    local dir="${1:-$DYNAMIC_DIR}"
    local keep=(.gitignore static-routers.yml static-services.yml static-transports.yml)
    local name skip removed=0
    [ -d "$dir" ] || { log "    動態路由目錄不存在: $dir"; return 0; }

    for f in "$dir"/*; do
        [ -f "$f" ] || continue
        name="$(basename "$f")"
        skip=false
        for k in "${keep[@]}"; do
            if [ "$name" = "$k" ]; then
                skip=true
                break
            fi
        done
        if $skip; then
            log "  keep  $name"
        else
            rm -f "$f"
            log "  rm    $name"
            ((++removed))
        fi
    done
    log "  -> 已移除 $removed 個路由檔案"
}

# Remove Docker named volumes created for persistent storage (`ow-persist-*`).
# These local-bind named volumes are never removed by the API's lifecycle
# paths — only the explicit cleanup endpoint and this script remove them.
cleanup_volumes() {
    local vol
    log "==> [volumes] 清理 persistent-storage 命名卷 (ow-persist-*)..."
    mapfile -t VOLS < <(docker volume ls -q --filter name='^ow-persist-' 2>/dev/null || true)
    if [ ${#VOLS[@]} -eq 0 ]; then
        log "    沒有符合條件的卷。"
        return 0
    fi
    for vol in "${VOLS[@]}"; do
        if docker volume rm "$vol" >/dev/null 2>&1; then
            log "  - 已移除卷: $vol"
        else
            echo "> 錯誤：無法移除卷 '$vol'。" >&2
        fi
    done
}

cleanup_tests() {
    log "==> [tests] 清理測試容器..."
    sweep_containers '^ow-'
    log "==> [tests] 清理測試網路..."
    remove_networks '^ow-' '^ow-network$'
    log "==> [tests] 清理測試卷..."
    cleanup_volumes
    log "==> [tests] 清理測試 traefik 配置 (測試目錄)..."
    cleanup_traefik "$TEST_DYNAMIC_DIR"
    log "==> [tests] 清理孤兒 runsc / containerd-shim 進程..."
    sweep_orphans
}

cleanup_instances() {
    log "==> [instances] 清理 instance 容器..."
    sweep_containers "$INSTANCE_NET_PATTERN"
    log "==> [instances] 清理 instance 網路..."
    remove_networks "$INSTANCE_NET_PATTERN"
}

cleanup_network() {
    local name="ow-network"
    log "==> [network] 清理共享控制網路 '$name'..."
    if ! docker network inspect "$name" >/dev/null 2>&1; then
        log "    網路 '$name' 不存在，無須清理。"
        return 0
    fi

    local attached
    attached=$(docker network inspect "$name" --format '{{range .Containers}}{{.Name}} {{end}}' 2>/dev/null || true)
    if [ -n "$attached" ]; then
        log "    中斷容器與 '$name' 的連線: $attached"
        for container in $attached; do
            docker network disconnect -f "$name" "$container" 2>/dev/null || true
        done
    fi

    if docker network rm "$name" >/dev/null 2>&1; then
        log "  - 已移除網路: $name"
    else
        echo "> 錯誤：無法移除網路 '$name'，請檢查是否有其他資源鎖定該網路。" >&2
    fi
}

case "$SUBCOMMAND" in
    tests) cleanup_tests ;;
    volumes) cleanup_volumes ;;
    instances) cleanup_instances ;;
    network) cleanup_network ;;
    traefik) cleanup_traefik ;;
    orphans) sweep_orphans ;;
    all)
        cleanup_instances
        cleanup_tests
        cleanup_network
        cleanup_traefik
        sweep_orphans
        ;;
esac

log "==> 完成"
