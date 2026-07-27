#!/usr/bin/env bash
set -euo pipefail

DYNAMIC_DIR="$(cd "$(dirname "$0")/../docker/openworkspace_dev/traefik/dynamic" && pwd)"

KEEP=(
  .gitignore
  static-routers.yml
  static-services.yml
  static-transports.yml
)

removed=0
for f in "$DYNAMIC_DIR"/*; do
  [ -f "$f" ] || continue
  name="$(basename "$f")"
  skip=false
  for k in "${KEEP[@]}"; do
    if [ "$name" = "$k" ]; then
      skip=true
      break
    fi
  done
  if $skip; then
    echo "  keep  $name"
  else
    rm -f "$f"
    echo "  rm    $name"
    ((removed++))
  fi
done

echo "Done — removed $removed file(s) from $DYNAMIC_DIR"
