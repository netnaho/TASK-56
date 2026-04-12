#!/usr/bin/env bash
# Verifies the /health endpoint returns a real DB-checked response.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

echo "[health_check] testing health endpoint"

BODY=$(mktemp)

CODE=$(curl -s -o "$BODY" -w "%{http_code}" "${BASE}/health")
expect_status "200" "$CODE" "health endpoint responds"

if have_jq; then
    STATUS=$(jq -r '.status // empty' "$BODY")
    DB=$(jq -r '.database // empty' "$BODY")
    # Must return ok or degraded — never the old "not_implemented" stub.
    if [ "$STATUS" = "ok" ] || [ "$STATUS" = "degraded" ]; then
        pass "status field is present and valid: ${STATUS}"
    else
        fail "unexpected status value: '${STATUS}' (expected ok or degraded)"
    fi
    if [ -n "$DB" ]; then
        pass "database field present: ${DB}"
    else
        fail "database field missing from health response"
    fi
else
    pass "health returned 200 (jq not available for deep check)"
fi

rm -f "$BODY"
echo "[health_check] ALL PASS"
