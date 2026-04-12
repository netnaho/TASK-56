#!/usr/bin/env bash
# Phase 5 — check-in listing masks PII for callers that hold
# CheckinRead but lack DashboardViewSensitive and are not the section's
# instructor.
#
# The task spec names "Viewer" as the masked caller. In the Phase 5
# capability matrix Viewer does NOT have CheckinRead at all and would
# be rejected with 403, so we use an Instructor who is *not* the
# assigned section instructor — they also hit the masked path because
# they lack DashboardViewSensitive and the ownership branch does not
# match. Admin always sees the sensitive view and is the reference.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for checkin_masking.sh"
fi

DEPT_CS="20000000-0000-0000-0000-000000000001"

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

# ── Admin creates course + section (no instructor assigned) ──────────────
ADMIN_TOKEN=$(login_as "admin@scholarly.local")
SUFFIX=$(date +%s | tail -c 5)
COURSE_CODE="CKM${SUFFIX}"

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses" \
    -d "{\"code\":\"${COURSE_CODE}\",\"title\":\"Mask test\",\"department_id\":\"${DEPT_CS}\",\"credit_hours\":3,\"contact_hours\":3}")
expect_status "200" "$CODE" "create course"
COURSE_ID=$(jq -r '.id' "$BODY")

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/sections" \
    -d "{\"course_id\":\"${COURSE_ID}\",\"section_code\":\"01\",\"term\":\"fall\",\"year\":2026,\"capacity\":30}")
expect_status "200" "$CODE" "create section"
SECTION_ID=$(jq -r '.id' "$BODY")

# ── Admin performs a check-in so the row exists ───────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/checkins" \
    -d "{\"section_id\":\"${SECTION_ID}\",\"checkin_type\":\"qr_code\"}")
expect_status "200" "$CODE" "admin check-in"

# ── Masked caller (Instructor not assigned to the section) ────────────────
INSTRUCTOR_TOKEN=$(login_as "instructor@scholarly.local")
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${INSTRUCTOR_TOKEN}" \
    "${BASE}/checkins?section_id=${SECTION_ID}")
expect_status "200" "$CODE" "masked listing -> 200"

MASKED_EMAIL=$(jq -r '.[0].user_email' "$BODY")
[ "$MASKED_EMAIL" = "null" ] || fail "expected user_email=null for masked caller, got $MASKED_EMAIL"
pass "user_email is null for masked caller"

# ── Admin view: email visible ─────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/checkins?section_id=${SECTION_ID}")
expect_status "200" "$CODE" "admin listing -> 200"

ADMIN_EMAIL=$(jq -r '.[0].user_email' "$BODY")
[ "$ADMIN_EMAIL" != "null" ] || fail "admin should see real email, got null"
[ -n "$ADMIN_EMAIL" ] || fail "admin email empty"
pass "admin sees real user_email (${ADMIN_EMAIL})"
