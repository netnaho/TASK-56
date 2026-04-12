#!/usr/bin/env bash
# Phase 5 — check-in happy path.
# Admin creates a dedicated course + section, then an instructor
# performs a one-tap check-in and we verify the response shape.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for checkin_happy_path.sh"
fi

# CS department — seeds/002_seed_departments.sql. The default instructor
# user belongs here, so section-readability checks will succeed.
DEPT_CS="20000000-0000-0000-0000-000000000001"

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

# ── Admin creates course + section ────────────────────────────────────────
ADMIN_TOKEN=$(login_as "admin@scholarly.local")
SUFFIX=$(date +%s | tail -c 5)
COURSE_CODE="CHK${SUFFIX}"

echo "[checkin_happy_path] create course ${COURSE_CODE}"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses" \
    -d "{\"code\":\"${COURSE_CODE}\",\"title\":\"Phase 5 test\",\"department_id\":\"${DEPT_CS}\",\"credit_hours\":3,\"contact_hours\":3}")
expect_status "200" "$CODE" "create course"
COURSE_ID=$(jq -r '.id' "$BODY")
[ -n "$COURSE_ID" ] && [ "$COURSE_ID" != "null" ] || fail "no course id"

echo "[checkin_happy_path] create section"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/sections" \
    -d "{\"course_id\":\"${COURSE_ID}\",\"section_code\":\"01\",\"term\":\"fall\",\"year\":2026,\"capacity\":30}")
expect_status "200" "$CODE" "create section"
SECTION_ID=$(jq -r '.id' "$BODY")
[ -n "$SECTION_ID" ] && [ "$SECTION_ID" != "null" ] || fail "no section id"

# ── Instructor taps in ────────────────────────────────────────────────────
INSTRUCTOR_TOKEN=$(login_as "instructor@scholarly.local")

echo "[checkin_happy_path] POST /checkins"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${INSTRUCTOR_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/checkins" \
    -d "{\"section_id\":\"${SECTION_ID}\",\"checkin_type\":\"qr_code\",\"device_fingerprint\":{\"ua\":\"phase5-test\"}}")
expect_status "200" "$CODE" "instructor check-in"

STATUS=$(jq -r '.status' "$BODY")
SEQ=$(jq -r '.view.retry_sequence' "$BODY")
DUP=$(jq -r '.view.is_duplicate_attempt' "$BODY")
[ "$STATUS" = "success" ] || fail "expected status=success, got $STATUS"
[ "$SEQ" = "0" ] || fail "expected retry_sequence=0, got $SEQ"
[ "$DUP" = "false" ] || fail "expected is_duplicate_attempt=false, got $DUP"
pass "check-in recorded as success with retry_sequence=0"
