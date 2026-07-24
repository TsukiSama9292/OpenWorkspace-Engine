#!/usr/bin/env bash
# Remove all containers connected to the "ow-test" network OR named with "ow_test" prefix, then remove the network.
# Usage: ./scripts/remove_test_instance.sh

remove_test_instance() {
    NETWORK="ow-test"
    PREFIX="ow_test"

    echo "==> 查找 '${NETWORK}' 網路下或名稱以 '${PREFIX}' 開頭的所有容器..."

    # 1. 抓出連到該網路的所有容器（不論是否關閉）
    NET_CONTAINERS=$(docker ps -a --filter "network=${NETWORK}" --format '{{.Names}}' 2>/dev/null || true)

    # 2. 抓出名稱以 PREFIX 開頭的所有容器（不論是否關閉）
    NAME_CONTAINERS=$(docker ps -a --filter "name=^${PREFIX}" --format '{{.Names}}' 2>/dev/null || true)

    # 3. 合併清單並去重 (sort -u)
    ALL_TARGETS=$(echo -e "${NET_CONTAINERS}\n${NAME_CONTAINERS}" | sed '/^$/d' | sort -u)

    if [ -n "$ALL_TARGETS" ]; then
        for c in $ALL_TARGETS; do
            echo "    強制移除容器: $c"
            docker rm -f "$c" &>/dev/null || true
        done
    else
        echo "    沒有找到符合條件的容器"
    fi

    echo "==> 移除網路 ${NETWORK}..."
    docker network rm "$NETWORK" &>/dev/null || true

    echo "==> 完成"
}

remove_test_instance