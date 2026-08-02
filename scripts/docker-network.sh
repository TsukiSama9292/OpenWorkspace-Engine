#!/bin/bash

NETWORK_NAME="ow-network"

VERBOSE=0
for arg in "$@"; do
    case "$arg" in
        --verbose|-v) VERBOSE=1 ;;
    esac
done

log() {
    if [ "$VERBOSE" -eq 1 ]; then
        echo "$@"
    fi
}

# 檢查 Docker 網路是否已存在
if docker network inspect "$NETWORK_NAME" >/dev/null 2>&1; then
    log "> Docker 網路 '$NETWORK_NAME' 已經存在，略過建立步驟。"
else
    log "> 正在尋找可用的 /16 網段並建立 Docker 網路 '$NETWORK_NAME'..."

    # 動態尋找一個未被佔用的 172.x.0.0/16 網段 (x 從 16 到 31)
    SELECTED_SUBNET=""
    for i in {16..31}; do
        CANDIDATE="172.${i}.0.0/16"
        # 檢查該網段是否已經被現有的 Docker 網路使用
        if ! docker network ls --format '{{.Name}}' | xargs -I {} docker network inspect {} --format '{{range .IPAM.Config}}{{.Subnet}}{{"\n"}}{{end}}' 2>/dev/null | grep -q "^${CANDIDATE}"; then
            SELECTED_SUBNET="$CANDIDATE"
            break
        fi
    done

    # 如果沒有找到合適的，就退回讓 Docker 自動分配（但通常 172.16-31 很夠用）
    if [ -z "$SELECTED_SUBNET" ]; then
        log "> 警告：未能在 172.16-31 範圍找到未使用的 /16 網段，改由 Docker 自動指派..."
        docker network create \
            --driver bridge \
            --subnet 172.28.0.0/16 \
            "$NETWORK_NAME"
    else
        log "> 自動選擇未使用的網段: $SELECTED_SUBNET"
        docker network create \
            --driver bridge \
            --subnet "$SELECTED_SUBNET" \
            "$NETWORK_NAME"
    fi

    if [ $? -eq 0 ]; then
        log "> 網路 '$NETWORK_NAME' 建立成功！"
    else
        echo "> 錯誤：網路建立失敗。" >&2
        exit 1
    fi
fi
