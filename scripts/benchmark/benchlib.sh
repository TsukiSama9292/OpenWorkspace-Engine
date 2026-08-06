#!/usr/bin/env bash
# benchlib.sh — pure-function measurement library for the production benchmark.
#
# Every function reads from arguments/stdin and writes to stdout; no side
# effects, no filesystem writes, no docker/curl/jq. Sourced by benchmark-prod.sh
# and by scripts/benchmark/tests/run.sh.
#
# Value formats used across the library:
#   - CPU utilization: percent, 2 decimals (e.g. "12.50")
#   - memory: integer bytes (e.g. "1320702444")
#   - docker-stats row: "name|cpu|mem_bytes"
#   - sample rows: "cpu mem" per line, one per second
#   - pipeline rows: pipe-separated, first line is the header

# ---------------------------------------------------------------------------
# Host CPU — /proc/stat aggregate line
# ---------------------------------------------------------------------------

# cpu_stat_ticks TEXT: extract the aggregate "cpu " line and print "idle total"
# (idle includes iowait; total is the sum of all tick counters).
cpu_stat_ticks() {
    printf '%s\n' "$1" | awk '/^cpu / {
        idle = $5 + $6;
        total = $2 + $3 + $4 + $5 + $6 + $7 + $8 + $9 + $10 + $11;
        printf "%d %d\n", idle, total;
        exit
    }'
}

# cpu_utilization PREV_TEXT CURR_TEXT: percent busy between two /proc/stat
# snapshots. Prints "12.50" (2 decimals). Guards against a zero-time delta.
cpu_utilization() {
    local prev="$1" cur="$2"
    local p_idle p_total c_idle c_total
    read -r p_idle p_total <<<"$(cpu_stat_ticks "$prev")"
    read -r c_idle c_total <<<"$(cpu_stat_ticks "$cur")"
    local d_total=$((c_total - p_total))
    local d_idle=$((c_idle - p_idle))
    if (( d_total <= 0 )); then
        echo "0.00"
        return
    fi
    local d_busy=$((d_total - d_idle))
    awk -v busy="$d_busy" -v total="$d_total" 'BEGIN { printf "%.2f\n", busy * 100 / total }'
}

# ---------------------------------------------------------------------------
# Host memory — /proc/meminfo
# ---------------------------------------------------------------------------

meminfo_available_bytes() {
    printf '%s\n' "$1" | awk '/^MemAvailable:/ { printf "%d\n", $2 * 1024; exit }'
}

meminfo_total_bytes() {
    printf '%s\n' "$1" | awk '/^MemTotal:/ { printf "%d\n", $2 * 1024; exit }'
}

# ---------------------------------------------------------------------------
# docker stats — one "{{json .}}" line
# ---------------------------------------------------------------------------

# human_to_bytes "1.23GiB": human size to integer bytes. Accepts B/KB/MB/GB
# (decimal) and KiB/MiB/GiB/TiB (binary). "0B" -> 0.
human_to_bytes() {
    local human="$1"
    local num unit mult
    num=$(printf '%s' "$human" | sed 's/[A-Za-z]//g')
    unit=$(printf '%s' "$human" | sed 's/[0-9.]//g')
    case "$unit" in
        B) mult=1 ;;
        KB) mult=1000 ;;
        MB) mult=1000000 ;;
        GB) mult=1000000000 ;;
        KiB) mult=1024 ;;
        MiB) mult=1048576 ;;
        GiB) mult=1073741824 ;;
        TiB) mult=1099511627776 ;;
        *) mult=1 ;;
    esac
    awk -v n="$num" -v m="$mult" 'BEGIN { printf "%.0f\n", n * m }'
}

# parse_docker_stats_line LINE: parse one `docker stats --format '{{json .}}'`
# line into "name|cpu|mem_bytes". MemUsage is "used / limit"; only the used
# part is parsed. Missing MemUsage -> 0. Accepts both key styles modern docker
# emits (PascalCase: "Name"/"CPUPerc"/"MemUsage") and older daemons' snake_case
# ("name"/"cpu_perc"/"mem_usage").
parse_docker_stats_line() {
    local line="$1"
    local name cpu mem_usage mem_human mem_bytes
    name=$(printf '%s' "$line" | sed -n 's/.*"\(Name\|name\)":"\([^"]*\)".*/\2/p')
    cpu=$(printf '%s' "$line" | sed -n 's/.*"\(CPUPerc\|cpu_perc\)":"\([^"]*\)%".*/\2/p')
    mem_usage=$(printf '%s' "$line" | sed -n 's/.*"\(MemUsage\|mem_usage\)":"\([^"]*\)".*/\2/p')
    if [[ -n "$mem_usage" ]]; then
        mem_human=${mem_usage%% *}
        mem_bytes=$(human_to_bytes "$mem_human")
    else
        mem_bytes=0
    fi
    printf '%s|%s|%s\n' "$name" "$cpu" "$mem_bytes"
}

# ---------------------------------------------------------------------------
# CSV records
# ---------------------------------------------------------------------------

csv_record() {
    local IFS=,
    echo "$*"
}

# ---------------------------------------------------------------------------
# Aggregation
# ---------------------------------------------------------------------------

# aggregate_cpu_mem SAMPLES_TEXT: rows "cpu mem" per line (one per second).
# Prints "peak_cpu|mean_cpu|peak_mem|mean_mem" — cpu 2 decimals, mem integers.
# Empty input -> "0.00|0.00|0|0".
aggregate_cpu_mem() {
    awk -v s="$1" '
    BEGIN {
        n = split(s, lines, "\n");
        if (n == 0 || (n == 1 && lines[1] == "")) { print "0.00|0.00|0|0"; exit }
        for (i = 1; i <= n; i++) {
            if (lines[i] == "") continue
            split(lines[i], f, " ");
            cpu_sum += f[1]; mem_sum += f[2];
            if (cnt == 0 || f[1] > max_cpu) max_cpu = f[1];
            if (cnt == 0 || f[2] > max_mem) max_mem = f[2];
            cnt++
        }
        if (cnt == 0) { print "0.00|0.00|0|0"; exit }
        printf "%.2f|%.2f|%.0f|%.0f\n", max_cpu, cpu_sum / cnt, max_mem, mem_sum / cnt
    }'
}

# ---------------------------------------------------------------------------
# Markdown emission — pipe-separated rows, first line is the header
# ---------------------------------------------------------------------------

# md_peak_table LABEL ROWS: name|cpu|mem -> platform/per-instance peak table.
md_peak_table() {
    local label="$1" rows="$2"
    printf '| %s | peak_cpu | peak_mem |\n' "$label"
    printf '| --- | --- | --- |\n'
    printf '%s\n' "$rows" | while IFS='|' read -r name cpu mem; do
        printf '| %s | %s | %s |\n' "$name" "$cpu" "$mem"
    done
}

# md_runr_compare ROWS: runtime|remote_type|mean_cpu|peak_cpu|mean_mem|peak_mem
md_runr_compare() {
    local rows="$1"
    printf '| runtime | remote_type | mean_cpu | peak_cpu | mean_mem | peak_mem |\n'
    printf '| --- | --- | --- | --- | --- | --- |\n'
    printf '%s\n' "$rows" | while IFS='|' read -r rt type mc pc mm pm; do
        printf '| %s | %s | %s | %s | %s | %s |\n' "$rt" "$type" "$mc" "$pc" "$mm" "$pm"
    done
}

# md_delta_table ROWS: metric|before|after|delta
md_delta_table() {
    local rows="$1"
    printf '| metric | before | after | delta |\n'
    printf '| --- | --- | --- | --- |\n'
    printf '%s\n' "$rows" | while IFS='|' read -r metric before after delta; do
        printf '| %s | %s | %s | %s |\n' "$metric" "$before" "$after" "$delta"
    done
}
