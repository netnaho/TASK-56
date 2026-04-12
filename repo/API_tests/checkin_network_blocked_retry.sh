#!/usr/bin/env bash
# Phase 5 (hardened) — network rule is enforced identically on both the
# initial check-in and the retry endpoint.
#
# BEFORE the fix the retry endpoint returned HTTP 200 with
# `status:"network_blocked"` instead of HTTP 403.  This script verifies
# the unified policy:
#
#   1. Enable a restrictive CIDR list that will not match the test runner's IP.
#   2. Initial check-in with a fresh section → expect 403.
#   3. Disable the rule, create a clean check-in, capture the original_id.
#   4. Re-enable the rule.
#   5. Retry the captured check-in → expect 403 (not 200).
#   6. Verify the retry slot was NOT consumed (retry count still 0).
#   7. Restore admin_settings to their original state.
#
# Prerequisites: running backend at $BASE (default http://localhost:8000/api/v1)
# and `jq` in PATH.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for checkin_network_blocked_retry.sh"
fi

DEPT_CS="20000000-0000-0000-0000-000000000001"
# TEST-NET-3 (RFC 5737) — routable but never assigned; will not match any
# real test-runner IP.
RESTRICTIVE_CIDR='["203.0.113.0/24"]'
EMPTY_CIDR='[]'

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

# ── Auth ──────────────────────────────────────────────────────────────────────
ADMIN_TOKEN=$(login_as "admin@scholarly.local")
INSTRUCTOR_TOKEN=$(login_as "instructor@scholarly.local")
SUFFIX=$(date +%s | tail -c 4)

# ── Create a fresh course + section ──────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses" \
    -d "{\"code\":\"NBR${SUFFIX}\",\"title\":\"NetBlockRetry ${SUFFIX}\",\"department_id\":\"${DEPT_CS}\",\"credit_hours\":3,\"contact_hours\":3}")
expect_status "200" "$CODE" "create course"
COURSE_ID=$(jq -r '.id' "$BODY")

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/sections" \
    -d "{\"course_id\":\"${COURSE_ID}\",\"section_code\":\"01\",\"term\":\"fall\",\"year\":2026,\"capacity\":30}")
expect_status "200" "$CODE" "create section"
SECTION_ID=$(jq -r '.id' "$BODY")

# ── Step 1: Enable restrictive network rule ───────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X PUT "${BASE}/admin/config/checkin.allowed_client_cidrs" \
    -d "{\"value\":${RESTRICTIVE_CIDR}}")
expect_status "200" "$CODE" "enable restrictive network rule"

# ── Step 2: Initial check-in → must be 403 ───────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${INSTRUCTOR_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/checkins" \
    -d "{\"section_id\":\"${SECTION_ID}\",\"checkin_type\":\"qr_code\"}")
expect_status "403" "$CODE" "initial check-in blocked by network rule -> 403"
echo "  PASS: initial check-in correctly returned 403 when network rule active"

# ── Step 3: Disable rule, create clean check-in to get an original_id ────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X PUT "${BASE}/admin/config/checkin.allowed_client_cidrs" \
    -d "{\"value\":${EMPTY_CIDR}}")
expect_status "200" "$CODE" "disable network rule for clean check-in"

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${INSTRUCTOR_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/checkins" \
    -d "{\"section_id\":\"${SECTION_ID}\",\"checkin_type\":\"qr_code\"}")
expect_status "200" "$CODE" "clean check-in with rule disabled"
ORIGINAL_ID=$(jq -r '.view.id' "$BODY")
[ -n "$ORIGINAL_ID" ] && [ "$ORIGINAL_ID" != "null" ] || fail "no original_id from clean check-in"
echo "  original_id: ${ORIGINAL_ID}"

# ── Step 4: Re-enable the restrictive rule ────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X PUT "${BASE}/admin/config/checkin.allowed_client_cidrs" \
    -d "{\"value\":${RESTRICTIVE_CIDR}}")
expect_status "200" "$CODE" "re-enable restrictive network rule"

# ── Step 5: Retry the captured check-in → must be 403, NOT 200 ───────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${INSTRUCTOR_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/checkins/${ORIGINAL_ID}/retry" \
    -d '{"reason_code":"device_glitch"}')
if [ "$CODE" = "200" ]; then
    RETURNED_STATUS=$(jq -r '.status // empty' "$BODY" 2>/dev/null || true)
    fail "retry endpoint returned 200 (status=${RETURNED_STATUS}) instead of 403 — the old network-blocked bug is back"
fi
expect_status "403" "$CODE" "retry blocked by network rule -> 403 (unified policy)"
echo "  PASS: retry endpoint correctly returned 403 when network rule active"

# Response body on 403 must NOT contain network_blocked.
RESP_BODY=$(cat "$BODY")
if echo "${RESP_BODY}" | grep -q "network_blocked"; then
    fail "retry 403 response body must not contain 'network_blocked' — got: ${RESP_BODY}"
fi
echo "  PASS: 403 response body does not expose network_blocked status"

# ── Step 6: Verify retry slot was NOT consumed ────────────────────────────────
# Disable the rule and attempt an actual retry — it should still succeed (slot free).
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X PUT "${BASE}/admin/config/checkin.allowed_client_cidrs" \
    -d "{\"value\":${EMPTY_CIDR}}")
expect_status "200" "$CODE" "disable rule to test slot preservation"

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${INSTRUCTOR_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/checkins/${ORIGINAL_ID}/retry" \
    -d '{"reason_code":"device_glitch"}')
expect_status "200" "$CODE" "retry slot still free after blocked attempt -> 200"
RETRY_STATUS=$(jq -r '.status' "$BODY")
[ "${RETRY_STATUS}" = "retried" ] || fail "expected status=retried, got ${RETRY_STATUS}"
echo "  PASS: retry slot was not consumed by the blocked attempt (status=${RETRY_STATUS})"

# ── Step 7: Verify second retry is now rejected (slot consumed) ───────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${INSTRUCTOR_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/checkins/${ORIGINAL_ID}/retry" \
    -d '{"reason_code":"device_glitch"}')
expect_status "409" "$CODE" "second retry rejected after slot consumed -> 409"
echo "  PASS: second retry correctly rejected with 409 (slot consumed)"

echo ""
echo "ALL CHECKS PASSED: checkin_network_blocked_retry"
