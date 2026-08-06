#!/usr/bin/env bash
# Fixture-driven unit tests for benchlib.sh.
# Pure functions only — no Docker, no curl, no jq required.
#
# Usage:
#   ./scripts/benchmark/tests/run.sh
#
# Exits non-zero on the first failure; prints PASS/FAIL per assertion.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB="$SCRIPT_DIR/../benchlib.sh"
FIXTURES="$SCRIPT_DIR/fixtures"

source "$LIB"

FAILED=0

pass() { echo "PASS: $1"; }
fail() { echo "FAIL: $1"; echo "  expected: [$2]"; echo "  actual:   [$3]"; FAILED=1; }

assert_eq() {
    local desc="$1" expected="$2" actual="$3"
    if [[ "$expected" == "$actual" ]]; then
        pass "$desc"
    else
        fail "$desc" "$expected" "$actual"
    fi
}

# --- host CPU utilization ---------------------------------------------------

actual=$(cpu_utilization "$(cat "$FIXTURES/proc_stat_1.txt")" "$(cat "$FIXTURES/proc_stat_2.txt")")
assert_eq "cpu_utilization computes percent from two /proc/stat snapshots" "12.50" "$actual"

# --- host memory ------------------------------------------------------------

actual=$(meminfo_available_bytes "$(cat "$FIXTURES/meminfo.txt")")
assert_eq "meminfo_available_bytes parses MemAvailable" "4294967296" "$actual"

actual=$(meminfo_total_bytes "$(cat "$FIXTURES/meminfo.txt")")
assert_eq "meminfo_total_bytes parses MemTotal" "8589934592" "$actual"

# --- docker stats line parsing ---------------------------------------------

actual=$(parse_docker_stats_line "$(cat "$FIXTURES/docker_stats_line.txt")")
assert_eq "parse_docker_stats_line extracts name/cpu/mem from {{json}} line" "ow-api|0.10|1320702444" "$actual"

actual=$(parse_docker_stats_line "$(cat "$FIXTURES/docker_stats_line_no_mem.txt")")
assert_eq "parse_docker_stats_line handles absent MemUsage" "ow-api|0.10|0" "$actual"

actual=$(parse_docker_stats_line "$(cat "$FIXTURES/docker_stats_line_snake.txt")")
assert_eq "parse_docker_stats_line handles snake_case keys from older daemons" "ow-api|2.50|536870912" "$actual"

# --- CSV record -------------------------------------------------------------

actual=$(csv_record 1700000000 "ow-api" "0.10" "1320702444")
assert_eq "csv_record joins fields with commas" "1700000000,ow-api,0.10,1320702444" "$actual"

# --- aggregation ------------------------------------------------------------

actual=$(aggregate_cpu_mem "$(cat "$FIXTURES/samples.txt")")
assert_eq "aggregate_cpu_mem peak/mean over a window" "2.50|1.20|10485760|7340032" "$actual"

actual=$(aggregate_cpu_mem "")
assert_eq "aggregate_cpu_mem empty window is zeros" "0.00|0.00|0|0" "$actual"

# --- markdown emission ------------------------------------------------------

actual=$(md_peak_table "name" "$(cat "$FIXTURES/peaks.txt")")
expected="| name | peak_cpu | peak_mem |
| --- | --- | --- |
| ow-api | 0.10 | 10485760 |
| ow-web | 0.20 | 20971520 |"
assert_eq "md_peak_table emits a two-row peak table" "$expected" "$actual"

actual=$(md_runr_compare "$(cat "$FIXTURES/runr.txt")")
expected="| runtime | remote_type | mean_cpu | peak_cpu | mean_mem | peak_mem |
| --- | --- | --- | --- | --- | --- |
| runc | kasmvnc | 1.00 | 1.50 | 10485760 | 20971520 |
| runsc | kasmvnc | 1.20 | 1.80 | 11534336 | 23068672 |"
assert_eq "md_runr_compare emits the runC-vs-runsc table" "$expected" "$actual"

actual=$(md_delta_table "$(cat "$FIXTURES/delta.txt")")
expected="| metric | before | after | delta |
| --- | --- | --- | --- |
| cpu_percent | 0.00 | 5.00 | +5.00 |
| mem_bytes | 0 | 10485760 | +10485760 |"
assert_eq "md_delta_table emits the host before→after table" "$expected" "$actual"

if [[ "$FAILED" -eq 0 ]]; then
    echo "All benchlib tests passed"
else
    echo "Some benchlib tests failed" >&2
    exit 1
fi
