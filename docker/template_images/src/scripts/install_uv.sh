#!/usr/bin/env bash
set -ex

export UV_INSTALL_DIR="/home/kasm-user/.local/bin"

mkdir -p "$UV_INSTALL_DIR"

curl -LsSf https://astral.sh/uv/install.sh \
  | env UV_INSTALL_DIR="$UV_INSTALL_DIR" \
       INSTALLER_NO_MODIFY_PATH=1 sh

chown -R 1000:1000 /home/kasm-user/.local

cat > /etc/profile.d/uv.sh <<'EOF'
export PATH="$HOME/.local/bin:$PATH"
EOF

tee -a /etc/bash.bashrc >/dev/null <<'EOF'
[ -s "/etc/profile.d/uv.sh" ] && . "/etc/profile.d/uv.sh"
EOF