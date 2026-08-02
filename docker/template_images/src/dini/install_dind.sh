#!/usr/bin/env bash
set -euo pipefail

ARCH=$(arch | sed 's/aarch64/arm64/g' | sed 's/x86_64/amd64/g')
CODENAME=$(. /etc/os-release && echo "$VERSION_CODENAME")

# ca-certificates + gnupg are required before importing the Docker repo key
apt-get update
apt-get install -y ca-certificates gnupg

# Enable the Docker apt repo
curl -fsSL https://download.docker.com/linux/ubuntu/gpg | gpg --dearmor -o /usr/share/keyrings/docker-archive-keyring.gpg
echo "deb [arch=${ARCH} signed-by=/usr/share/keyrings/docker-archive-keyring.gpg] https://download.docker.com/linux/ubuntu ${CODENAME} stable" > /etc/apt/sources.list.d/docker.list

apt-get update
apt-get install -y \
    curl \
    gnupg \
    docker-ce \
    docker-ce-cli \
    containerd.io \
    docker-buildx-plugin \
    docker-compose-plugin \
    fuse-overlayfs \
    iptables \
    kmod \
    sudo \
    wget

# Container DNS resolution (no systemd-resolved inside)
echo 'hosts: files dns' > /etc/nsswitch.conf

# The in-instance dockerd nests inside the host's overlay2 storage, which would
# hit the overlay maximum-nesting-depth limit; fuse-overlayfs avoids it.
echo '{"storage-driver":"fuse-overlayfs","experimental":true}' > /etc/docker/daemon.json

if [ -z "${SKIP_CLEAN:-}" ]; then
    apt-get autoclean
    rm -rf /var/lib/apt/lists/* /tmp/*
fi
