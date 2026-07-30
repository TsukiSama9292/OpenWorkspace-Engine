#!/usr/bin/env bash
set -ex

export NVM_DIR="/home/kasm-user/.nvm"

mkdir -p "$NVM_DIR"

env NVM_DIR="$NVM_DIR" INSTALLER_NO_MODIFY_PATH=1 bash -c \
  "curl -o- https://raw.githubusercontent.com/nvm-sh/nvm/v0.40.3/install.sh | bash"

. "$NVM_DIR/nvm.sh"

nvm install 24
nvm alias default 24

chown -R 1000:1000 /home/kasm-user/.nvm

tee /etc/profile.d/nvm.sh >/dev/null <<EOF
export NVM_DIR=/home/kasm-user/.nvm
[ -s "\$NVM_DIR/nvm.sh" ] && \. "\$NVM_DIR/nvm.sh"
EOF

tee -a /etc/bash.bashrc >/dev/null <<EOF
[ -s "/etc/profile.d/nvm.sh" ] && \. "/etc/profile.d/nvm.sh"
EOF
