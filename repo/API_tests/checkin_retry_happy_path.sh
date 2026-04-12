#!/usr/bin/env bash
# Phase 5 — one reasoned retry succeeds; a second retry is rejected
# because `checkin.max_retry_count` defaults to 1.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for checkin_retry_happy_path.sh"
fi

DEPT_CS="20000000-0000-0000-0000-000000000001"

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

# ── Admin creates course + section ────────────────────────────────────────
ADMIN_TOKEN=$(login_as "admin@scholarly.local")
SUFFIX=$(date +%s | tail -c 5)
COURSE_CODE="CRH${SUFFIX}"

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses" \
    -d "{\"code\":\"${COURSE_CODE}\",\"title\":\"Retry HP\",\"department_id\":\"${DEPT_CS}\",\"credit_hours\":3,\"contact_hours\":3}")
expect_status "200" "$CODE" "create course"
COURSE_ID=$(jq -r '.id' "$BODY")

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/sections" \
    -d "{\"course_id\":\"${COURSE_ID}\",\"section_code\":\"01\",\"term\":\"fall\",\"year\":2026,\"capacity\":30}")
expect_status "200" "$CODE" "create section"
SECTION_ID=$(jq -r '.id' "$BODY")

# ── Instructor first tap → capture original_id ───────────────────────────
INSTRUCTOR_TOKEN=$(login_as "instructor@scholarly.local")
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${INSTRUCTOR_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/checkins" \
    -d "{\"section_id\":\"${SECTION_ID}\",\"checkin_type\":\"qr_code\"}")
expect_status "200" "$CODE" "first tap -> 200"
ORIGINAL_ID=$(jq -r '.view.id' "$BODY")
[ -n "$ORIGINAL_ID" ] && [ "$ORIGINAL_ID" != "null" ] || fail "no original id"

# ── Duplicate → 409 ───────────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${INSTRUCTOR_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/checkins" \
    -d "{\"section_id\":\"${SECTION_ID}\",\"checkin_type\":\"qr_code\"}")
expect_status "409" "$CODE" "second tap -> 409 duplicate"

# ── First retry → 200 ─────────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${INSTRUCTOR_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/checkins/${ORIGINAL_ID}/retry" \
    -d '{"reason_code":"device_glitch"}')
expect_status "200" "$CODE" "first retry -> 200"

STATUS=$(jq -r '.status' "$BODY")
SEQ=$(jq -r '.view.retry_sequence' "$BODY")
REASON=$(jq -r '.view.retry_reason' "$BODY")
[ "$STATUS" = "retried" ] || fail "expected status=retried, got $STATUS"
[ "$SEQ" = "1" ] || fail "expected retry_sequence=1, got $SEQ"
[ "$REASON" = "device_glitch" ] || fail "expected retry_reason=device_glitch, got $REASON"
pass "retry persisted with reason code"

# ── Second retry → 409 (max retry exceeded) ───────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${INSTRUCTOR_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/checkins/${ORIGINAL_ID}/retry" \
    -d '{"reason_code":"device_glitch"}')
expect_status "409" "$CODE" "second retry -> 409 (max retry)"
