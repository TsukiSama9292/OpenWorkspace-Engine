#!/usr/bin/env bash
set -euo pipefail

# security:api — fuzz the 17 green endpoints of a RUNNING dev stack with
# Schemathesis (02-be-schemathesis).
#
# Pass 1 (admin session): schema-valid 200s (or declared 4xx), never a 5xx,
# under malformed / extreme generated input.
# Pass 2 (fuzz-user session): the low-privilege cookie must never get a 2xx
# from an `admin-gated` endpoint (403 or 404 are acceptable).
#
# Requires: the dev stack up (`pnpm run dev:nosudo`), Docker (the Schemathesis
# image is built on first run — no host Python / pipx), jq, python3, curl.
#
# Env overrides: SECURITY_API_URL (default http://localhost:3000),
# SECURITY_MAX_EXAMPLES (default 30), SECURITY_SEED (default 20260101),
# SECURITY_WORKERS (default 1), SECURITY_ADMIN_PASSWORD (default admin),
# SECURITY_FUZZ_USER (default fuzz-user), SECURITY_FUZZ_PASSWORD (default fuzz-fuzz).

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
API_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

API_URL="${SECURITY_API_URL:-http://localhost:3000}"
MAX_EXAMPLES="${SECURITY_MAX_EXAMPLES:-30}"
SEED="${SECURITY_SEED:-20260101}"
WORKERS="${SECURITY_WORKERS:-1}"
ADMIN_USER="admin"
ADMIN_PASSWORD="${SECURITY_ADMIN_PASSWORD:-admin}"
FUZZ_USER="${SECURITY_FUZZ_USER:-fuzz-user}"
FUZZ_PASSWORD="${SECURITY_FUZZ_PASSWORD:-fuzz-fuzz}"

SPEC="$API_DIR/security/openapi.json"
REPORT_DIR="$API_DIR/target/security-reports"
IMAGE="ow-schemathesis"
CHECKS="not_a_server_error,status_code_conformance,content_type_conformance,response_schema_conformance,negative_data_rejection"
PHASES="examples,coverage,fuzzing"

log() { echo "==> $*"; }
die() { echo "ERROR: $*" >&2; exit 1; }

# --- fail fast: the fuzzer needs a live stack ---------------------------------
if ! curl -sf -m 5 "$API_URL/health" >/dev/null 2>&1; then
    die "API not reachable at $API_URL/health. Start the dev stack first: pnpm run dev:nosudo"
fi
log "Dev API is up at $API_URL"

# --- fail fast: the safety config must still be in force ----------------------
# The unexpected-method disable is what stops the fuzzer from probing real
# mutating handlers (the 2026-08-06 admin deletion). If this line ever regresses
# or is removed, abort before fuzzing — the destructive mode must never silently
# come back.
if ! awk '
    /^\[/ { section = $0 }
    section == "[phases.coverage]" && $0 !~ /^[[:space:]]*#/ && /unexpected-methods[[:space:]]*=[[:space:]]*\[\]/ { found = 1 }
    END { exit found ? 0 : 1 }
' "$SCRIPT_DIR/schemathesis.toml"; then
    die "schemathesis.toml no longer disables unexpected-method probing (missing an active 'unexpected-methods = []' inside [phases.coverage]). Refusing to fuzz with a destructive harness."
fi
log "Safety config in force: unexpected-method probing disabled"

# --- build the Schemathesis image on first use --------------------------------
if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
    log "Building $IMAGE image (one-time; the official :stable image bundles a broken tracecov plugin)..."
    docker build -t "$IMAGE" -f "$SCRIPT_DIR/schemathesis.Dockerfile" "$SCRIPT_DIR"
fi

# --- regenerate the exported spec from the running code -----------------------
log "Regenerating the OpenAPI spec ($SPEC)..."
( cd "$API_DIR" && cargo run -q --bin export_openapi )
jq -e '.paths | length == 17' "$SPEC" >/dev/null \
    || die "regenerated spec is invalid or does not cover the 17 safe endpoints"
log "Spec covers 17 paths (validated before fuzzing)"

# --- helpers ------------------------------------------------------------------
login_cookie() {
    local user="$1" pass="$2" jar body
    jar="$(mktemp)"
    body="$(jq -nc --arg u "$user" --arg p "$pass" '{username: $u, password: $p}')"
    curl -sf -m 10 -c "$jar" -H 'Content-Type: application/json' \
        -d "$body" "$API_URL/api/auth/login" >/dev/null
    local token
    token="$(awk -F '\t' '$6 == "ow_token" { print $7 }' "$jar" | head -1)"
    rm -f "$jar"
    [ -n "$token" ] || die "login as '$user' failed — no session cookie issued"
    printf '%s' "$token"
}

run_pass() {
    local label="$1" cookie="$2" enforce_rbac="$3"
    local out_dir="$REPORT_DIR/$label"
    mkdir -p "$out_dir"
    log "Pass $label: seed=$SEED max-examples=$MAX_EXAMPLES workers=$WORKERS"
    local rc=0
    docker run --rm \
        -v "$SPEC:/spec/openapi.json:ro" \
        -v "$SCRIPT_DIR/schemathesis_pre_run.py:/harness/schemathesis_pre_run.py:ro" \
        -v "$SCRIPT_DIR/schemathesis.toml:/harness/schemathesis.toml:ro" \
        -v "$out_dir:/work" \
        -w /work \
        --network host \
        -e PYTHONPATH=/harness \
        -e OW_ENFORCE_RBAC="$enforce_rbac" \
        "$IMAGE" schemathesis --config-file /harness/schemathesis.toml run /spec/openapi.json \
            -u "$API_URL" \
            -H "Cookie: ow_token=$cookie" \
            -n "$MAX_EXAMPLES" \
            --seed "$SEED" \
            -w "$WORKERS" \
            -c "$CHECKS" \
            --phases "$PHASES" \
            --no-color \
            || rc=$?
    if [ "$rc" -ne 0 ]; then
        log "Pass $label FAILED (exit $rc); details: $out_dir"
        return "$rc"
    fi
    log "Pass $label passed"
}

# --- integrity snapshot (admin-protection 03-int) ------------------------------
# The canary for state damage: the admin account must survive the run intact,
# and no template/instance rows may be created or destroyed. The fuzz-user
# self-provisioning creates one user and nothing else, so it never disturbs the
# snapshot.
admin_is_admin() {
    curl -sf -m 10 -b "ow_token=$ADMIN_COOKIE" "$API_URL/api/users" \
        | jq -e --arg u "$ADMIN_USER" '.users[] | select(.username == $u) | .is_admin' >/dev/null
}
count_templates() {
    local n
    n="$(curl -sf -m 10 -b "ow_token=$ADMIN_COOKIE" "$API_URL/api/templates" 2>/dev/null | jq -r '.templates | length' 2>/dev/null)" || n="N/A"
    echo "${n:-N/A}"
}
count_instances() {
    local n
    n="$(curl -sf -m 10 -b "ow_token=$ADMIN_COOKIE" "$API_URL/api/instances" 2>/dev/null | jq -r '.instances | length' 2>/dev/null)" || n="N/A"
    echo "${n:-N/A}"
}

check_integrity() {
    local failures=0
    if ! admin_is_admin; then
        log "INTEGRITY FAILURE: the $ADMIN_USER user is missing or no longer an Admin-group member"
        failures=1
    fi
    local ct ci
    ct="$(count_templates)"
    ci="$(count_instances)"
    if [ "$ct" = "N/A" ] || [ "$ci" = "N/A" ]; then
        log "INTEGRITY FAILURE: could not read template/instance state (admin session or API failed)"
        failures=1
    else
        if [ "$ct" != "$TEMPLATE_COUNT_BEFORE" ]; then
            log "INTEGRITY FAILURE: workspace_templates changed ($TEMPLATE_COUNT_BEFORE -> $ct)"
            failures=1
        fi
        if [ "$ci" != "$INSTANCE_COUNT_BEFORE" ]; then
            log "INTEGRITY FAILURE: workspace_instances changed ($INSTANCE_COUNT_BEFORE -> $ci)"
            failures=1
        fi
    fi
    return "$failures"
}

# --- provisioning --------------------------------------------------------------
log "Provisioning: logging in as $ADMIN_USER..."
ADMIN_COOKIE="$(login_cookie "$ADMIN_USER" "$ADMIN_PASSWORD")"
log "Admin session obtained"

TEMPLATE_COUNT_BEFORE="$(count_templates)"
INSTANCE_COUNT_BEFORE="$(count_instances)"
if admin_is_admin; then
    log "Integrity snapshot: admin intact, templates=$TEMPLATE_COUNT_BEFORE instances=$INSTANCE_COUNT_BEFORE"
else
    die "pre-run integrity check failed — the $ADMIN_USER user is missing or not an admin; refusing to fuzz"
fi

if ! curl -sf -m 10 -b "ow_token=$ADMIN_COOKIE" "$API_URL/api/users" \
    | jq -e --arg u "$FUZZ_USER" '.users[] | select(.username == $u)' >/dev/null 2>&1; then
    log "Creating $FUZZ_USER (User system group, fixed dev password)..."
    body="$(jq -nc --arg u "$FUZZ_USER" --arg p "$FUZZ_PASSWORD" '{username: $u, password: $p}')"
    curl -sf -m 10 -b "ow_token=$ADMIN_COOKIE" -H 'Content-Type: application/json' \
        -d "$body" "$API_URL/api/users" >/dev/null
else
    log "$FUZZ_USER already exists — keeping it"
fi
log "Logging in as $FUZZ_USER..."
FUZZ_COOKIE="$(login_cookie "$FUZZ_USER" "$FUZZ_PASSWORD")"

# --- dual-pass fuzzing ---------------------------------------------------------
run_pass "1-admin" "$ADMIN_COOKIE" "0"; PASS1=$?
run_pass "2-fuzz-user" "$FUZZ_COOKIE" "1"; PASS2=$?

# --- post-run integrity check --------------------------------------------------
# Always runs, even if a pass failed: the state-damage canary must not be
# skipped by `set -e` the moment a pass errors.
if check_integrity; then
    log "Post-run integrity OK: admin intact, templates=$TEMPLATE_COUNT_BEFORE instances=$INSTANCE_COUNT_BEFORE"
else
    die "post-run integrity check failed — the fuzzer mutated state it must not touch"
fi

if [ "$PASS1" -ne 0 ] || [ "$PASS2" -ne 0 ]; then
    die "a fuzzing pass failed — see reports in $REPORT_DIR"
fi

log "Both passes passed (seed=$SEED). Reports in $REPORT_DIR"
