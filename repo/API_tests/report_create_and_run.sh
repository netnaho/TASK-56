#!/usr/bin/env bash
# Report creation, manual trigger, run polling, and download authentication.
# Requires a running backend at BACKEND_URL (default http://localhost:8000).
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

echo "[report_create_and_run] testing report lifecycle"

# ── Login as admin ─────────────────────────────────────────────────────────
ADMIN_TOKEN=$(login_as "admin@scholarly.local")

BODY=$(mktemp)

# ── Create a report definition ────────────────────────────────────────────
echo "  -> POST /reports (create journal_catalog report)"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/reports" \
    -d '{
        "title": "Test Journal Catalog",
        "description": "Integration test report",
        "query_definition": {
            "report_type": "journal_catalog",
            "filters": {}
        },
        "default_format": "csv"
    }')
expect_status "200" "$CODE" "create report"

REPORT_ID=$(json_field "$BODY" id)
if [ -z "$REPORT_ID" ] || [ "$REPORT_ID" = "null" ]; then
    cat "$BODY"; fail "no report id in create response"
fi
pass "report created (id=${REPORT_ID})"

# ── List reports ───────────────────────────────────────────────────────────
echo "  -> GET /reports"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/reports")
expect_status "200" "$CODE" "list reports"
pass "list reports returns 200"

# ── Get single report ──────────────────────────────────────────────────────
echo "  -> GET /reports/${REPORT_ID}"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/reports/${REPORT_ID}")
expect_status "200" "$CODE" "get report by id"
TITLE=$(json_field "$BODY" title)
if [ "$TITLE" != "Test Journal Catalog" ]; then
    fail "wrong title: ${TITLE}"
fi
pass "get report by id (title OK)"

# ── Trigger a manual run ──────────────────────────────────────────────────
echo "  -> POST /reports/${REPORT_ID}/run"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/reports/${REPORT_ID}/run" \
    -d '{"format": "csv"}')
expect_status "202" "$CODE" "trigger report run"

RUN_ID=$(json_field "$BODY" id)
RUN_STATUS=$(json_field "$BODY" status)
if [ -z "$RUN_ID" ] || [ "$RUN_ID" = "null" ]; then
    cat "$BODY"; fail "no run id in trigger response"
fi
pass "run triggered (id=${RUN_ID}, status=${RUN_STATUS})"

# ── List runs ────────────────────────────────────────────────────────────
echo "  -> GET /reports/${REPORT_ID}/runs"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/reports/${REPORT_ID}/runs")
expect_status "200" "$CODE" "list runs"
pass "list runs returns 200"

# ── Poll until completed (up to 15 seconds) ───────────────────────────────
echo "  -> polling run ${RUN_ID} for completion..."
for i in $(seq 1 15); do
    sleep 1
    CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}" \
        "${BASE}/reports/runs/${RUN_ID}")
    expect_status "200" "$CODE" "get run poll $i"
    RUN_STATUS=$(json_field "$BODY" status)
    ARTIFACT=$(json_field "$BODY" artifact_available)
    if [ "$RUN_STATUS" = "completed" ]; then
        pass "run completed (attempt $i)"
        break
    fi
    if [ "$RUN_STATUS" = "failed" ]; then
        ERR=$(json_field "$BODY" error_message)
        fail "run failed: ${ERR}"
    fi
done
if [ "$RUN_STATUS" != "completed" ]; then
    fail "run did not complete within 15 seconds (status=$RUN_STATUS)"
fi

# ── Download artifact (requires auth) ────────────────────────────────────
echo "  -> GET /reports/runs/${RUN_ID}/download (authenticated)"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/reports/runs/${RUN_ID}/download")
expect_status "200" "$CODE" "download artifact with valid token"
SIZE=$(wc -c < "$BODY")
if [ "$SIZE" -lt 2 ]; then
    fail "artifact file is empty"
fi
pass "artifact downloaded (${SIZE} bytes)"

# ── Download without auth → 401 ───────────────────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    "${BASE}/reports/runs/${RUN_ID}/download")
expect_status "401" "$CODE" "download without auth returns 401"

rm -f "$BODY"
echo "[report_create_and_run] ALL PASS"
