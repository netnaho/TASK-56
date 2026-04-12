#!/usr/bin/env bash
# Tests for GET /api/v1/audit-logs/export.csv
#
# Verifies:
#   1. Admin receives 200 with correct Content-Type and a non-empty CSV.
#   2. CSV has the expected 10-column header row.
#   3. The export itself is recorded in the audit log (audit.export action).
#   4. Non-admin caller (librarian, which lacks AuditExport) gets 403.
#   5. Unauthenticated request gets 401.
#   6. Filter parameters are accepted and narrow the result (smoke-test only).
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

echo "[audit_log_export] testing GET /audit-logs/export.csv"

BODY=$(mktemp)
HEADERS=$(mktemp)

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

# ── 1. Admin gets 200 with correct Content-Type ───────────────────────────
CODE=$(curl -s -D "$HEADERS" -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/audit-logs/export.csv")
expect_status "200" "$CODE" "admin export returns 200"

# Content-Type must indicate CSV.
if grep -qi "text/csv" "$HEADERS"; then
    pass "Content-Type contains text/csv"
else
    fail "Content-Type missing text/csv (got: $(grep -i content-type "$HEADERS" || echo 'none'))"
fi

# Content-Disposition must trigger a download.
if grep -qi "attachment" "$HEADERS"; then
    pass "Content-Disposition is attachment"
else
    fail "Content-Disposition is not attachment"
fi

# Filename must look like audit_logs_<date>.csv
if grep -qi "audit_logs_" "$HEADERS"; then
    pass "Content-Disposition filename contains audit_logs_"
else
    fail "Content-Disposition filename does not contain 'audit_logs_'"
fi

# ── 2. CSV body is non-empty and has the correct header row ──────────────
CSV_BODY=$(cat "$BODY")
if [ -z "$CSV_BODY" ]; then
    fail "export body is empty"
fi

HEADER_LINE=$(head -n1 "$BODY")
EXPECTED_HEADER="sequence_number,id,actor_id,actor_email,action,target_entity_type,target_entity_id,ip_address,created_at,current_hash"
if [ "$HEADER_LINE" = "$EXPECTED_HEADER" ]; then
    pass "CSV header row matches expected columns"
else
    fail "CSV header mismatch. Expected: $EXPECTED_HEADER — Got: $HEADER_LINE"
fi

# Count data rows (at least the login event that just happened).
DATA_ROWS=$(echo "$CSV_BODY" | tail -n +2 | grep -c '.' || true)
if [ "${DATA_ROWS:-0}" -ge 1 ]; then
    pass "CSV contains ${DATA_ROWS} data row(s)"
else
    fail "CSV contains no data rows after header"
fi

# ── 3. The export itself was recorded as audit.export ────────────────────
AUDIT_BODY=$(mktemp)
CODE=$(curl -s -o "$AUDIT_BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/audit-logs?action=audit.export&limit=5")
expect_status "200" "$CODE" "audit log search for audit.export action"

if have_jq; then
    EXPORT_COUNT=$(jq '.count' "$AUDIT_BODY")
    if [ "${EXPORT_COUNT:-0}" -ge 1 ]; then
        pass "audit.export event recorded in audit log (${EXPORT_COUNT} entries)"
    else
        fail "no audit.export event found in audit log"
    fi
fi
rm -f "$AUDIT_BODY"

# ── 4. Non-admin (librarian) gets 403 — lacks AuditExport capability ─────
LIBRARIAN_TOKEN=$(login_as "librarian@scholarly.local")
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer ${LIBRARIAN_TOKEN}" \
    "${BASE}/audit-logs/export.csv")
expect_status "403" "$CODE" "librarian is forbidden from export.csv"

# ── 5. Unauthenticated request gets 401 ──────────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    "${BASE}/audit-logs/export.csv")
expect_status "401" "$CODE" "unauthenticated request returns 401"

# ── 6. Filter parameters are forwarded (smoke test) ──────────────────────
# Filter to an action that exists (auth.login.success); result must still be 200
# and have at least one data row.
FILTER_BODY=$(mktemp)
CODE=$(curl -s -o "$FILTER_BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/audit-logs/export.csv?action=auth.login.success&limit=10")
expect_status "200" "$CODE" "export.csv with action filter returns 200"

if [ -s "$FILTER_BODY" ]; then
    FILTER_ROWS=$(tail -n +2 "$FILTER_BODY" | grep -c '.' || true)
    if [ "${FILTER_ROWS:-0}" -ge 1 ]; then
        pass "filtered export contains ${FILTER_ROWS} row(s) for auth.login.success"
    else
        fail "filtered export returned no rows for auth.login.success (expected at least 1)"
    fi
fi
rm -f "$FILTER_BODY"

# ── 7. Invalid UUID in actor_id yields 422 ───────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/audit-logs/export.csv?actor_id=not-a-uuid")
expect_status "422" "$CODE" "invalid actor_id UUID yields 422 validation error"

rm -f "$BODY" "$HEADERS"
echo "[audit_log_export] ALL PASS"
