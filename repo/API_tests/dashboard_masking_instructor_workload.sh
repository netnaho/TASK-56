#!/usr/bin/env bash
# Phase 5 — instructor_workload masks instructor display names unless
# the caller holds DashboardViewSensitive.
#
# Admin has DashboardViewSensitive -> sees the plain "Default Instructor".
#
# The task spec names Viewer as the masked caller, but the seeded
# Viewer user is pinned to the MATH department and would therefore
# not see the CS test section at all in the scoped query. Librarian
# holds DashboardRead without DashboardViewSensitive and is *not*
# pinned to a single department by resolve_window, so they are the
# correct stand-in: same masking path, full cross-department query.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for dashboard_masking_instructor_workload.sh"
fi

DEPT_CS="20000000-0000-0000-0000-000000000001"
INSTRUCTOR_USER_ID="30000000-0000-0000-0000-000000000003"
INSTR_NAME="Default Instructor"

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

# ── Admin creates course + section assigned to the instructor ────────────
ADMIN_TOKEN=$(login_as "admin@scholarly.local")
SUFFIX=$(date +%s | tail -c 5)
COURSE_CODE="IWM${SUFFIX}"

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses" \
    -d "{\"code\":\"${COURSE_CODE}\",\"title\":\"Workload mask\",\"department_id\":\"${DEPT_CS}\",\"credit_hours\":3,\"contact_hours\":3}")
expect_status "200" "$CODE" "create course"
COURSE_ID=$(jq -r '.id' "$BODY")

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/sections" \
    -d "{\"course_id\":\"${COURSE_ID}\",\"section_code\":\"01\",\"term\":\"fall\",\"year\":2026,\"capacity\":30,\"instructor_id\":\"${INSTRUCTOR_USER_ID}\"}")
expect_status "200" "$CODE" "create section with instructor"

# ── Admin view: must contain the plain display name somewhere ────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/dashboards/instructor-workload")
expect_status "200" "$CODE" "admin instructor-workload"

ADMIN_HIT=$(jq --arg n "$INSTR_NAME" '[.rows[] | select(.label == $n)] | length' "$BODY")
[ "$ADMIN_HIT" -ge "1" ] || fail "admin view missing plain label '${INSTR_NAME}'"
pass "admin sees plain display name"

# ── Masked caller (librarian stand-in for viewer) ────────────────────────
LIBRARIAN_TOKEN=$(login_as "librarian@scholarly.local")
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${LIBRARIAN_TOKEN}" \
    "${BASE}/dashboards/instructor-workload")
expect_status "200" "$CODE" "librarian instructor-workload"

LIB_PLAIN=$(jq --arg n "$INSTR_NAME" '[.rows[] | select(.label == $n)] | length' "$BODY")
[ "$LIB_PLAIN" = "0" ] || fail "masked caller should not see plain '${INSTR_NAME}' label"

# At least one row's label starts with "user:" (the Phase 2 mask prefix).
MASKED=$(jq '[.rows[] | select(.label | startswith("user:"))] | length' "$BODY")
[ "$MASKED" -ge "1" ] || fail "masked view has no masked 'user:' labels"
pass "masked caller sees 'user:...' label instead of plain name"
