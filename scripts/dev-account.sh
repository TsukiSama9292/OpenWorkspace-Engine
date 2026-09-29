#!/usr/bin/env bash
# Show the default dev accounts (seeded at API startup, see
# `apps/api/src/dev_seed.rs`).
#
#   admin — seeded on every boot (`ADMIN_PASSWORD`, default "admin")
#   user  — seeded only when `OW_DEV_SEED=1` (set by `pnpm run dev:api`),
#           password from `DEV_USER_PASSWORD` (default "user")
#
# When the dev API is reachable, each account is verified with a real login
# and the result is reported. Never run against production: these are
# well-known dev-only credentials.
set -euo pipefail

ADMIN_USER="admin"
ADMIN_PASSWORD="${ADMIN_PASSWORD:-admin}"
DEV_USER="user"
DEV_USER_PASSWORD="${DEV_USER_PASSWORD:-user}"
API_BASE="${API_BASE_URL:-http://localhost:3000}"

printf 'Dev accounts (dev stack only, never production):\n'
printf '  %-6s / %s\n' "$ADMIN_USER" "$ADMIN_PASSWORD"
printf '  %-6s / %s\n' "$DEV_USER" "$DEV_USER_PASSWORD"

if ! curl -fsS -o /dev/null --max-time 5 "$API_BASE/health" 2>/dev/null; then
    printf '\nAPI is not reachable at %s — start the dev stack first (pnpm run dev).\n' "$API_BASE"
    exit 0
fi

check_login() {
    local username="$1" password="$2"
    local code
    code="$(curl -s -o /dev/null -w '%{http_code}' --max-time 10 \
        -X POST "$API_BASE/api/auth/login" \
        -H 'Content-Type: application/json' \
        -d "$(printf '{"username":%s,"password":%s}' \
            "$(printf '%s' "$username" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')" \
            "$(printf '%s' "$password" | python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))')")" \
        || true)"
    if [ "$code" = "200" ]; then
        printf '  [ok] %s login works\n' "$username"
    else
        printf '  [missing] %s login failed (HTTP %s)' "$username" "$code"
        if [ "$username" = "$DEV_USER" ]; then
            printf ' — restart the dev API with OW_DEV_SEED=1 (pnpm run dev:api)\n'
        else
            printf '\n'
        fi
    fi
}

printf '\nVerifying against %s:\n' "$API_BASE"
check_login "$ADMIN_USER" "$ADMIN_PASSWORD"
check_login "$DEV_USER" "$DEV_USER_PASSWORD"
