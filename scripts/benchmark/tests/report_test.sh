#!/usr/bin/env bash
# Synthetic-data test for the orchestrator's report assembly.
# Sources benchmark-prod.sh (no main run) and exercises write_report with
# fabricated CSVs, asserting the report's four tables.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

OUT_DIR="$(mktemp -d)"
trap 'rm -rf "$OUT_DIR"' EXIT

source "$REPO_ROOT/scripts/benchmark/benchmark-prod.sh" 2>/dev/null
OUT_DIR="$(mktemp -d)"
trap 'rm -rf "$OUT_DIR"' EXIT

printf '1,0.50,16000000000,32000000000\n2,0.40,15900000000,32000000000\n3,0.60,15800000000,32000000000\n' > "$OUT_DIR/host-before.csv"
printf '1,5.00,14000000000,32000000000\n2,6.00,13900000000,32000000000\n3,7.00,13800000000,32000000000\n' > "$OUT_DIR/host-after.csv"
printf '1,ow-traefik,0.10,10485760\n2,ow-traefik,0.20,11534336\n1,ow-postgres,1.00,52428800\n2,ow-postgres,1.50,55574528\n1,ow-web,0.05,20971520\n2,ow-web,0.10,22020096\n1,ow-api,0.30,31457280\n2,ow-api,0.35,32505856\n' > "$OUT_DIR/platform.csv"
printf '1,bench-runsc-kasmvnc-1,1.00,10485760\n2,bench-runsc-kasmvnc-1,1.50,11534336\n1,bench-runc-kasmvnc-1,0.80,10485760\n2,bench-runc-kasmvnc-1,0.90,11534336\n1,bench-runsc-ttyd-1,0.20,1048576\n2,bench-runsc-ttyd-1,0.25,1572864\n1,bench-runc-ttyd-1,0.15,1048576\n2,bench-runc-ttyd-1,0.18,1572864\n1,bench-runsc-jupyter-1,2.00,52428800\n2,bench-runsc-jupyter-1,2.50,55574528\n1,bench-runc-jupyter-1,1.80,52428800\n2,bench-runc-jupyter-1,2.00,55574528\n' > "$OUT_DIR/instances.csv"

write_report "$OUT_DIR/host-before.csv" "$OUT_DIR/host-after.csv" "$OUT_DIR/platform.csv" "$OUT_DIR/instances.csv" 2>/dev/null >/dev/null

FAILED=0
assert_contains() {
    local desc="$1" needle="$2" file="$3"
    if grep -qF -- "$needle" "$file"; then
        echo "PASS: $desc"
    else
        echo "FAIL: $desc — missing [$needle] in report"
        FAILED=1
    fi
}

report="$OUT_DIR/report.md"

assert_contains "platform peak table header" "| container | peak_cpu | peak_mem |" "$report"
assert_contains "platform peak row traefik" "| ow-traefik | 0.20 | 11534336 |" "$report"
assert_contains "platform peak row postgres" "| ow-postgres | 1.50 | 55574528 |" "$report"
assert_contains "instance peak header" "| instance | remote_type | runtime | peak_cpu | peak_mem |" "$report"
assert_contains "instance peak row runsc kasmvnc" "| bench-runsc-kasmvnc-1 | kasmvnc | runsc | 1.50 | 11534336 |" "$report"
assert_contains "runr table header" "| runtime | remote_type | mean_cpu | peak_cpu | mean_mem | peak_mem |" "$report"
assert_contains "runr row runsc kasmvnc" "| runsc | kasmvnc | 1.25 | 1.50 |" "$report"
assert_contains "runr row runc jupyter" "| runc | jupyter | 1.90 | 2.00 |" "$report"
assert_contains "host delta header" "| metric | before | after | delta |" "$report"
assert_contains "host cpu delta" "| cpu_percent | 0.50 | 6.00 | 5.50 |" "$report"
assert_contains "host mem delta" "| mem_available_bytes | 15900000000 | 13900000000 | -2000000000 |" "$report"

if [[ "$FAILED" -eq 0 ]]; then
    echo "All report-assembly tests passed"
else
    echo "Some report-assembly tests failed" >&2
    exit 1
fi
