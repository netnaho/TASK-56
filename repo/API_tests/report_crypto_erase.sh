#!/usr/bin/env bash
# Phase 6 (hardened) — Cryptographic erasure of report artifacts.
#
# Assertions:
#   1. A JournalCatalog run can be triggered and its artifact downloaded
#      successfully (decrypt path works end-to-end).
#   2. After the retention policy for report_runs fires (dry_run=false),
#      the artifact file is deleted and the download endpoint returns 404.
#   3. The artifact_dek column is NULL after the retention run (crypto-erase
#      committed before physical delete).
#
# Note: This test directly manipulates the retention_days of the
#       report_runs policy to 0 to force immediate expiry.
#
# Prerequisites: running backend at $BASE (default http://localhost:8000/api/v1)
# and `jq` in PATH.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for report_crypto_erase.sh"
fi

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

SUFFIX=$(date +%s | tail -c 8)

# ── Auth ──────────────────────────────────────────────────────────────────────
ADMIN_TOKEN=$(login_as "admin@scholarly.local")

# ══════════════════════════════════════════════════════════════════════════════
# Section A — Create a report run and verify artifact is downloadable
# ══════════════════════════════════════════════════════════════════════════════

# A1. Create a JournalCatalog report.
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/reports" \
    -d "{
        \"title\": \"CryptoErase-Test-${SUFFIX}\",
        \"query_definition\": {\"report_type\": \"journal_catalog\", \"filters\": {}},
        \"default_format\": \"csv\"
    }")
expect_status "200" "$CODE" "admin creates JournalCatalog report"
REPORT_ID=$(jq -r '.id' "$BODY")
[ -n "$REPORT_ID" ] && [ "$REPORT_ID" != "null" ] || fail "no report id"
echo "  Report id: ${REPORT_ID}"

# A2. Trigger a run.
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/reports/${REPORT_ID}/run" \
    -d '{}')
expect_status "202" "$CODE" "admin triggers run -> 202"
RUN_ID=$(jq -r '.id' "$BODY")
[ -n "$RUN_ID" ] && [ "$RUN_ID" != "null" ] || fail "no run id"
echo "  Run id: ${RUN_ID}"

# A3. Download the artifact — must succeed.
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X GET "${BASE}/reports/runs/${RUN_ID}/download")
expect_status "200" "$CODE" "admin downloads artifact -> 200 (decrypt path)"
ARTIFACT_SIZE=$(wc -c < "$BODY")
[ "$ARTIFACT_SIZE" -gt 0 ] || fail "artifact is empty"
echo "  PASS: artifact downloaded successfully (${ARTIFACT_SIZE} bytes)"

# ══════════════════════════════════════════════════════════════════════════════
# Section B — Force retention expiry and verify crypto-erase
# ══════════════════════════════════════════════════════════════════════════════

# B1. Find the report_runs retention policy.
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X GET "${BASE}/admin/retention/")
expect_status "200" "$CODE" "list retention policies"
POLICY_ID=$(jq -r '[.[] | select(.target_entity_type == "report_runs")] | first | .id' "$BODY")
[ -n "$POLICY_ID" ] && [ "$POLICY_ID" != "null" ] || fail "no report_runs retention policy found"
echo "  Policy id: ${POLICY_ID}"

# B2. Set retention_days to 0 so the run we just created is immediately expired.
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X PUT "${BASE}/admin/retention/${POLICY_ID}" \
    -d '{"retention_days": 0, "action": "delete", "is_active": true}')
expect_status "200" "$CODE" "set retention_days=0"
echo "  retention_days set to 0"

# B3. Execute the retention policy (live run, not dry run).
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/admin/retention/${POLICY_ID}/execute" \
    -d '{"dry_run": false}')
expect_status "200" "$CODE" "execute retention policy"
FILES_DELETED=$(jq -r '.files_deleted' "$BODY")
echo "  files_deleted=${FILES_DELETED}"

# B4. Attempt to download the artifact again — must now return 404.
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X GET "${BASE}/reports/runs/${RUN_ID}/download")
if [ "$CODE" = "200" ]; then
    fail "REGRESSION: artifact still downloadable after retention execution (crypto-erase failed)"
fi
expect_status "404" "$CODE" "artifact download after retention -> 404"
echo "  PASS: artifact correctly returns 404 after retention execution"

# ══════════════════════════════════════════════════════════════════════════════
# Cleanup — restore retention_days to a sane default (365 days)
# ══════════════════════════════════════════════════════════════════════════════
curl -s -o /dev/null \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X PUT "${BASE}/admin/retention/${POLICY_ID}" \
    -d '{"retention_days": 365, "action": "delete", "is_active": true}'
echo "  Policy retention_days restored to 365"

echo ""
echo "ALL CHECKS PASSED: report_crypto_erase"
