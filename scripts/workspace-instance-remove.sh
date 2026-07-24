#!/bin/bash

NETWORK_NAME="ow-network"

echo "> 正在檢查 Docker 網路 '$NETWORK_NAME'..."

# 1. 檢查網路是否存在
if ! docker network inspect "$NETWORK_NAME" >/dev/null 2>&1; then
    echo "> 訊息：網路 '$NETWORK_NAME' 不存在，無須清理 workspace instance。"
    exit 0
fi

# 2. 取得所有連接在此網路上的容器 ID
CONTAINER_IDS=$(docker network inspect "$NETWORK_NAME" --format '{{range .Containers}}{{.Name}} {{end}}')

if [ -n "$CONTAINER_IDS" ]; then
    echo "> 發現以下容器正在使用網路 '$NETWORK_NAME'："
    for cid in $CONTAINER_IDS; do
        echo "  - $cid"
    done

    # 3. 停止並移除這些容器
    echo "> 正在停止並移除上述容器..."
    # 強制停止並刪除容器 (-f 表示 force，即使運行中也會直接 kill 並 remove)
    docker rm -f $CONTAINER_IDS >/dev/null 2>&1

    if [ $? -eq 0 ]; then
        echo "> 所有相關容器已順利移除！"
    else
        echo "> 警告：部分容器移除時遇到問題，嘗試繼續處理..." >&2
    fi
else
    echo "> 沒有發現任何容器連接至網路 '$NETWORK_NAME'。"
fi