#!/usr/bin/env bash
# Shared root wrapper for the plain ttyd / jupyterlab images: rewrite
# /etc/resolv.conf from OW_DNS as root (the rewrite needs root; the app
# entrypoint runs as ow_user), then drop to ow_user and exec the app
# entrypoint. Mirrors the *_dini entrypoints minus the dockerd bootstrap.
# With OW_DNS unset the rewrite is a no-op and the container runs exactly
# as ow_user as before.
set -euo pipefail

/usr/local/bin/apply-ow-dns.sh

exec setpriv --reuid=1000 --regid=1000 --init-groups -- env HOME=/home/ow_user \
    /home/ow_user/entrypoint.sh
