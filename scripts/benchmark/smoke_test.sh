#!/usr/bin/env bash
# smoke_test.sh — end-to-end verification of the production benchmark pipeline.
#
# Proves benchmark-prod.sh --smoke works against a real host + the production
# compose stack, on both runtimes of the instance matrix, and leaves the host
# clean. Same shape as scripts/network_isolation_smoke_test.sh:
#
#   [1] the orchestrator's preflight passes (or fails with its fix message)
#   [2] all four platform containers reach ready (running + healthy where a
#       healthcheck exists)
#   [3] all six instances (3 remote types x runc/runsc) reach running
#   [4] the synchronized window samples all six instance containers + the host,
#       and the report (CSVs + report.md with all four tables) is produced
#   [5] teardown leaves no bench containers, no ow-* instance networks, no
#       bench instance/template rows in the DB, and the compose stack down —
#       the host is back to its pre-run state
#
# Usage:
#   ./scripts/benchmark/smoke_test.sh
#
# Environment overrides (all optional, forwarded to the orchestrator):
#   OW_ADMIN_USER / OW_ADMIN_PASSWORD   admin creds (defaults admin/admin)
#   OW_BASE_URL                         API base via Traefik (default http://localhost)
#
# Requirements: a docker host with runsc registered and port 80 free (the
# orchestrator's own preflight enforces this and prints the fix steps).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
COMPOSE_FILE="$REPO_ROOT/docker/openworkspace/docker-compose.yml"
BENCH="$SCRIPT_DIR/benchmark-prod.sh"

FAILED=0
BENCH_PID=""
BENCH_RC=""
OUT="$(mktemp -d)"
BENCH_LOG="$OUT/bench.log"
STACK_UP=0

log() { echo "==> $*"; }
note() { echo "    $*"; }
fail() { echo "ERROR: $*" >&2; exit 1; }

# Unique name on purpose: sourcing benchmark-prod.sh (for api_login/compose_up
# during [5b]) redefines `cleanup`/`log`/`note`/`fail` in this shell, so the
# EXIT trap must point at a function the sourced script does not clobber.
smoke_cleanup() {
    # If the bench is still running (or hung), kill it — its own EXIT trap runs
    # its teardown. Then make sure the stack is down and nothing we created is
    # left behind.
    if [[ -n "$BENCH_PID" ]] && kill -0 "$BENCH_PID" 2>/dev/null; then
        kill "$BENCH_PID" 2>/dev/null || true
        wait "$BENCH_PID" 2>/dev/null || true
    fi
    if [[ "$STACK_UP" -eq 1 ]]; then
        docker compose -f "$COMPOSE_FILE" down >/dev/null 2>&1 || true
    fi
    for c in $(docker ps -aq --filter "name=bench-" 2>/dev/null || true); do
        docker rm -f "$c" >/dev/null 2>&1 || true
    done
    for n in $(docker network ls --format '{{.Name}}' 2>/dev/null | grep -E '^ow-[0-9a-f]' || true); do
        docker network rm "$n" >/dev/null 2>&1 || true
    done
    rm -rf "$OUT"
}
trap smoke_cleanup EXIT

for bin in docker curl jq; do
    command -v "$bin" >/dev/null || fail "'$bin' is not installed"
done

# ── [1] orchestrator preflight + full --smoke run, in the background ──
log "Running: bash benchmark-prod.sh --smoke (background)"
bash "$BENCH" --smoke --out "$OUT/bench" >"$BENCH_LOG" 2>&1 &
BENCH_PID=$!

# ── [2] wait for the four platform containers to be ready ──
log "[2] waiting for platform containers to be ready"
platform_ok=0
for _ in $(seq 1 150); do
    if ! kill -0 "$BENCH_PID" 2>/dev/null; then break; fi
    all_ready=1
    for name in ow-traefik ow-postgres ow-web ow-api; do
        running=$(docker inspect -f '{{.State.Running}}' "$name" 2>/dev/null || echo "missing")
        health=$(docker inspect -f '{{if .State.Health}}{{.State.Health.Status}}{{else}}none{{end}}' "$name" 2>/dev/null || echo "missing")
        if [[ "$running" != "true" ]] || [[ "$health" != "none" && "$health" != "healthy" ]]; then
            all_ready=0
            break
        fi
    done
    if [[ "$all_ready" -eq 1 ]]; then
        platform_ok=1
        note "PASS [2]: all platform containers ready"
        break
    fi
    sleep 2
done
if [[ "$platform_ok" -ne 1 ]]; then
    note "    FAIL [2]: platform containers never became ready"
    FAILED=$((FAILED + 1))
fi

# ── [3] wait for all six instance containers to be running ──
log "[3] waiting for all six instance containers to be running"
instances_ok=0
for _ in $(seq 1 90); do
    if ! kill -0 "$BENCH_PID" 2>/dev/null; then break; fi
    running_ctrs="$(docker ps --format '{{.Names}}' | grep '^bench-' || true)"
    if [[ "$(wc -l <<<"$running_ctrs")" -ge 6 ]]; then
        instances_ok=1
        note "PASS [3]: six instance containers running"
        break
    fi
    sleep 2
done
if [[ "$instances_ok" -ne 1 ]]; then
    note "    FAIL [3]: fewer than six bench-* containers became running"
    note "        observed: $(docker ps --format '{{.Names}}' | grep '^bench-' || echo none)"
    FAILED=$((FAILED + 1))
fi

# ── wait for the orchestrator to finish; capture its exit code ──
log "Waiting for orchestrator to finish"
set +e
wait "$BENCH_PID"
BENCH_RC=$?
set -e
BENCH_PID=""
if [[ "$BENCH_RC" -ne 0 ]]; then
    note "    FAIL: orchestrator exited $BENCH_RC"
    note "        --- benchmark-prod.sh log (tail) ---"
    tail -30 "$BENCH_LOG" | sed 's/^/        /'
    FAILED=$((FAILED + 1))
else
    note "PASS: orchestrator exited 0"
fi

# ── [4] report files produced with all four tables ──
log "[4] checking report output"
REPORT_DIR="$OUT/bench"
for f in host-before.csv host-after.csv platform.csv instances.csv report.md; do
    if [[ -s "$REPORT_DIR/$f" ]]; then
        note "PASS [4]: $f present ($(wc -l < "$REPORT_DIR/$f") lines)"
    else
        note "    FAIL [4]: $f missing or empty"
        FAILED=$((FAILED + 1))
    fi
done
for header in \
    "| container | peak_cpu | peak_mem |" \
    "| instance | remote_type | runtime | peak_cpu | peak_mem |" \
    "| runtime | remote_type | mean_cpu | peak_cpu | mean_mem | peak_mem |" \
    "| metric | before | after | delta |"; do
    if grep -qF "$header" "$REPORT_DIR/report.md"; then
        note "PASS [4]: report.md table [$header] present"
    else
        note "    FAIL [4]: report.md missing table [$header]"
        FAILED=$((FAILED + 1))
    fi
done
missing_samples=0
for name in bench-runsc-kasmvnc-1 bench-runc-kasmvnc-1 bench-runsc-ttyd-1 bench-runc-ttyd-1 bench-runsc-jupyter-1 bench-runc-jupyter-1; do
    if grep -q ",$name," "$REPORT_DIR/instances.csv"; then
        note "PASS [4]: instances.csv sampled $name"
    else
        note "    FAIL [4]: instances.csv never sampled $name"
        missing_samples=$((missing_samples + 1))
    fi
done
[[ "$missing_samples" -eq 0 ]] || FAILED=$((FAILED + 1))

# ── [5] teardown left the host clean ──
log "[5] checking post-teardown state"
leftover_ctrs="$(docker ps -aq --filter "name=bench-" | wc -l | tr -d ' ')"
if [[ "$leftover_ctrs" -eq 0 ]]; then
    note "PASS [5]: no leftover bench-* containers"
else
    note "    FAIL [5]: $leftover_ctrs leftover bench-* container(s): $(docker ps -a --format '{{.Names}}' | grep '^bench-')"
    FAILED=$((FAILED + 1))
fi
leftover_nets="$(docker network ls --format '{{.Name}}' | grep -E '^ow-[0-9a-f]' || true)"
if [[ -z "$leftover_nets" ]]; then
    note "PASS [5]: no leftover instance networks"
else
    note "    FAIL [5]: leftover instance networks: $leftover_nets"
    FAILED=$((FAILED + 1))
fi
if docker ps --format '{{.Names}}' | grep -qE '^(ow-traefik|ow-postgres|ow-web|ow-api)$'; then
    note "    FAIL [5]: compose stack still up"
    FAILED=$((FAILED + 1))
else
    note "PASS [5]: compose stack down"
fi

# ── [5b] DB rows: bring the stack back up, login, assert no bench rows ──
log "[5b] verifying no bench instance/template rows in the DB"
source "$SCRIPT_DIR/benchmark-prod.sh" 2>/dev/null
compose_up >/dev/null 2>&1 || fail "compose up failed during DB verification"
STACK_UP=1
api_ready=0
for _ in $(seq 1 60); do
    code=$(curl -sS -o /dev/null -w '%{http_code}' "$API/instances" 2>/dev/null || echo 000)
    if [[ "$code" != "000" ]]; then
        api_ready=1
        break
    fi
    sleep 2
done
[[ "$api_ready" -eq 1 ]] || fail "api did not come up during DB verification"
api_login >/dev/null 2>&1 || fail "api login failed during DB verification"
set +e
instances_body=$(curl -sS -b "$COOKIE_JAR" "$API/instances")
templates_body=$(curl -sS -b "$COOKIE_JAR" "$API/templates")
set -e
bench_rows="$(printf '%s' "$instances_body" | jq -r '.instances[].id' | wc -l | tr -d ' ')"
if [[ "$bench_rows" -eq 0 ]]; then
    note "PASS [5b]: no instance rows remain"
else
    note "    FAIL [5b]: $bench_rows instance row(s) remain: $(printf '%s' "$instances_body" | jq -r '.instances[] | [.name, .status] | @tsv' | sed 's/^/        /')"
    FAILED=$((FAILED + 1))
fi
leftover_tpls="$(printf '%s' "$templates_body" | jq -r '.templates[].name' | grep '^bench-' || true)"
if [[ -z "$leftover_tpls" ]]; then
    note "PASS [5b]: no bench-* template rows remain"
else
    note "    FAIL [5b]: leftover template rows: $leftover_tpls"
    FAILED=$((FAILED + 1))
fi
docker compose -f "$COMPOSE_FILE" down >/dev/null 2>&1 || note "compose down failed after DB verification"
STACK_UP=0

echo "Result: $FAILED check(s) failed."
[ "$FAILED" -eq 0 ] || exit 1
echo "All production-benchmark smoke checks passed."
