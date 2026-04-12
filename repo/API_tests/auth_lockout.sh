#!/usr/bin/env bash
# After 5 failed attempts, the 6th must be rejected with 429 (AccountLocked).
# Creates a dedicated canary user so that no shared role account is locked out,
# which would cascade failures into the many other suites that need those accounts.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

# Create a throwaway user solely for this lockout test.
TS=$(date +%s)
CANARY_EMAIL="lockout_canary_${TS}@scholarly.local"
CANARY_PASS="CanaryPass12!"

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/users" \
    -d "{\"email\":\"${CANARY_EMAIL}\",\"display_name\":\"Lockout Canary ${TS}\",\"password\":\"${CANARY_PASS}\",\"roles\":[\"viewer\"]}")
expect_status "200" "$CODE" "create lockout canary user"
pass "canary user created: ${CANARY_EMAIL}"

echo "[auth_lockout] Triggering 5 failed logins for ${CANARY_EMAIL}"
for i in 1 2 3 4 5; do
    CODE=$(curl -s -o /dev/null -w "%{http_code}" \
        -H "Content-Type: application/json" \
        -X POST "${BASE}/auth/login" \
        -d "{\"email\":\"${CANARY_EMAIL}\",\"password\":\"WrongBadBad${i}\"}")
    if [ "$CODE" != "401" ]; then
        fail "attempt #${i} expected 401, got ${CODE}"
    fi
    pass "attempt #${i} -> 401"
done

echo "[auth_lockout] 6th attempt (even with correct password) must be locked"
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/auth/login" \
    -d "{\"email\":\"${CANARY_EMAIL}\",\"password\":\"${CANARY_PASS}\"}")
expect_status "429" "$CODE" "locked after 5 failures"
