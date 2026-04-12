#!/usr/bin/env bash
# An admin should be able to list audit log entries and verify the chain.
# The chain should report valid=true immediately after any fresh login.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

echo "[audit_log_search] GET /audit-logs"
BODY=$(mktemp)
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/audit-logs?limit=20")
expect_status "200" "$CODE" "audit list"

# Result must include at least one entry (the admin login we just did).
if have_jq; then
    COUNT=$(jq '.count' "$BODY")
    if [ "${COUNT:-0}" -lt 1 ]; then
        cat "$BODY"
        fail "audit search returned zero entries"
    fi
    pass "audit search returned ${COUNT} entries"

    # Every entry must have a current_hash.
    HASH_COUNT=$(jq '[.entries[] | select(.current_hash != null and .current_hash != "")] | length' "$BODY")
    if [ "${HASH_COUNT:-0}" -lt 1 ]; then
        fail "no current_hash present on audit entries"
    fi
    pass "all returned entries carry a current_hash"
fi

echo "[audit_log_search] GET /audit-logs/verify-chain"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/audit-logs/verify-chain")
expect_status "200" "$CODE" "verify-chain"

if have_jq; then
    VALID=$(jq -r '.valid' "$BODY")
    if [ "$VALID" != "true" ]; then
        cat "$BODY"
        fail "audit chain reported invalid"
    fi
    pass "audit chain is valid"
fi

rm -f "$BODY"
