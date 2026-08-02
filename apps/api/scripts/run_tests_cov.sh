#!/usr/bin/env bash
set -euo pipefail

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

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"
source "$SCRIPT_DIR/create_test_pg.sh"

# Route files written by the API under test must never land in the dev
# traefik dynamic dir — give the suite its own dedicated dir.
TEST_DYNAMIC_DIR="$REPO_ROOT/apps/api/target/traefik-dynamic"
export TRAEFIK_DYNAMIC_DIR="$TEST_DYNAMIC_DIR"
mkdir -p "$TEST_DYNAMIC_DIR"

cleanup() {
    log "==> 清理 Postgres 測試容器..."
    destroy_test_pg
    log "==> 清理測試環境..."
    if [ "$VERBOSE" -eq 1 ]; then
        "$REPO_ROOT/scripts/cleanup.sh" --verbose tests
    else
        "$REPO_ROOT/scripts/cleanup.sh" tests
    fi
}
trap cleanup EXIT

log "==> 啟動 Postgres 測試容器..."
create_test_pg
log "==> Postgres 就緒 (PG_HOST=$PG_HOST PG_PORT=$PG_PORT)"

log "==> 執行測試與覆蓋率收集 (cargo llvm-cov nextest)..."
cargo llvm-cov nextest --features docker --ignore-filename-regex "main\.rs" --json --output-path coverage.json
