#!/usr/bin/env bash
# Prove Docker-in-Instance (DinI) works end-to-end on this host, on both the
# runc and runsc runtimes, so a host upgrade or image change cannot silently
# break in-instance Docker. Same shape as apps/api/scripts/apply_bw_smoke.sh.
#
# For each runtime it provisions a throwaway instance exactly like the API does
# for a DinI-on container (default `bridge` network, --privileged, /var/lib/docker
# tmpfs, persistent home bind mount, OW_DOCKER_IN_INSTANCE=true), then verifies:
#   [1] the in-instance dockerd becomes ready via `docker info` within 15 s
#   [2] a nested --network=host service is reachable at localhost in-instance
#   [3] a nested container bind-mounting the persistent home writes through to
#       the host
#
# Usage:
#   ./scripts/dini_smoke_test.sh
#
# Environment overrides (all optional):
#   RUNTIMES            space-separated runtimes to test
#                                         default "runc runsc"
#   IMAGE               the *_dini template image to run
#                                         default tsukisama9292/ow-ttyd-ubuntu-dini:jammy
#   HOME_MOUNT          in-instance persistent-home path (the bind target)
#                                         default /home/ow_user/data
#   DOCKER_DAEMON_JSON  daemon.json path (for the runsc prereq check)
#                                         default /etc/docker/daemon.json
#
# Requirements: host docker access (no sudo needed), python3, and the
# busybox:1 image (for the nested checks).
set -euo pipefail

VERBOSE=0
for arg in "$@"; do
    case "$arg" in
        --verbose|-v) VERBOSE=1 ;;
    esac
done

RUNTIMES="${RUNTIMES:-runc runsc}"
IMAGE="${IMAGE:-tsukisama9292/ow-ttyd-ubuntu-dini:jammy}"
HOME_MOUNT="${HOME_MOUNT:-/home/ow_user/data}"
DOCKER_DAEMON_JSON="${DOCKER_DAEMON_JSON:-/etc/docker/daemon.json}"
PORT="${PORT:-18080}"

FAILED=0
CREATED=""
HOMES=""

log() {
    if [ "$VERBOSE" -eq 1 ]; then
        echo "==> $*"
    fi
}
fail() { echo "ERROR: $*" >&2; exit 1; }

cleanup() {
    for n in $CREATED; do
        docker rm -f "$n" >/dev/null 2>&1 || true
    done
    for h in $HOMES; do
        rm -rf "$h"
    done
}
trap cleanup EXIT

# ── Prerequisite checks ──
for bin in docker python3; do
    command -v "$bin" >/dev/null || fail "'$bin' is not installed"
done
docker image inspect "$IMAGE" >/dev/null 2>&1 \
    || fail "DinI image '$IMAGE' is missing (build with 'pnpm run build:template-images')"
docker image inspect busybox:1 >/dev/null 2>&1 \
    || fail "busybox:1 image is missing"

# runsc needs the Docker-in-gVisor runtimeArgs on the host; this is what the
# API relies on for the instance sandbox itself (see scripts/docker-runtime-gvisor.sh).
if [[ " $RUNTIMES " == *" runsc "* ]]; then
    MISSING_ARGS="$(python3 - "$DOCKER_DAEMON_JSON" <<'PY'
import json, sys
required = ["--net-raw", "--allow-packet-socket-write"]
try:
    with open(sys.argv[1], encoding="utf-8") as f:
        data = json.load(f)
except Exception:
    print(" ".join(required))
    sys.exit(0)
runsc = (data.get("runtimes") or {}).get("runsc") or {}
args = runsc.get("runtimeArgs") or []
print(" ".join(a for a in required if a not in args))
PY
)"
    [ -z "$MISSING_ARGS" ] \
        || fail "runsc is missing required runtimeArgs: $MISSING_ARGS — run 'sudo bash scripts/docker-runtime-gvisor.sh' first"
fi

# ── Per-runtime run ──
run_runtime() {
    local rt=$1 cid name home t marker
    home="$(mktemp -d /tmp/dini-smoke.XXXXXX)"
    HOMES="$HOMES $home"
    name="dini-smoke-${rt}-$$"
    marker="persist-${rt}-$$.txt"
    CREATED="$CREATED $name"

    log "Runtime: $rt  (image $IMAGE, home $home)"

    cid="$(docker run -d --name "$name" --runtime "$rt" --network bridge --privileged \
        --tmpfs /var/lib/docker:exec,mode=755 \
        -v "$home:$HOME_MOUNT" \
        -e OW_DOCKER_IN_INSTANCE=true \
        "$IMAGE")"

    # [1] dockerd ready within 15 s via `docker info`
    t=0
    for t in $(seq 1 15); do
        if docker exec "$cid" docker info >/dev/null 2>&1; then
            break
        fi
        sleep 1
    done
    if docker exec "$cid" docker info >/dev/null 2>&1; then
        log "    PASS: in-instance dockerd ready via 'docker info' (${t}s)"
    else
        echo "    FAIL: in-instance dockerd not ready within 15s"
        docker logs "$cid" >&2 || true
        FAILED=$((FAILED + 1))
        return
    fi

    # [2] nested --network=host service reachable at localhost in-instance
    if docker exec "$cid" docker run -d --network=host --name nested-smoke busybox:1 \
        sh -c "mkdir -p /tmp/www && echo dini-ok > /tmp/www/index.html && httpd -f -p $PORT -h /tmp/www" >/dev/null 2>&1; then
        t=0
        for t in $(seq 1 10); do
            if docker exec "$cid" curl -fsS "http://127.0.0.1:$PORT/" >/dev/null 2>&1; then
                break
            fi
            sleep 1
        done
        if docker exec "$cid" curl -fsS "http://127.0.0.1:$PORT/" >/dev/null 2>&1; then
            log "    PASS: nested --network=host service reachable at localhost:$PORT (${t}s)"
        else
            echo "    FAIL: nested --network=host service not reachable at localhost:$PORT"
            FAILED=$((FAILED + 1))
        fi
    else
        echo "    FAIL: could not start nested --network=host container"
        FAILED=$((FAILED + 1))
    fi

    # [3] nested bind-mount of the persistent home writes through to the host
    if docker exec "$cid" docker run --rm -v "$HOME_MOUNT:/hosttmp" busybox:1 \
        sh -c "echo persisted > /hosttmp/$marker" >/dev/null 2>&1; then
        if [ -f "$home/$marker" ]; then
            log "    PASS: nested bind-mount write-through reached host ($marker)"
        else
            echo "    FAIL: nested bind-mount write-through did not reach the host"
            FAILED=$((FAILED + 1))
        fi
    else
        echo "    FAIL: nested bind-mount container could not write"
        FAILED=$((FAILED + 1))
    fi
}

for rt in $RUNTIMES; do
    run_runtime "$rt"
done

echo "Result: $FAILED check(s) failed."
[ "$FAILED" -eq 0 ] || exit 1
echo "All DinI smoke checks passed on: $RUNTIMES"
