#!/usr/bin/env bash
# Exit immediately if a command exits with a non-zero status
set -e

echo "=========================================="
echo " Step 1: Generate SSL Certificate for ttyd"
echo "=========================================="
CERT_DIR="$HOME/.ttyd/certs"
mkdir -p "$CERT_DIR"

if [ ! -f "$CERT_DIR/ttyd.crt" ] || [ ! -f "$CERT_DIR/ttyd.key" ]; then
    echo "Generating SSL RSA key and self-signed certificate..."
    openssl req -x509 -newkey rsa:4096 -days 365 -nodes \
      -keyout "$CERT_DIR/ttyd.key" \
      -out "$CERT_DIR/ttyd.crt" \
      -subj "/C=TW/ST=Taiwan/L=Taipei/O=Development/CN=localhost"
    echo "SSL certificate successfully generated at: $CERT_DIR"
else
    echo "Existing SSL certificate detected, skipping generation."
fi

echo ""
echo "=========================================="
echo " Step 2: Configure Authentication & Start ttyd"
echo "=========================================="

# Read environment variables with defaults (using :- for proper fallback)
TTYD_USER="${TTYD_USERNAME:-ow_user}"
TTYD_PASS="${TTYD_PASSWORD:-password}"

echo "Starting ttyd..."
echo "Default credentials: ${TTYD_USER}:${TTYD_PASS}"
echo "You can customize them before running the script:"
echo "  export TTYD_USERNAME='your_username'"
echo "  export TTYD_PASSWORD='your_password'"
echo ""

# Start ttyd with SSL enabled
ttyd -S \
    -c "${TTYD_USER}:${TTYD_PASS}" \
    -p 7681 \
    -C "$CERT_DIR/ttyd.crt" \
    -K "$CERT_DIR/ttyd.key" \
    bash