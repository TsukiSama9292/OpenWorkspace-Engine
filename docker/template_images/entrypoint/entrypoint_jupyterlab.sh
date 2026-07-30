#!/usr/bin/env bash
# Exit immediately if a command exits with a non-zero status
set -e

# 將 uv 的安裝路徑加入 PATH 中，確保腳本能找到 uv 與 uvx
export PATH="$HOME/.local/bin:$PATH"

echo "=========================================="
echo " Step 1: Generate SSL Certificate"
echo "=========================================="
CERT_DIR="$HOME/.jupyter/certs"
mkdir -p "$CERT_DIR"

if [ ! -f "$CERT_DIR/jupyter.crt" ]; then
    openssl req -x509 -nodes -days 365 -newkey rsa:2048 \
      -keyout "$CERT_DIR/jupyter.key" \
      -out "$CERT_DIR/jupyter.crt" \
      -subj "/C=TW/ST=Taiwan/L=Taipei/O=Development/CN=localhost"
    echo "SSL certificate successfully generated at: $CERT_DIR"
else
    echo "Existing SSL certificate detected, skipping generation."
fi

echo ""
echo "=========================================="
echo " Step 2: Configure Token and Start Jupyter Lab (with SSL)"
echo "=========================================="

# Read JUPYTER_TOKEN environment variable, default to 'password' if not set
JUPYTER_TOKEN="${JUPYTER_TOKEN:-password}"

echo "Starting Jupyter Lab..."
echo "Default token (if JUPYTER_TOKEN is not set): password"
echo "You can customize your token before running the script:"
echo "  export JUPYTER_TOKEN='your_secure_token'"
echo ""

# Read JUPYTER_BASE_URL environment variable (optional)
JUPYTER_BASE_URL="${JUPYTER_BASE_URL:-}"

BASE_URL_ARG=""
if [ -n "$JUPYTER_BASE_URL" ]; then
    BASE_URL_ARG="--ServerApp.base_url=$JUPYTER_BASE_URL"
fi

# Start Jupyter Lab using uvx with SSL certificates and token auth
uvx --from jupyterlab jupyter lab \
    --ServerApp.certfile="$CERT_DIR/jupyter.crt" \
    --ServerApp.keyfile="$CERT_DIR/jupyter.key" \
    --IdentityProvider.token="$JUPYTER_TOKEN" \
    --ServerApp.password="" \
    --ip=0.0.0.0 \
    --port=8888 \
    --no-browser \
    $BASE_URL_ARG