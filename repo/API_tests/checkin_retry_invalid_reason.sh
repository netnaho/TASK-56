#!/usr/bin/env bash
# Phase 5 — the retry endpoint rejects reason codes that are not in
# `checkin_retry_reasons`, including empty string.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for checkin_retry_invalid_reason.sh"
fi

DEPT_CS="20000000-0000-0000-0000-000000000001"

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

# ── Admin setup ──────────────────────────────────────────────────────────
ADMIN_TOKEN=$(login_as "admin@scholarly.local")
SUFFIX=$(date +%s | tail -c 5)
COURSE_CODE="CRI${SUFFIX}"

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses" \
    -d "{\"code\":\"${COURSE_CODE}\",\"title\":\"Retry reason test\",\"department_id\":\"${DEPT_CS}\",\"credit_hours\":3,\"contact_hours\":3}")
expect_status "200" "$CODE" "create course"
COURSE_ID=$(jq -r '.id' "$BODY")

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/sections" \
    -d "{\"course_id\":\"${COURSE_ID}\",\"section_code\":\"01\",\"term\":\"fall\",\"year\":2026,\"capacity\":30}")
expect_status "200" "$CODE" "create section"
SECTION_ID=$(jq -r '.id' "$BODY")

# ── Instructor tap ────────────────────────────────────────────────────────
INSTRUCTOR_TOKEN=$(login_as "instructor@scholarly.local")
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${INSTRUCTOR_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/checkins" \
    -d "{\"section_id\":\"${SECTION_ID}\",\"checkin_type\":\"qr_code\"}")
expect_status "200" "$CODE" "first tap -> 200"
ORIGINAL_ID=$(jq -r '.view.id' "$BODY")
[ -n "$ORIGINAL_ID" ] && [ "$ORIGINAL_ID" != "null" ] || fail "no original id"

# ── Bogus reason → 422 ───────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${INSTRUCTOR_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/checkins/${ORIGINAL_ID}/retry" \
    -d '{"reason_code":"totally_made_up"}')
expect_status "422" "$CODE" "bogus reason -> 422"

# ── Empty reason → 422 ───────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${INSTRUCTOR_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/checkins/${ORIGINAL_ID}/retry" \
    -d '{"reason_code":""}')
expect_status "422" "$CODE" "empty reason -> 422"
