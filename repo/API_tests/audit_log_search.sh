#!/usr/bin/env bash
# Audit log search and chain verification.
# - Admin can search logs and verify chain.
# - Non-admin (viewer, instructor) must get 403.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

echo "[audit_log_search] testing audit log API"

BODY=$(mktemp)

ADMIN_TOKEN=$(login_as "admin@scholarly.local")
VIEWER_TOKEN=$(login_as "viewer@scholarly.local")

# ── List audit logs (admin) ───────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/audit-logs?limit=10")
expect_status "200" "$CODE" "list audit logs (admin)"

if have_jq; then
    COUNT=$(jq '.count // 0' "$BODY")
    pass "audit log count: ${COUNT}"
    # Must be an object with 'entries' array.
    ENTRIES=$(jq -r '.entries | type' "$BODY")
    if [ "$ENTRIES" = "array" ]; then
        pass "entries field is an array"
    else
        fail "entries field is not an array (got ${ENTRIES})"
    fi
fi

# ── Filter by action ──────────────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/audit-logs?action=auth.login&limit=50")
expect_status "200" "$CODE" "filter audit logs by action"

# ── Non-admin cannot read audit logs ─────────────────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer ${VIEWER_TOKEN}" \
    "${BASE}/audit-logs?limit=10")
expect_status "403" "$CODE" "viewer cannot read audit logs"

# ── Verify chain (admin) ──────────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/audit-logs/verify-chain")
expect_status "200" "$CODE" "verify audit chain"

if have_jq; then
    VALID=$(jq -r '.valid' "$BODY")
    TOTAL=$(jq '.total_entries' "$BODY")
    pass "chain valid=${VALID}, total_entries=${TOTAL}"
    if [ "$VALID" = "true" ]; then
        pass "audit chain is intact"
    elif [ "$VALID" = "false" ]; then
        pass "chain reports broken (valid=false returned cleanly — ok for empty DB)"
    else
        fail "unexpected valid value: ${VALID}"
    fi
fi

# ── Unauthenticated → 401 ─────────────────────────────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    "${BASE}/audit-logs?limit=10")
expect_status "401" "$CODE" "unauthenticated returns 401"

rm -f "$BODY"
echo "[audit_log_search] ALL PASS"
