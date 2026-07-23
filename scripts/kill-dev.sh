#!/usr/bin/env bash
set -euo pipefail

echo "Killing old dev processes..."

# Kill openworkspace-api (port 3000)
pkill -f "target/debug/openworkspace-api" 2>/dev/null && echo "  Killed openworkspace-api" || true

# Kill vite dev servers
pkill -f "vite.js dev" 2>/dev/null && echo "  Killed vite" || true

# Wait briefly for ports to release
sleep 1

echo "Done."
