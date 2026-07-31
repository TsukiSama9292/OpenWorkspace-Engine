#!/bin/bash
set -euo pipefail

BASE_DIR="./certs"
DAYS=36525

generate() {
  local dir="$BASE_DIR/$1"
  mkdir -p "$dir"
  openssl req -x509 -nodes -days "$DAYS" -newkey rsa:4096 \
    -keyout "$dir/key.pem" \
    -out "$dir/cert.pem" \
    -subj "/CN=$2" 2>/dev/null
  chmod 600 "$dir/key.pem"
}

generate "api"     "OpenWorkspace API"
generate "traefik" "OpenWorkspace Traefik"

echo "Self-signed certificates generated in $BASE_DIR/"
echo "  api/cert.pem, api/key.pem"
echo "  traefik/cert.pem, traefik/key.pem"
echo "  Expiry: $(date -d "+$DAYS days" '+%Y-%m-%d')"
