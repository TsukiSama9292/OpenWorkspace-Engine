#!/usr/bin/env bash
# KasmVNC DinI entrypoint: boot the in-instance dockerd as root (the
# Docker-in-gVisor recipe starts dockerd as root; no setuid/sudo involved),
# then drop to kasm-user and hand off to Kasm's stock startup chain. Without
# OW_DOCKER_IN_INSTANCE=true the bootstrap is a no-op and the container just
# runs as kasm-user exactly as today.
set -euo pipefail

# Rewrite /etc/resolv.conf from OW_DNS as root before anything starts (the
# nested dockerd inherits it). No-op when OW_DNS is unset.
/usr/local/bin/apply-ow-dns.sh

if [ "${OW_DOCKER_IN_INSTANCE:-}" = "true" ]; then
    /usr/local/bin/bootstrap-dockerd.sh
fi

exec setpriv --reuid=1000 --regid=1000 --init-groups -- env HOME=/home/kasm-user \
    /dockerstartup/kasm_default_profile.sh \
    /dockerstartup/vnc_startup.sh \
    /dockerstartup/kasm_startup.sh \
    "$@"
