#!/usr/bin/env bash
# Phase 5 — duplicate check-ins inside the configured window are blocked
# with HTTP 409 and the duplicate row is still persisted as evidence.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for checkin_duplicate_blocked.sh"
fi

DEPT_CS="20000000-0000-0000-0000-000000000001"

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

# ── Admin creates course + section ────────────────────────────────────────
ADMIN_TOKEN=$(login_as "admin@scholarly.local")
SUFFIX=$(date +%s | tail -c 5)
COURSE_CODE="CKD${SUFFIX}"

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses" \
    -d "{\"code\":\"${COURSE_CODE}\",\"title\":\"Dup test\",\"department_id\":\"${DEPT_CS}\",\"credit_hours\":3,\"contact_hours\":3}")
expect_status "200" "$CODE" "create course"
COURSE_ID=$(jq -r '.id' "$BODY")

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/sections" \
    -d "{\"course_id\":\"${COURSE_ID}\",\"section_code\":\"01\",\"term\":\"fall\",\"year\":2026,\"capacity\":30}")
expect_status "200" "$CODE" "create section"
SECTION_ID=$(jq -r '.id' "$BODY")

# ── Instructor tap #1 → 200 ───────────────────────────────────────────────
INSTRUCTOR_TOKEN=$(login_as "instructor@scholarly.local")
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${INSTRUCTOR_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/checkins" \
    -d "{\"section_id\":\"${SECTION_ID}\",\"checkin_type\":\"qr_code\",\"device_fingerprint\":{\"ua\":\"phase5-test\"}}")
expect_status "200" "$CODE" "first tap -> 200"

# ── Instructor tap #2 → 409 ───────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${INSTRUCTOR_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/checkins" \
    -d "{\"section_id\":\"${SECTION_ID}\",\"checkin_type\":\"qr_code\",\"device_fingerprint\":{\"ua\":\"phase5-test\"}}")
expect_status "409" "$CODE" "duplicate tap -> 409"

ERR_CODE=$(jq -r '.error.code' "$BODY")
[[ "$ERR_CODE" == *"conflict"* ]] || fail "expected error.code to contain 'conflict', got $ERR_CODE"
pass "duplicate rejected with conflict error code"

# ── Listing via admin must now show 2 rows, one is_duplicate_attempt=true.
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/checkins?section_id=${SECTION_ID}")
expect_status "200" "$CODE" "list checkins"

TOTAL=$(jq 'length' "$BODY")
DUPES=$(jq '[.[] | select(.is_duplicate_attempt == true)] | length' "$BODY")
[ "$TOTAL" -ge "2" ] || fail "expected at least 2 rows, got $TOTAL"
[ "$DUPES" -ge "1" ] || fail "expected at least one duplicate row, got $DUPES"
pass "list contains original + duplicate evidence row"
