#!/usr/bin/env bash
# Builds all template images (regular + *_dini) in dependency order.
# The *_dini variants build FROM the regular images, so order matters.
# Docker layer caching keeps rebuilds fast when nothing changed.
set -euo pipefail

VERBOSE=0
for arg in "$@"; do
    case "$arg" in
        --verbose|-v) VERBOSE=1 ;;
    esac
done

log() {
    if [ "$VERBOSE" -eq 1 ]; then
        echo "$@"
    fi
}

cd "$(dirname "$0")"

KASMVNC_BASE_TAG="${KASMVNC_BASE_TAG:-1.19.0-rolling-daily}"

log "> Building tsukisama9292/ow-jupyter-ubuntu:jammy"
docker build -t tsukisama9292/ow-jupyter-ubuntu:jammy -f Dockerfile.jupyterlab_ubuntu .

log "> Building tsukisama9292/ow-ttyd-ubuntu:jammy"
docker build -t tsukisama9292/ow-ttyd-ubuntu:jammy -f Dockerfile.ttyd_ubuntu .

log "> Building tsukisama9292/ow-kasmvnc-ubuntu:jammy"
docker build -t tsukisama9292/ow-kasmvnc-ubuntu:jammy \
    -f Dockerfile.kasmvnc_ubuntu \
    --build-arg BASE_TAG="${KASMVNC_BASE_TAG}" .

log "> Building tsukisama9292/ow-jupyter-ubuntu-dini:jammy"
docker build -t tsukisama9292/ow-jupyter-ubuntu-dini:jammy -f Dockerfile.jupyterlab_ubuntu_dini .

log "> Building tsukisama9292/ow-ttyd-ubuntu-dini:jammy"
docker build -t tsukisama9292/ow-ttyd-ubuntu-dini:jammy -f Dockerfile.ttyd_ubuntu_dini .

log "> Building tsukisama9292/ow-kasmvnc-ubuntu-dini:jammy"
docker build -t tsukisama9292/ow-kasmvnc-ubuntu-dini:jammy -f Dockerfile.kasmvnc_ubuntu_dini .

log "> All template images built."
