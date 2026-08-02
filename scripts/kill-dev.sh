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

log "Killing old dev processes..."

# Kill openworkspace-api (port 3000)
pkill -f "target/debug/openworkspace-api" 2>/dev/null && log "  Killed openworkspace-api" || true

# Kill vite dev servers
pkill -f "vite.js dev" 2>/dev/null && log "  Killed vite" || true

# Wait briefly for ports to release
sleep 1

log "Done."
