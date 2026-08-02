#!/usr/bin/env bash
# Prove the per-instance /30 isolation + DNS topology end-to-end on this host,
# on both the runc and runsc runtimes, so a host upgrade or image change cannot
# silently break tenant isolation or the runsc DNS fix. Same shape as
# scripts/dini_smoke_test.sh / apps/api/scripts/apply_bw_smoke.sh.
#
# For each runtime it provisions two throwaway instances on two distinct /30
# bridges (the API's per-instance topology) plus one OW-image instance for the
# DNS rewrite, then verifies:
#   [1] each isolation instance holds its own unique /30 IP (network+2,
#       gateway network+1)
#   [2] each instance reaches its own listener (positive control) via its own
#       /30 gateway
#   [3] the two instances are mutually unreachable (cross-network TCP probe
#       fails both ways) while each still reaches the internet
#   [4] an OW instance with OW_DNS rewrites /etc/resolv.conf and resolves a
#       public hostname in-instance (the rewrite that fixes DNS under runsc)
#   [5] teardown removes every instance container and every bridge this script
#       created — the host accumulates nothing
#
# Usage:
#   ./scripts/network_isolation_smoke_test.sh
#
# Environment overrides (all optional):
#   RUNTIMES  space-separated runtimes to test       default "runc runsc"
#   DNS_IMAGE the OW image for the DNS rewrite check
#             default tsukisama9292/ow-ttyd-ubuntu:jammy
#   OW_DNS    comma-separated resolvers the API sets on every instance
#             default "8.8.8.8,1.1.1.1"
#   LISTEN_PORT  listener port for the isolation probes   default 5555
#
# Requirements: host docker access (no sudo needed), python3, the busybox:1
# image, and DNS_IMAGE present locally (rebuild with docker/template_images/build.sh).
set -euo pipefail

VERBOSE=0
for arg in "$@"; do
    case "$arg" in
        --verbose|-v) VERBOSE=1 ;;
    esac
done

RUNTIMES="${RUNTIMES:-runc runsc}"
DNS_IMAGE="${DNS_IMAGE:-tsukisama9292/ow-ttyd-ubuntu:jammy}"
OW_DNS="${OW_DNS:-8.8.8.8,1.1.1.1}"
LISTEN_PORT="${LISTEN_PORT:-5555}"
DOCKER_DAEMON_JSON="${DOCKER_DAEMON_JSON:-/etc/docker/daemon.json}"

FAILED=0
CREATED_NETS=""
CREATED_CTRS=""

log() {
    if [ "$VERBOSE" -eq 1 ]; then
        echo "==> $*"
    fi
}
fail() { echo "ERROR: $*" >&2; exit 1; }
note() { echo "    $*"; }

cleanup() {
    for c in $CREATED_CTRS; do
        docker rm -f "$c" >/dev/null 2>&1 || true
    done
    for n in $CREATED_NETS; do
        docker network rm "$n" >/dev/null 2>&1 || true
    done
}
trap cleanup EXIT

# ── Prerequisite checks ──
for bin in docker python3; do
    command -v "$bin" >/dev/null || fail "'$bin' is not installed"
done
docker image inspect busybox:1 >/dev/null 2>&1 \
    || fail "busybox:1 image is missing"
docker image inspect "$DNS_IMAGE" >/dev/null 2>&1 \
    || fail "DNS image '$DNS_IMAGE' is missing (build with docker/template_images/build.sh)"

# runsc needs the Docker-in-gVisor runtimeArgs on the host (see scripts/docker-runtime-gvisor.sh).
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

# A fresh /30 subnet in the 10.201.0.0/16 range — a base the API allocator
# (default 10.200.0.0/16) never uses, so the smoke cannot collide with live
# instance networks. Prints "subnet gateway instance" (network .0, .1, .2).
random_30() {
    python3 - <<'PY'
import secrets
third = (secrets.randbelow(256) % 64) * 4
a = secrets.randbelow(256)
print(f"10.201.{a}.{third}/30 10.201.{a}.{third + 1} 10.201.{a}.{third + 2}")
PY
}

# Run `cmd` inside a running container; prints the exit code.
exec_in() {
    local cid=$1; shift
    docker exec "$cid" "$@" >/dev/null 2>&1
}

# ── Per-runtime run ──
run_runtime() {
    local rt=$1 suffix net_a net_b net_dns ctr_a ctr_b ctr_dns
    local subnet gw ip
    local s_a g_a i_a s_b g_b i_b s_d g_d i_d

    suffix="$(python3 -c 'import secrets; print(secrets.token_hex(6))')"
    net_a="ow-smoke-${rt}-a-${suffix}"
    net_b="ow-smoke-${rt}-b-${suffix}"
    net_dns="ow-smoke-${rt}-dns-${suffix}"
    ctr_a="ow-smoke-${rt}-a-c-${suffix}"
    ctr_b="ow-smoke-${rt}-b-c-${suffix}"
    ctr_dns="ow-smoke-${rt}-dns-c-${suffix}"

    # Tolerate leftovers from a crashed earlier run.
    for n in "$net_a" "$net_b" "$net_dns"; do
        docker network inspect "$n" >/dev/null 2>&1 && docker network rm "$n" >/dev/null 2>&1 || true
    done

    log "Runtime: $rt"
    CREATED_NETS="$CREATED_NETS $net_a $net_b $net_dns"
    CREATED_CTRS="$CREATED_CTRS $ctr_a $ctr_b $ctr_dns"

    read -r s_a g_a i_a <<<"$(random_30)"
    read -r s_b g_b i_b <<<"$(random_30)"
    while [ "$s_a" = "$s_b" ]; do
        read -r s_b g_b i_b <<<"$(random_30)"
    done
    read -r s_d g_d i_d <<<"$(random_30)"
    while [ "$s_d" = "$s_a" ] || [ "$s_d" = "$s_b" ]; do
        read -r s_d g_d i_d <<<"$(random_30)"
    done

    docker network create --driver bridge --subnet "$s_a" --gateway "$g_a" "$net_a" >/dev/null \
        || fail "runtime $rt: failed to create network $net_a ($s_a)"
    docker network create --driver bridge --subnet "$s_b" --gateway "$g_b" "$net_b" >/dev/null \
        || fail "runtime $rt: failed to create network $net_b ($s_b)"
    docker network create --driver bridge --subnet "$s_d" --gateway "$g_d" "$net_dns" >/dev/null \
        || fail "runtime $rt: failed to create network $net_dns ($s_d)"

    # Each isolation instance runs a busybox nc listener as PID 1 so the
    # cross-network probe is symmetric: reaching the listener exits 0; an
    # isolated /30 drops the SYN and nc times out (exit 1).
    docker run -d --name "$ctr_a" --runtime "$rt" --network "$net_a" busybox:1 \
        sh -c "nc -l -p $LISTEN_PORT -s 0.0.0.0 & sleep 3600" >/dev/null \
        || fail "runtime $rt: could not start instance A"
    docker run -d --name "$ctr_b" --runtime "$rt" --network "$net_b" busybox:1 \
        sh -c "nc -l -p $LISTEN_PORT -s 0.0.0.0 & sleep 3600" >/dev/null \
        || fail "runtime $rt: could not start instance B"
    sleep 2

    # [1] each instance holds its own unique /30 IP (network+2)
    local ip_a ip_b
    ip_a="$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$ctr_a")"
    ip_b="$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' "$ctr_b")"
    if [ "$ip_a" = "$i_a" ] && [ "$ip_b" = "$i_b" ] && [ "$ip_a" != "$ip_b" ]; then
        log "    PASS [1]: instances hold unique /30 IPs ($ip_a, $ip_b)"
    else
        note "    FAIL [1]: expected IPs $i_a / $i_b, got $ip_a / $ip_b"
        FAILED=$((FAILED + 1))
    fi

    # [2] positive control + per-instance gateway: each reaches its own listener
    # through its own /30 gateway.
    if exec_in "$ctr_a" sh -c "nc -w 3 $i_a $LISTEN_PORT"; then
        log "    PASS [2]: instance A reaches its own listener"
    else
        note "    FAIL [2]: instance A cannot reach its own listener"
        FAILED=$((FAILED + 1))
    fi
    if exec_in "$ctr_b" sh -c "nc -w 3 $i_b $LISTEN_PORT"; then
        log "    PASS [2]: instance B reaches its own listener"
    else
        note "    FAIL [2]: instance B cannot reach its own listener"
        FAILED=$((FAILED + 1))
    fi
    local route_a route_b
    route_a="$(docker exec "$ctr_a" ip route 2>/dev/null || true)"
    route_b="$(docker exec "$ctr_b" ip route 2>/dev/null || true)"
    if [[ "$route_a" == *"default via $g_a"* ]] && [[ "$route_b" == *"default via $g_b"* ]]; then
        log "    PASS [2]: each instance routes via its own /30 gateway"
    else
        note "    FAIL [2]: default route not via own gateway (A: $route_a | B: $route_b)"
        FAILED=$((FAILED + 1))
    fi

    # [3] internet on each, then mutual isolation both ways. The internet probe
    # is a raw-IP TCP connect (nc to 1.1.1.1:443), deliberately DNS-independent:
    # these busybox instances have no OW_DNS resolv.conf rewrite, and Docker's
    # embedded resolver (127.0.0.11) does not bind under runsc, so a hostname
    # fetch would fail under runsc regardless of connectivity. Name resolution
    # is asserted separately in [4] against an OW instance that has the rewrite.
    if exec_in "$ctr_a" sh -c "nc -w 8 1.1.1.1 443" \
        && exec_in "$ctr_b" sh -c "nc -w 8 1.1.1.1 443"; then
        log "    PASS [3]: both instances reach the internet"
    else
        note "    FAIL [3]: an instance could not reach the internet"
        FAILED=$((FAILED + 1))
    fi
    if exec_in "$ctr_b" sh -c "nc -w 3 $i_a $LISTEN_PORT"; then
        note "    FAIL [3]: isolation broken — instance B reached instance A"
        FAILED=$((FAILED + 1))
    else
        log "    PASS [3]: instance B cannot reach instance A"
    fi
    if exec_in "$ctr_a" sh -c "nc -w 3 $i_b $LISTEN_PORT"; then
        note "    FAIL [3]: isolation broken — instance A reached instance B"
        FAILED=$((FAILED + 1))
    else
        log "    PASS [3]: instance A cannot reach instance B"
    fi

    # [4] DNS rewrite in an OW instance: OW_DNS → resolv.conf → in-instance
    # resolution (the runsc fix).
    docker run -d --name "$ctr_dns" --runtime "$rt" --network "$net_dns" \
        -e "OW_DNS=$OW_DNS" "$DNS_IMAGE" >/dev/null \
        || fail "runtime $rt: could not start DNS instance"
    local t resolv ok
    resolv=""
    for t in $(seq 1 30); do
        if [ "$(docker inspect -f '{{.State.Running}}' "$ctr_dns" 2>/dev/null || echo false)" = "true" ]; then
            if out="$(docker exec "$ctr_dns" cat /etc/resolv.conf 2>/dev/null)"; then
                if [[ "$out" == *"8.8.8.8"* ]] && [[ "$out" == *"1.1.1.1"* ]]; then
                    resolv="$out"
                    break
                fi
            fi
        fi
        sleep 1
    done
    if [ -n "$resolv" ]; then
        log "    PASS [4]: resolv.conf rewritten to $OW_DNS ($t s)"
        ok=""
        for t in $(seq 1 10); do
            if exec_in "$ctr_dns" getent hosts example.com; then
                ok=1
                break
            fi
            sleep 1
        done
        if [ -n "$ok" ]; then
            log "    PASS [4]: in-instance resolution works (getent hosts example.com, ${t}s)"
        else
            note "    FAIL [4]: resolv.conf rewritten but in-instance resolution failed"
            FAILED=$((FAILED + 1))
        fi
    else
        note "    FAIL [4]: resolv.conf was not rewritten to $OW_DNS"
        docker logs "$ctr_dns" >&2 || true
        FAILED=$((FAILED + 1))
    fi

    # Tear down this runtime's instances + bridges now so each runtime is
    # independently clean; the trap also covers a failed run.
    for c in "$ctr_a" "$ctr_b" "$ctr_dns"; do
        docker rm -f "$c" >/dev/null 2>&1 || true
    done
    for n in "$net_a" "$net_b" "$net_dns"; do
        docker network rm "$n" >/dev/null 2>&1 || true
    done
    CREATED_NETS=""
    CREATED_CTRS=""
}

for rt in $RUNTIMES; do
    run_runtime "$rt"
done

# [5] the script leaves nothing behind: no bridge it created survives.
LEFTOVER="$(docker network ls --format '{{.Name}}' | grep '^ow-smoke-' || true)"
if [ -n "$LEFTOVER" ]; then
    note "    FAIL [5]: leftover bridges from this script: $LEFTOVER"
    FAILED=$((FAILED + 1))
else
    log "    PASS [5]: no leftover ow-smoke-* bridges after teardown"
fi

echo "Result: $FAILED check(s) failed."
[ "$FAILED" -eq 0 ] || exit 1
echo "All isolation + DNS smoke checks passed on: $RUNTIMES"
