#!/usr/bin/env bash
# Start a short-lived postgres container for integration tests.
#
# Usage (in a test runner script):
#   source scripts/create_test_pg.sh
#   create_test_pg          # starts container, sets PG_HOST / PG_PORT
#   ... run tests ...
#   destroy_test_pg         # stops and removes the container

CONTAINER_NAME="ow-test-pg-$$"

create_test_pg() {
    CONTAINER_ID=$(docker run -d \
        --name "$CONTAINER_NAME" \
        -e POSTGRES_HOST_AUTH_METHOD=trust \
        -p 0:5432 \
        postgres:18-alpine)

    local raw_port
    raw_port=$(docker port "$CONTAINER_NAME" 5432/tcp | head -1 | cut -d: -f2)
    if [ -z "$raw_port" ]; then
        echo "ERROR: could not read port mapping" >&2
        return 1
    fi

    export PG_HOST="127.0.0.1"
    export PG_PORT="$raw_port"

    # Wait for Postgres to accept connections (up to 15s)
    for i in $(seq 1 30); do
        if (echo > /dev/tcp/$PG_HOST/$PG_PORT) 2>/dev/null; then
            return 0
        fi
        sleep 0.5
    done
    echo "ERROR: postgres did not become ready within 15s" >&2
    return 1
}

destroy_test_pg() {
    docker rm -fv "$CONTAINER_NAME" &>/dev/null || true
}
