#!/usr/bin/env bash
# benchmark-prod.sh — measure the production OpenWorkspace stack's CPU/RAM.
#
# Pipeline (single script; --phase picks a subset; --smoke shortens windows):
#   [preflight]  runsc registered, port 80 free, dini images present (build if missing)
#   [phase 1]    host-before: sample /proc/stat + /proc/meminfo 1/sec (idle host)
#   [phase 2]    compose build (unmeasured) -> up -d -> wait healthy ->
#                sample the 4 platform containers + host 1/sec
#   [phase 3]    API login -> create 6 templates (3 types x runc/runsc, dini) ->
#                launch 6 instances (no_persistent) -> wait all running ->
#                synchronized 1/min window sampling all 6 + host (platform+instances)
#   [cleanup]    delete instances/templates, compose down -> report (CSV + markdown)
#
# Dependencies: bash + docker + curl + jq (no node, no python).
#
# Usage:
#   ./scripts/benchmark/benchmark-prod.sh [--phase N] [--smoke] [--seconds N] [--out DIR]
#
# Environment:
#   OW_ADMIN_USER / OW_ADMIN_PASSWORD   admin creds (defaults admin/admin)
#   OW_BASE_URL                         API base via Traefik (default http://localhost)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
COMPOSE_FILE="$REPO_ROOT/docker/openworkspace/docker-compose.yml"

source "$SCRIPT_DIR/benchlib.sh"

OW_ADMIN_USER="${OW_ADMIN_USER:-admin}"
OW_ADMIN_PASSWORD="${OW_ADMIN_PASSWORD:-admin}"
OW_BASE_URL="${OW_BASE_URL:-http://localhost}"
API="$OW_BASE_URL/api"

SECONDS_PER_WINDOW=60
PHASE=0            # 0 = all phases
SMOKE=0
OUT_DIR=""
COOKIE_JAR="$(mktemp)"

# Created resources (for teardown). Use the template name as the handle so the
# teardown knows exactly what to delete even if a launch failed partway.
TEMPLATE_IDS=()
INSTANCE_IDS=()
STACK_UP=0

# Instance matrix: "template_name|remote_type|image|runtime_value|runtime_label"
# runtime_value is the API's container_runtime field; "docker" maps to runC.
MATRIX=(
    "bench-runsc-kasmvnc|kasmvnc|tsukisama9292/ow-kasmvnc-ubuntu-dini:jammy|runsc|runsc"
    "bench-runc-kasmvnc|kasmvnc|tsukisama9292/ow-kasmvnc-ubuntu-dini:jammy|docker|runc"
    "bench-runsc-ttyd|ttyd|tsukisama9292/ow-ttyd-ubuntu-dini:jammy|runsc|runsc"
    "bench-runc-ttyd|ttyd|tsukisama9292/ow-ttyd-ubuntu-dini:jammy|docker|runc"
    "bench-runsc-jupyter|jupyter|tsukisama9292/ow-jupyter-ubuntu-dini:jammy|runsc|runsc"
    "bench-runc-jupyter|jupyter|tsukisama9292/ow-jupyter-ubuntu-dini:jammy|docker|runc"
)
PLATFORM_CONTAINERS="ow-traefik ow-postgres ow-web ow-api"

log() { echo "==> $*"; }
note() { echo "    $*"; }
fail() { echo "ERROR: $*" >&2; exit 1; }

usage() {
    sed -n '2,20p' "$0" | sed 's/^# \{0,1\}//'
    echo
    echo "Options:"
    echo "  --phase N      run one phase (1=host-before, 2=platform, 3=instances); default all"
    echo "  --smoke        short windows (~5s) for fast end-to-end verification"
    echo "  --seconds N    sampling window length (default 60)"
    echo "  --out DIR      report output directory (default scripts/benchmark/reports/bench-<ts>)"
}

# ---------------------------------------------------------------------------
# Preflight
# ---------------------------------------------------------------------------
preflight() {
    log "Preflight"
    for bin in docker curl jq awk ss; do
        command -v "$bin" >/dev/null || fail "'$bin' is not installed"
    done

    if docker info 2>/dev/null | grep -q 'runsc'; then
        note "PASS: runsc runtime registered"
    else
        fail "runsc runtime is not registered — run 'sudo bash scripts/docker-runtime-gvisor.sh' (pnpm run init) first"
    fi

    if ss -ltn 2>/dev/null | grep -q ':80 '; then
        fail "port 80 is in use (dev stack or another traefik?) — stop it first"
    else
        note "PASS: port 80 free"
    fi

    local images_missing=0
    for entry in "${MATRIX[@]}"; do
        local image
        image="${entry#*|*|}"
        image="${image%%|*}"
        if ! docker image inspect "$image" >/dev/null 2>&1; then
            note "missing image: $image"
            images_missing=$((images_missing + 1))
        fi
    done
    if [[ "$images_missing" -gt 0 ]]; then
        note "building template images ($images_missing missing) via docker/template_images/build.sh..."
        (cd "$REPO_ROOT" && bash docker/template_images/build.sh) \
            || fail "image build failed — build manually with docker/template_images/build.sh"
    fi
    for entry in "${MATRIX[@]}"; do
        local image
        image="${entry#*|*|}"
        image="${image%%|*}"
        docker image inspect "$image" >/dev/null 2>&1 || fail "image '$image' still missing after build"
    done
    note "PASS: all dini images present"
}

# ---------------------------------------------------------------------------
# Host sampling
# ---------------------------------------------------------------------------

# measure_host_window OUT_FILE TICKS: sample /proc/stat + /proc/meminfo 1/sec.
# CSV: timestamp,cpu_percent,mem_available_bytes,mem_total_bytes
measure_host_window() {
    local out="$1" ticks="$2"
    : > "$out"
    local prev cur cpu avail total ts
    prev=$(cat /proc/stat)
    for ((i = 1; i <= ticks; i++)); do
        sleep 1
        cur=$(cat /proc/stat)
        cpu=$(cpu_utilization "$prev" "$cur")
        avail=$(meminfo_available_bytes "$(cat /proc/meminfo)")
        total=$(meminfo_total_bytes "$(cat /proc/meminfo)")
        ts=$(date +%s)
        csv_record "$ts" "$cpu" "$avail" "$total" >> "$out"
        prev="$cur"
    done
}

# measure_container_window OUT_FILE TICKS [ALLOWLIST]: sample docker stats 1/sec.
# CSV: timestamp,container,cpu_percent,mem_bytes
measure_container_window() {
    local out="$1" ticks="$2" allowlist="${3:-}"
    : > "$out"
    local ts row name cpu mem line
    for ((i = 1; i <= ticks; i++)); do
        ts=$(date +%s)
        while IFS= read -r line; do
            row=$(parse_docker_stats_line "$line")
            IFS='|' read -r name cpu mem <<<"$row"
            if [[ -z "$allowlist" ]] || [[ " $allowlist " == *" $name "* ]]; then
                csv_record "$ts" "$name" "$cpu" "$mem" >> "$out"
            fi
        done < <(docker stats --no-stream --format '{{json .}}' 2>/dev/null)
        if (( i < ticks )); then sleep 1; fi
    done
}

# measure_host_and_containers HOST_OUT CTR_OUT TICKS ALLOWLIST: one synchronized
# window sampling both the host and the allowlisted containers every second.
measure_host_and_containers() {
    local host_out="$1" ctr_out="$2" ticks="$3" allowlist="$4"
    : > "$host_out"
    : > "$ctr_out"
    local prev cur cpu avail total ts row name cpu_pct mem_bytes line
    prev=$(cat /proc/stat)
    for ((i = 1; i <= ticks; i++)); do
        sleep 1
        cur=$(cat /proc/stat)
        cpu=$(cpu_utilization "$prev" "$cur")
        avail=$(meminfo_available_bytes "$(cat /proc/meminfo)")
        total=$(meminfo_total_bytes "$(cat /proc/meminfo)")
        ts=$(date +%s)
        csv_record "$ts" "$cpu" "$avail" "$total" >> "$host_out"
        prev="$cur"
        while IFS= read -r line; do
            row=$(parse_docker_stats_line "$line")
            IFS='|' read -r name cpu_pct mem_bytes <<<"$row"
            if [[ " $allowlist " == *" $name "* ]]; then
                csv_record "$ts" "$name" "$cpu_pct" "$mem_bytes" >> "$ctr_out"
            fi
        done < <(docker stats --no-stream --format '{{json .}}' 2>/dev/null)
    done
}

# ---------------------------------------------------------------------------
# API helpers (cookie-based auth)
# ---------------------------------------------------------------------------
api_login() {
    log "Logging into API as '$OW_ADMIN_USER'"
    local code
    code=$(curl -sS -o /dev/null -w '%{http_code}' -c "$COOKIE_JAR" \
        -H 'Content-Type: application/json' \
        -d "{\"username\":\"$OW_ADMIN_USER\",\"password\":\"$OW_ADMIN_PASSWORD\"}" \
        "$API/auth/login")
    [[ "$code" == "200" ]] || fail "login failed (HTTP $code) — check OW_ADMIN_USER/OW_ADMIN_PASSWORD"
}

api_create_template() {
    local name="$1" remote_type="$2" image="$3" runtime="$4"
    local body
    body=$(curl -sS -b "$COOKIE_JAR" -H 'Content-Type: application/json' \
        -d "{\"name\":\"$name\",\"image\":\"$image\",\"remote_type\":\"$remote_type\",\"container_runtime\":\"$runtime\",\"docker_in_instance\":true}" \
        "$API/templates")
    local id
    id=$(printf '%s' "$body" | jq -r '.template.id // empty')
    [[ -n "$id" ]] || fail "template create failed for '$name': $body"
    TEMPLATE_IDS+=("$id")
    note "created template '$name' (id=$id, runtime=$runtime)"
}

api_launch_instance() {
    local template_id="$1"
    local body
    body=$(curl -sS -b "$COOKIE_JAR" -H 'Content-Type: application/json' \
        -d "{\"template_id\":\"$template_id\",\"persistence\":\"no_persistent\"}" \
        "$API/instances")
    local id
    id=$(printf '%s' "$body" | jq -r '.instance.id // empty')
    [[ -n "$id" ]] || fail "instance launch failed: $body"
    INSTANCE_IDS+=("$id")
    note "launched instance (id=$id)"
}

api_wait_all_running() {
    local deadline=$(( $(date +%s) + 60 )) all_running
    while [[ "$(date +%s)" -lt "$deadline" ]]; do
        all_running=1
        local body
        body=$(curl -sS -b "$COOKIE_JAR" "$API/instances")
        local id
        for id in "${INSTANCE_IDS[@]}"; do
            local status
            status=$(printf '%s' "$body" | jq -r --arg id "$id" '.instances[] | select(.id == $id) | .status // "unknown"')
            if [[ "$status" != "running" ]]; then
                all_running=0
                break
            fi
        done
        if [[ "$all_running" -eq 1 ]]; then
            note "all instances running"
            return 0
        fi
        sleep 2
    done
    fail "timed out waiting for instances to reach 'running'"
}

api_delete_instance() {
    local id="$1"
    local code
    code=$(curl -sS -o /dev/null -w '%{http_code}' -X DELETE -b "$COOKIE_JAR" "$API/instances/$id")
    [[ "$code" == "204" ]] || note "instance $id delete returned HTTP $code (cleanup failure)"
}

api_delete_template() {
    local id="$1"
    local code
    code=$(curl -sS -o /dev/null -w '%{http_code}' -X DELETE -b "$COOKIE_JAR" "$API/templates/$id")
    [[ "$code" == "204" ]] || note "template $id delete returned HTTP $code (cleanup failure)"
}

# ---------------------------------------------------------------------------
# Compose lifecycle
# ---------------------------------------------------------------------------
compose_build() {
    log "Building production compose (not measured)"
    docker compose -f "$COMPOSE_FILE" build || fail "compose build failed"
}

compose_up() {
    log "Starting production compose"
    docker compose -f "$COMPOSE_FILE" up -d || fail "compose up failed"
    STACK_UP=1

    log "Waiting for all platform containers healthy"
    local deadline=$(( $(date +%s) + 180 ))
    while [[ "$(date +%s)" -lt "$deadline" ]]; do
        local all_healthy=1 name running health
        for name in $PLATFORM_CONTAINERS; do
            running=$(docker inspect -f '{{.State.Running}}' "$name" 2>/dev/null || echo "missing")
            health=$(docker inspect -f '{{if .State.Health}}{{.State.Health.Status}}{{else}}none{{end}}' "$name" 2>/dev/null || echo "missing")
            # No healthcheck defined -> "none": running is the readiness signal.
            if [[ "$running" != "true" ]] || { [[ "$health" != "none" ]] && [[ "$health" != "healthy" ]]; }; then
                all_healthy=0
                break
            fi
        done
        if [[ "$all_healthy" -eq 1 ]]; then
            note "all platform containers healthy"
            return 0
        fi
        sleep 3
    done
    fail "timed out waiting for platform containers to become healthy"
}

compose_down() {
    if [[ "$STACK_UP" -eq 1 ]]; then
        log "Stopping production compose"
        docker compose -f "$COMPOSE_FILE" down || note "compose down failed (manual: docker compose -f $COMPOSE_FILE down)"
        STACK_UP=0
    fi
}

# ---------------------------------------------------------------------------
# Report assembly
# ---------------------------------------------------------------------------
# per-container samples: awk over a CSV -> "cpu mem" lines for one container
container_samples() {
    local csv="$1" name="$2"
    awk -F, -v n="$name" '$2 == n { print $3, $4 }' "$csv"
}

# peak rows for md_peak_table: "name|peak_cpu|peak_mem"
peak_rows() {
    local csv="$1"; shift
    local name agg pc pm
    for name in "$@"; do
        agg=$(aggregate_cpu_mem "$(container_samples "$csv" "$name")")
        IFS='|' read -r pc _ pm _ <<<"$agg"
        echo "$name|$pc|$pm"
    done
}

# runC-vs-runsc rows for md_runr_compare:
# "runtime_label|remote_type|mean_cpu|peak_cpu|mean_mem|peak_mem"
runr_rows() {
    local csv="$1"
    local tpl type label agg pc mc pm mm
    for entry in "${MATRIX[@]}"; do
        IFS='|' read -r tpl type _ _ label <<<"$entry"
        agg=$(aggregate_cpu_mem "$(container_samples "$csv" "${tpl}-1")")
        IFS='|' read -r pc mc pm mm <<<"$agg"
        echo "$label|$type|$mc|$pc|$mm|$pm"
    done
}

write_report() {
    local host_before="$1" host_after="$2" platform_csv="$3" instances_csv="$4"
    local report="$OUT_DIR/report.md"
    local default_runtime digests="" seen="" compose_rev="unknown"
    default_runtime=$(docker info --format '{{.DefaultRuntime}}' 2>/dev/null || echo "unknown")
    if git -C "$REPO_ROOT" rev-parse --short HEAD >/dev/null 2>&1; then
        compose_rev=$(git -C "$REPO_ROOT" rev-parse --short HEAD)
    fi
    local entry image
    for entry in "${MATRIX[@]}"; do
        IFS='|' read -r _ _ image _ _ <<<"$entry"
        if [[ " $seen " != *" $image "* ]]; then
            seen+=" $image"
        fi
    done
    for image in $seen; do
        digests+=" $(docker image inspect --format '{{index .RepoDigests 0}}' "$image" 2>/dev/null || echo "$image")"
    done
    {
        echo "# Production stack CPU/RAM benchmark"
        echo
        echo "- Timestamp: $(date -Is)"
        echo "- Windows: $SECONDS_PER_WINDOW s each (1 sample/s)"
        echo "- Docker default runtime: $default_runtime"
        echo "- Compose file: docker/openworkspace/docker-compose.yml @ $compose_rev"
        echo "- Platform: $PLATFORM_CONTAINERS"
        echo "- Instances: 3 remote types x runc/runsc, dini, no_persistent"
        echo "- Template images:"
        local d
        for d in $digests; do
            echo "  - $d"
        done
        echo
        echo "## 1. Platform container peaks"
        echo
        md_peak_table "container" "$(peak_rows "$platform_csv" ow-traefik ow-postgres ow-web ow-api)"
        echo
        echo "## 2. Per-instance peaks"
        echo
        local rows="" entry name remote_type runtime_label agg pc pm
        for entry in "${MATRIX[@]}"; do
            IFS='|' read -r name remote_type _ _ runtime_label <<<"$entry"
            agg=$(aggregate_cpu_mem "$(container_samples "$instances_csv" "${name}-1")")
            IFS='|' read -r pc _ pm _ <<<"$agg"
            if [[ -n "$pc" ]]; then
                rows+="| ${name}-1 | $remote_type | $runtime_label | $pc | $pm |\n"
            fi
        done
        printf '| instance | remote_type | runtime | peak_cpu | peak_mem |\n'
        printf '| --- | --- | --- | --- | --- |\n'
        printf '%b' "$rows"
        echo
        echo "## 3. runC vs runsc aggregate (per remote type)"
        echo
        md_runr_compare "$(runr_rows "$instances_csv")"
        echo
        echo "## 4. Host before -> after"
        echo
        local b_cpu b_mem a_cpu a_mem
        b_cpu=$(aggregate_cpu_mem "$(awk -F, '{print $2, $3}' "$host_before")" | cut -d'|' -f2)
        a_cpu=$(aggregate_cpu_mem "$(awk -F, '{print $2, $3}' "$host_after")" | cut -d'|' -f2)
        b_mem=$(aggregate_cpu_mem "$(awk -F, '{print $2, $3}' "$host_before")" | cut -d'|' -f4)
        a_mem=$(aggregate_cpu_mem "$(awk -F, '{print $2, $3}' "$host_after")" | cut -d'|' -f4)
        md_delta_table "$(printf 'cpu_percent|%s|%s|%.2f\nmem_available_bytes|%s|%s|%s\n' \
            "$b_cpu" "$a_cpu" "$(awk -v a="$a_cpu" -v b="$b_cpu" 'BEGIN{print a-b}')" \
            "$b_mem" "$a_mem" "$((a_mem - b_mem))")"
        echo
    } > "$report"
    note "report written: $report"
    echo "--- peak values ---"
    cat "$report"
}

# ---------------------------------------------------------------------------
# Teardown
# ---------------------------------------------------------------------------
cleanup() {
    log "Teardown"
    local id failed=0
    for id in "${INSTANCE_IDS[@]}"; do
        api_delete_instance "$id" || failed=1
    done
    for id in "${TEMPLATE_IDS[@]}"; do
        api_delete_template "$id" || failed=1
    done
    compose_down || failed=1
    rm -f "$COOKIE_JAR"
    if [[ "$failed" -eq 1 ]]; then
        echo "CLEANUP INCOMPLETE — manual removal required:" >&2
        for id in "${INSTANCE_IDS[@]}"; do
            echo "  instance:  curl -X DELETE -b \"$COOKIE_JAR\" \"$API/instances/$id\"" >&2
        done
        for id in "${TEMPLATE_IDS[@]}"; do
            echo "  template:  curl -X DELETE -b \"$COOKIE_JAR\" \"$API/templates/$id\"" >&2
        done
        echo "  compose:   docker compose -f $COMPOSE_FILE down" >&2
        return 1
    fi
    note "teardown complete"
    return 0
}
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    trap 'cleanup || exit 1' EXIT
fi

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
main() {
    local host_before_csv="$OUT_DIR/host-before.csv"
    local host_after_csv="$OUT_DIR/host-after.csv"
    local platform_csv="$OUT_DIR/platform.csv"
    local instances_csv="$OUT_DIR/instances.csv"

    preflight

    if [[ "$PHASE" == "0" || "$PHASE" == "1" ]]; then
        log "Phase 1: host before (idle, no platform) — ${SECONDS_PER_WINDOW}s"
        measure_host_window "$host_before_csv" "$SECONDS_PER_WINDOW"
    fi

    if [[ "$PHASE" == "0" || "$PHASE" == "2" ]]; then
        log "Phase 2: platform idle — ${SECONDS_PER_WINDOW}s"
        compose_build
        compose_up
        measure_container_window "$platform_csv" "$SECONDS_PER_WINDOW" "$PLATFORM_CONTAINERS"
    fi

    if [[ "$PHASE" == "0" || "$PHASE" == "3" ]]; then
        log "Phase 3: six concurrent instances — synchronized ${SECONDS_PER_WINDOW}s window"
        if [[ "$STACK_UP" -ne 1 ]]; then
            compose_up
        fi
        api_login
        local entry name remote_type image runtime
        for entry in "${MATRIX[@]}"; do
            IFS='|' read -r name remote_type image runtime _ <<<"$entry"
            api_create_template "$name" "$remote_type" "$image" "$runtime"
        done
        local tpl_id
        for tpl_id in "${TEMPLATE_IDS[@]}"; do
            api_launch_instance "$tpl_id"
        done
        api_wait_all_running
        local instance_names=""
        local e name
        for e in "${MATRIX[@]}"; do
            name="${e%%|*}"
            instance_names+=" ${name}-1"
        done
        measure_host_and_containers "$host_after_csv" "$instances_csv" "$SECONDS_PER_WINDOW" "$instance_names"
    fi

    write_report "$host_before_csv" "$host_after_csv" "$platform_csv" "$instances_csv"
}

# ---------------------------------------------------------------------------
# Entry point (invoked last so `main` is defined; skipped when sourced for
# testing — functions only, no side effects).
# ---------------------------------------------------------------------------
if [[ "${BASH_SOURCE[0]}" != "$0" ]]; then
    log "sourced mode: functions available for testing (main not run)"
else
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --phase) PHASE="$2"; shift 2 ;;
            --smoke) SMOKE=1; shift ;;
            --seconds) SECONDS_PER_WINDOW="$2"; shift 2 ;;
            --out) OUT_DIR="$2"; shift 2 ;;
            -h|--help) usage; exit 0 ;;
            *) fail "unknown argument: $1 (try --help)" ;;
        esac
    done

    if [[ "$SMOKE" -eq 1 ]]; then
        SECONDS_PER_WINDOW="${SMOKE_SECONDS:-5}"
        log "SMOKE mode: windows shortened to ${SECONDS_PER_WINDOW}s"
    fi

    if [[ -z "$OUT_DIR" ]]; then
        OUT_DIR="$SCRIPT_DIR/reports/bench-$(date +%Y%m%d-%H%M%S)"
    fi
    mkdir -p "$OUT_DIR"

    main "$@"
fi
