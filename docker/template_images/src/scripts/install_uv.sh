#!/usr/bin/env bash
set -ex

curl -LsSf https://astral.sh/uv/install.sh | env UV_INSTALL_DIR="/home/kasm-user/.local/bin" INSTALLER_NO_MODIFY_PATH=1 sh