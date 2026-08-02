#!/usr/bin/env bash

# Continue execution even if a command fails, ensuring all checks run
set +e

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

log "=================================================="
log " [1/3] Running: Default cargo test --no-run check"
log "=================================================="
cargo test --no-run 2>&1 | grep -iE "warning|error"
log ""

log "=================================================="
log " [2/3] Running: cargo test with docker feature --no-run check"
log "=================================================="
cargo test --no-run --features docker 2>&1 | grep -iE "warning|error"
log ""

log "=================================================="
log " [3/3] Running: Library check (cargo check --lib)"
log "=================================================="
cargo check --lib 2>&1 | grep -iE "warning|error"
log ""

log "=================================================="
log " All checks completed!"
log "=================================================="
