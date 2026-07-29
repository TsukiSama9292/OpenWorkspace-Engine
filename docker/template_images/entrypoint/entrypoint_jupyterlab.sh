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
echo " Step 2: Configure Password and Start Jupyter Lab (with SSL)"
echo "=========================================="

# Read JUPYTER_PASSWORD environment variable, default to 'password' if not set
JUPYTER_PASS="${JUPYTER_PASSWORD:-password}"

# Calculate Jupyter-compatible password hash using python
echo "Calculating password hash..."
HASHED_PASS=$(uvx --from jupyterlab python -c "from jupyter_server.auth import passwd; print(passwd('$JUPYTER_PASS'))")

echo "Starting Jupyter Lab..."
echo "Default password (if JUPYTER_PASSWORD is not set): password"
echo "You can customize your password before running the script:"
echo "  export JUPYTER_PASSWORD='your_secure_password'"
echo ""

# Start Jupyter Lab using uvx with SSL certificates and password settings
uvx --from jupyterlab jupyter lab \
    --ServerApp.certfile="$CERT_DIR/jupyter.crt" \
    --ServerApp.keyfile="$CERT_DIR/jupyter.key" \
    --ServerApp.password="$HASHED_PASS" \
    --ip=0.0.0.0 \
    --port=8888 \
    --no-browser