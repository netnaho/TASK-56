#!/usr/bin/env bash
# Successful login with the seeded admin account returns a token.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

echo "[auth_login_success] POST ${BASE}/auth/login (admin)"
BODY=$(mktemp)
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/auth/login" \
    -d "{\"email\":\"admin@scholarly.local\",\"password\":\"${DEFAULT_PASSWORD}\"}")
expect_status "200" "$CODE" "admin login"

TOKEN=$(json_field "$BODY" token)
if [ -z "$TOKEN" ] || [ "$TOKEN" = "null" ]; then
    cat "$BODY"
    fail "no token in login response"
fi
pass "token present (len=${#TOKEN})"

# GET /auth/me with the token should succeed.
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer ${TOKEN}" \
    "${BASE}/auth/me")
expect_status "200" "$CODE" "authenticated /me"

rm -f "$BODY"
