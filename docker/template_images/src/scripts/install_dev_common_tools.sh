#!/usr/bin/env bash
set -ex

apt-get update && \
    apt-get upgrade -y && \
    apt install -y \
    gnome-keyring \
    htop \
    tree \
    p7zip-full \
    nmap \
    traceroute \
    tcptraceroute \
    curl \
    git \
    ca-certificates \
    wget \
    sudo \
    unzip \
    && rm -rf /var/lib/apt/lists/*