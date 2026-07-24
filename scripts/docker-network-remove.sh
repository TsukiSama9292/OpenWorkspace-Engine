#!/bin/bash

NETWORK_NAME="ow-network"

echo "> 正在檢查 Docker 網路 '$NETWORK_NAME'..."

# 1. 檢查網路是否存在
if ! docker network inspect "$NETWORK_NAME" >/dev/null 2>&1; then
    echo "> 訊息：網路 '$NETWORK_NAME' 不存在，無須清理。"
    exit 0
fi

# 2. 檢查是否有容器正連接在此網路上
ATTACHED_CONTAINERS=$(docker network inspect "$NETWORK_NAME" --format '{{range .Containers}}{{.Name}} {{end}}')

if [ -n "$ATTACHED_CONTAINERS" ]; then
    echo "> 警告：以下容器仍在使用網路 '$NETWORK_NAME'："
    echo "  -> $ATTACHED_CONTAINERS"
    
    # 自動中斷所有容器與該網路的連線
    echo "> 正在強制中斷容器與網路的連線..."
    for container in $ATTACHED_CONTAINERS; do
        docker network disconnect -f "$NETWORK_NAME" "$container" 2>/dev/null
        echo "  - 已中斷: $container"
    done
fi

# 3. 執行移除網路
echo "> 正在移除 Docker 網路 '$NETWORK_NAME'..."
if docker network rm "$NETWORK_NAME" >/dev/null 2>&1; then
    echo "> 成功：網路 '$NETWORK_NAME' 已順利移除！"
else
    echo "> 錯誤：無法移除網路 '$NETWORK_NAME'，請檢查是否有其他資源鎖定該網路。" >&2
    exit 1
fi