#!/usr/bin/env bash
# Verify that tc/HTB per-container bandwidth limiting actually works on this host.
#
# It creates a throwaway Docker network with two busybox containers, measures a
# baseline, applies HTB limits (upload on the client's eth0, download on the
# host-side veth — the same mechanism the API uses), then re-measures and
# reports whether throughput converged toward the configured rates.
#
# Usage:
#   sudo ./apply_bw_smoke.sh [UP_MBPS] [DOWN_MBPS] [SECS]
#
#   UP_MBPS    upload cap to test (container egress)     default 10
#   DOWN_MBPS  download cap to test (host veth egress)   default 20
#   SECS       measurement duration at cap (smaller = less space/time)
#                                                       default 8
#
# Requirements: root (or passwordless sudo), docker, tc+ip (iproute2),
# nsenter (util-linux), python3, and the busybox:1 image.
set -euo pipefail

UP=${1:-10}
DOWN=${2:-20}
SECS=${3:-8}
NET="ow-bw-smoke"
CLIENT="bw-smoke-client"
SERVER="bw-smoke-server"
DATA_DIR="$(mktemp -d /tmp/bw-smoke.XXXXXX)"
HTTP_PID=""

log() { echo "==> $*"; }
fail() { echo "ERROR: $*" >&2; exit 1; }

cleanup() {
    docker rm -f "$CLIENT" "$SERVER" >/dev/null 2>&1 || true
    docker network rm "$NET" >/dev/null 2>&1 || true
    [ -n "$HTTP_PID" ] && kill "$HTTP_PID" >/dev/null 2>&1 || true
    pkill -f "http.server 8080" >/dev/null 2>&1 || true
    pkill -f "upload_server.py" >/dev/null 2>&1 || true
    rm -rf "$DATA_DIR"
}
trap cleanup EXIT

# ── Prerequisite checks ──
[ "$UP" -ge 1 ] || fail "UP_MBPS must be >= 1"
[ "$DOWN" -ge 1 ] || fail "DOWN_MBPS must be >= 1"
[ "$SECS" -ge 1 ] || fail "SECS must be >= 1"
for bin in tc ip nsenter docker python3; do
    command -v "$bin" >/dev/null || fail "'$bin' is not installed (tc/ip come from iproute2, nsenter from util-linux)"
done
docker image inspect busybox:1 >/dev/null 2>&1 || fail "busybox:1 image is missing"

# Kernel must accept an HTB qdisc (self-test on loopback).
tc qdisc add dev lo root handle 1: htb default 10 2>/dev/null \
    || fail "cannot create HTB qdisc on lo — kernel support or NET_ADMIN missing"
tc qdisc del dev lo root

log "Testing upload=${UP} Mbps / download=${DOWN} Mbps"

docker network create "$NET" >/dev/null
docker run -d --name "$SERVER" --network "$NET" busybox:1 sleep 3600 >/dev/null
docker run -d --name "$CLIENT" --network "$NET" busybox:1 sleep 3600 >/dev/null

GATEWAY="$(docker network inspect "$NET" -f '{{(index .IPAM.Config 0).Gateway}}')"
CLIENT_PID="$(docker inspect -f '{{.State.Pid}}' "$CLIENT")"

CLIENT_IFIDX="$(nsenter -t "$CLIENT_PID" -n ip -o link show eth0 | sed -nE 's/^[0-9]+: eth0@if([0-9]+):.*/\1/p')"
# From inside the container netns, `eth0@ifN` names the peer veth's ifindex in
# the host netns (unique). Match that against the host veths' own ifindex.
VETH="$(ip -o link show type veth | sed -nE "s/^${CLIENT_IFIDX}: ([^@]+)@.*/\1/p" | head -1)"
[ -n "$VETH" ] || fail "could not find host-side veth for eth0 peer ifindex ${CLIENT_IFIDX}"
log "client pid=$CLIENT_PID  eth0 peer (host) ifindex=$CLIENT_IFIDX  host veth=$VETH  gateway=$GATEWAY"

BYTES_PER_MBPS_PER_SEC=131072                # 1 MB per Mbps per 8 s
DOWN_BYTES=$(( DOWN * SECS * BYTES_PER_MBPS_PER_SEC ))
UP_BYTES=$(( UP * SECS * BYTES_PER_MBPS_PER_SEC ))
dd if=/dev/zero of="$DATA_DIR/bw-smoke.bin" bs=1024 count=$(( DOWN_BYTES / 1024 )) 2>/dev/null

cat > "$DATA_DIR/upload_server.py" <<'PYEOF'
import socket
s = socket.socket()
s.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
s.bind(("0.0.0.0", 9001))
s.listen(1)
c, _ = s.accept()
while c.recv(65536):
    pass
PYEOF

mbps_from() { # mbps_from <bytes> <secs>
    awk "BEGIN { printf \"%.2f\", ($1 * 8) / ($2 * 1000000) }"
}

# ── Measurements ──
measure_download() { # <label>
    local label=$1 start end secs measured
    python3 -m http.server 8080 --bind 0.0.0.0 --directory "$DATA_DIR" >/dev/null 2>&1 &
    HTTP_PID=$!
    sleep 1
    start=$(date +%s%N)
    docker exec "$CLIENT" wget -q -O /dev/null "http://${GATEWAY}:8080/bw-smoke.bin"
    end=$(date +%s%N)
    kill "$HTTP_PID" >/dev/null 2>&1 || true
    HTTP_PID=""
    secs=$(awk "BEGIN { printf \"%.2f\", ($end - $start) / 1000000000 }")
    measured=$(mbps_from "$DOWN_BYTES" "$secs")
    printf "    %-22s download: %6.2f Mbps  (%d bytes in %.2fs)\n" "$label" "$measured" "$DOWN_BYTES" "$secs"
}

measure_upload() { # <label>
    local label=$1 start end secs measured bytes=$UP_BYTES
    python3 "$DATA_DIR/upload_server.py" &
    local py_pid=$!
    sleep 1
    start=$(date +%s%N)
    docker exec "$CLIENT" sh -c "dd if=/dev/zero bs=1024 count=$(( bytes / 1024 )) 2>/dev/null | nc -w 5 ${GATEWAY} 9001"
    end=$(date +%s%N)
    kill "$py_pid" >/dev/null 2>&1 || true
    wait "$py_pid" >/dev/null 2>&1 || true
    secs=$(awk "BEGIN { printf \"%.2f\", ($end - $start) / 1000000000 }")
    measured=$(mbps_from "$bytes" "$secs")
    printf "    %-22s upload:   %6.2f Mbps  (%d bytes in %.2fs)\n" "$label" "$measured" "$bytes" "$secs"
}

log "Baseline (unlimited):"
measure_download "baseline"
measure_upload "baseline"

log "Applying limits (HTB):"
nsenter -t "$CLIENT_PID" -n tc qdisc add dev eth0 root handle 1: htb default 10
nsenter -t "$CLIENT_PID" -n tc class add dev eth0 parent 1: classid 1:10 htb rate "${UP}mbit"
tc qdisc add dev "$VETH" root handle 1: htb default 10
tc class add dev "$VETH" parent 1: classid 1:10 htb rate "${DOWN}mbit"

log "Capped:"
measure_download "capped download"
measure_upload "capped upload"

log "Result: HTB limits applied on $CLIENT (upload ${UP} Mbps) and $VETH (download ${DOWN} Mbps)."
log "Compare the capped numbers against the baseline and the configured rates above."
