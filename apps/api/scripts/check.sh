#!/usr/bin/env bash

# Zero-warning gate for the Rust API. Must produce NO output on a clean
# codebase and exit non-zero if any check emits a warning or error.
#
# Checks: compile + test-driver compilation (default and docker feature sets),
# library check, and Clippy with warnings promoted to errors (all targets,
# all features) — the hard quality gate for lint violations.

set +e

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

FAILURES=""

check() {
    local name="$1"
    shift
    log "=================================================="
    log " Running: $name"
    log "=================================================="
    local out_file status_file
    out_file="$(mktemp)"
    status_file="$(mktemp)"
    {
        "$@" >"$out_file" 2>&1
        echo $? >"$status_file"
    }
    local output
    output="$(grep -iE "^(warning|error)" "$out_file")"
    local status
    status="$(cat "$status_file")"
    rm -f "$out_file" "$status_file"
    if [ -n "$output" ] || [ "$status" -ne 0 ]; then
        echo "$output"
        FAILURES="$FAILURES
- $name (exit $status)"
    fi
}

check "Default cargo test --no-run" cargo test --no-run
check "cargo test --features docker --no-run" cargo test --no-run --features docker
check "Library check (cargo check --lib)" cargo check --lib
check "Clippy (--all-targets --all-features -- -D warnings)" cargo clippy --all-targets --all-features -- -D warnings

if [ -n "$FAILURES" ]; then
    echo ""
    echo "FAILED:$FAILURES"
    exit 1
fi

log ""
log "=================================================="
log " All checks passed."
log "=================================================="
