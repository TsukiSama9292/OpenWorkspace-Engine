#!/usr/bin/env bash
# KasmVNC entrypoint: rewrite /etc/resolv.conf from OW_DNS as root (the
# rewrite needs root; /etc/resolv.conf is not writable by kasm-user), then
# drop to kasm-user and hand off to Kasm's stock startup chain. Mirrors
# kasm-dini-entrypoint.sh minus the in-instance dockerd bootstrap. With
# OW_DNS unset the rewrite is a no-op and the container runs exactly as
# Kasm's stock startup does.
set -euo pipefail

/usr/local/bin/apply-ow-dns.sh

exec setpriv --reuid=1000 --regid=1000 --init-groups -- env HOME=/home/kasm-user \
    /dockerstartup/kasm_default_profile.sh \
    /dockerstartup/vnc_startup.sh \
    /dockerstartup/kasm_startup.sh \
    "$@"
