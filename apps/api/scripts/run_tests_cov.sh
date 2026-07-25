#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
source "$SCRIPT_DIR/create_test_pg.sh"
source "$SCRIPT_DIR/remove_test_instance.sh"

cleanup() {
    echo "==> 清理 Postgres 測試容器..."
    destroy_test_pg
    echo "==> 清理測試環境..."
    remove_test_instance
}
trap cleanup EXIT

echo "==> 啟動 Postgres 測試容器..."
create_test_pg
echo "==> Postgres 就緒 (PG_HOST=$PG_HOST PG_PORT=$PG_PORT)"

echo "==> 執行測試與覆蓋率收集 (cargo llvm-cov nextest)..."
cargo llvm-cov nextest --features docker --ignore-filename-regex "main\.rs"
