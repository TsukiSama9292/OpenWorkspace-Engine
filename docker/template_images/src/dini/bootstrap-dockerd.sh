#!/usr/bin/env bash
# Shared in-instance Docker bootstrap for the *_dini images.
#
# Contract (see .scratch/docker-in-instance):
#  - No-op unless OW_DOCKER_IN_INSTANCE=true.
#  - Starts a root dockerd on the /var/lib/docker tmpfs (provisioned by the
#    API as a tmpfs) with --iptables=false --ip6tables=false.
#  - Polls readiness via `docker info` for up to 15 seconds; on failure logs
#    the dockerd log and exits non-zero.
set -euo pipefail

if [ "${OW_DOCKER_IN_INSTANCE:-}" != "true" ]; then
    echo "OW_DOCKER_IN_INSTANCE != true; skipping in-instance dockerd."
    exit 0
fi

echo "OW_DOCKER_IN_INSTANCE=true; starting in-instance dockerd..."

# Storage driver depends on the sandbox runtime:
#  - runc: fuse-overlayfs avoids the overlay maximum-nesting-depth limit (the
#    host itself runs overlay2 on the instance's /var/lib/docker tmpfs).
#  - runsc (gVisor): FUSE is broken inside the sandbox, but overlay2 works on
#    the /var/lib/docker tmpfs (the gVisor Docker-in-gVisor recipe).
# gVisor is detected via /proc/version (its kernel string names gVisor).
if grep -qi gvisor /proc/version 2>/dev/null; then
    storage_driver="overlay2"
else
    storage_driver="fuse-overlayfs"
fi
printf '{"storage-driver":"%s","experimental":true}\n' "$storage_driver" > /etc/docker/daemon.json

if ! pgrep -x dockerd >/dev/null 2>&1; then
    setsid dockerd \
        --iptables=false \
        --ip6tables=false \
        --data-root=/var/lib/docker \
        >/var/log/dockerd.log 2>&1 < /dev/null &
else
    echo "dockerd already running; checking readiness."
fi

ready=0
for _ in $(seq 1 15); do
    if docker info >/dev/null 2>&1; then
        ready=1
        break
    fi
    sleep 1
done

if [ "$ready" -ne 1 ]; then
    echo "in-instance dockerd failed to become ready within 15s" >&2
    if [ -f /var/log/dockerd.log ]; then
        cat /var/log/dockerd.log >&2 || true
    fi
    exit 1
fi

echo "in-instance dockerd is ready."
