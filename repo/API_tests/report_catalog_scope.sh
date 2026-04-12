#!/usr/bin/env bash
# Phase 6 (hardened) — JournalCatalog and ResourceCatalog are department-scoped
# via the creator/owner's department, not hard-blocked for non-admin callers.
#
# Assertions:
#   1. DeptHead (dept=CS) can trigger JournalCatalog and ResourceCatalog runs
#      (HTTP 202), whereas before the fix they received HTTP 403.
#   2. DeptHead's ResourceCatalog artifact contains CS-owned resources
#      but NOT resources owned by users with no department (e.g. admin).
#   3. Admin's (All-scope) ResourceCatalog artifact contains both.
#   4. DeptHead's JournalCatalog artifact does NOT contain journals created
#      by LS-dept users (cross-dept isolation).
#   5. Admin's JournalCatalog artifact DOES contain the LS-dept journal.
#
# Prerequisites: running backend at $BASE (default http://localhost:8000/api/v1)
# and `jq` in PATH.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for report_catalog_scope.sh"
fi

DEPT_CS="20000000-0000-0000-0000-000000000001"
BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

SUFFIX=$(date +%s | tail -c 8)

# ── Auth ──────────────────────────────────────────────────────────────────────
ADMIN_TOKEN=$(login_as "admin@scholarly.local")
LIBRARIAN_TOKEN=$(login_as "librarian@scholarly.local")
INSTRUCTOR_TOKEN=$(login_as "instructor@scholarly.local")
DEPTHEAD_TOKEN=$(login_as "depthead@scholarly.local")

# ══════════════════════════════════════════════════════════════════════════════
# Section A — JournalCatalog scope
# ══════════════════════════════════════════════════════════════════════════════

# A1. Librarian (dept=LS) creates a sentinel journal.
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${LIBRARIAN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/journals" \
    -d "{\"title\":\"ScopeTest-LS-Journal-${SUFFIX}\",\"body\":\"Scope test journal body\"}")
expect_status "200" "$CODE" "librarian creates LS journal"
LS_JOURNAL_ID=$(jq -r '.id' "$BODY")
[ -n "$LS_JOURNAL_ID" ] && [ "$LS_JOURNAL_ID" != "null" ] || fail "no journal id"
echo "  LS journal id: ${LS_JOURNAL_ID}"

# A2. DeptHead creates a JournalCatalog report.
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${DEPTHEAD_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/reports" \
    -d "{
        \"title\": \"JournalCatalog scope test ${SUFFIX}\",
        \"query_definition\": {\"report_type\": \"journal_catalog\", \"filters\": {}},
        \"default_format\": \"csv\"
    }")
expect_status "200" "$CODE" "depthead creates JournalCatalog report"
JOURNAL_REPORT_ID=$(jq -r '.id' "$BODY")
[ -n "$JOURNAL_REPORT_ID" ] && [ "$JOURNAL_REPORT_ID" != "null" ] || fail "no report id"

# A3. DeptHead runs the report — must now be 202, NOT 403.
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${DEPTHEAD_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/reports/${JOURNAL_REPORT_ID}/run" \
    -d '{}')
if [ "$CODE" = "403" ]; then
    fail "REGRESSION: DeptHead received 403 for JournalCatalog run — the hard-block bug is back"
fi
expect_status "202" "$CODE" "depthead triggers JournalCatalog run -> 202"
DH_JOURNAL_RUN_ID=$(jq -r '.id' "$BODY")
echo "  DeptHead JournalCatalog run id: ${DH_JOURNAL_RUN_ID}"

# A4. DeptHead downloads the artifact and verifies isolation.
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${DEPTHEAD_TOKEN}" \
    -X GET "${BASE}/reports/runs/${DH_JOURNAL_RUN_ID}/download")
expect_status "200" "$CODE" "depthead downloads JournalCatalog artifact"
if grep -q "${LS_JOURNAL_ID}" "$BODY"; then
    fail "SCOPE LEAK: DeptHead (CS) JournalCatalog contains LS journal ${LS_JOURNAL_ID}"
fi
echo "  PASS: LS journal ${LS_JOURNAL_ID} correctly excluded from CS-scope export"

# A5. Admin runs same report — LS journal must appear.
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/reports/${JOURNAL_REPORT_ID}/run" \
    -d '{}')
expect_status "202" "$CODE" "admin triggers JournalCatalog run"
ADMIN_JOURNAL_RUN_ID=$(jq -r '.id' "$BODY")

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X GET "${BASE}/reports/runs/${ADMIN_JOURNAL_RUN_ID}/download")
expect_status "200" "$CODE" "admin downloads JournalCatalog artifact"
if ! grep -q "${LS_JOURNAL_ID}" "$BODY"; then
    fail "Admin (All-scope) JournalCatalog must contain LS journal ${LS_JOURNAL_ID}"
fi
echo "  PASS: LS journal ${LS_JOURNAL_ID} visible in admin's All-scope export"

# ══════════════════════════════════════════════════════════════════════════════
# Section B — ResourceCatalog scope
# ══════════════════════════════════════════════════════════════════════════════

# B1. Instructor (dept=CS) creates a CS resource.
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${INSTRUCTOR_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/teaching-resources" \
    -d "{\"title\":\"ScopeTest-CS-Resource-${SUFFIX}\",\"resource_type\":\"document\"}")
expect_status "200" "$CODE" "instructor creates CS resource"
CS_RESOURCE_ID=$(jq -r '.id' "$BODY")
[ -n "$CS_RESOURCE_ID" ] && [ "$CS_RESOURCE_ID" != "null" ] || fail "no CS resource id"
echo "  CS resource id: ${CS_RESOURCE_ID}"

# B2. Admin (dept=NULL) creates a NULL-dept resource.
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/teaching-resources" \
    -d "{\"title\":\"ScopeTest-NULL-Resource-${SUFFIX}\",\"resource_type\":\"document\"}")
expect_status "200" "$CODE" "admin creates NULL-dept resource"
NULL_RESOURCE_ID=$(jq -r '.id' "$BODY")
[ -n "$NULL_RESOURCE_ID" ] && [ "$NULL_RESOURCE_ID" != "null" ] || fail "no NULL resource id"
echo "  NULL-dept resource id: ${NULL_RESOURCE_ID}"

# B3. DeptHead creates a ResourceCatalog report.
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${DEPTHEAD_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/reports" \
    -d "{
        \"title\": \"ResourceCatalog scope test ${SUFFIX}\",
        \"query_definition\": {\"report_type\": \"resource_catalog\", \"filters\": {}},
        \"default_format\": \"csv\"
    }")
expect_status "200" "$CODE" "depthead creates ResourceCatalog report"
RESOURCE_REPORT_ID=$(jq -r '.id' "$BODY")
[ -n "$RESOURCE_REPORT_ID" ] && [ "$RESOURCE_REPORT_ID" != "null" ] || fail "no report id"

# B4. DeptHead runs the ResourceCatalog report — must be 202, NOT 403.
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${DEPTHEAD_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/reports/${RESOURCE_REPORT_ID}/run" \
    -d '{}')
if [ "$CODE" = "403" ]; then
    fail "REGRESSION: DeptHead received 403 for ResourceCatalog run — the hard-block bug is back"
fi
expect_status "202" "$CODE" "depthead triggers ResourceCatalog run -> 202"
DH_RESOURCE_RUN_ID=$(jq -r '.id' "$BODY")
echo "  DeptHead ResourceCatalog run id: ${DH_RESOURCE_RUN_ID}"

# B5. DeptHead downloads artifact and verifies:
#     - CS resource IS present
#     - NULL-dept (admin) resource is NOT present
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${DEPTHEAD_TOKEN}" \
    -X GET "${BASE}/reports/runs/${DH_RESOURCE_RUN_ID}/download")
expect_status "200" "$CODE" "depthead downloads ResourceCatalog artifact"
if ! grep -q "${CS_RESOURCE_ID}" "$BODY"; then
    fail "DeptHead (CS-scope) ResourceCatalog must contain CS-owned resource ${CS_RESOURCE_ID}"
fi
echo "  PASS: CS resource ${CS_RESOURCE_ID} present in CS-scope export"
if grep -q "${NULL_RESOURCE_ID}" "$BODY"; then
    fail "SCOPE LEAK: DeptHead (CS) ResourceCatalog contains NULL-dept resource ${NULL_RESOURCE_ID}"
fi
echo "  PASS: NULL-dept resource ${NULL_RESOURCE_ID} correctly excluded from CS-scope export"

# B6. Admin runs the same report — both resources must appear.
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/reports/${RESOURCE_REPORT_ID}/run" \
    -d '{}')
expect_status "202" "$CODE" "admin triggers ResourceCatalog run"
ADMIN_RESOURCE_RUN_ID=$(jq -r '.id' "$BODY")

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X GET "${BASE}/reports/runs/${ADMIN_RESOURCE_RUN_ID}/download")
expect_status "200" "$CODE" "admin downloads ResourceCatalog artifact"
if ! grep -q "${CS_RESOURCE_ID}" "$BODY"; then
    fail "Admin (All-scope) ResourceCatalog must contain CS resource ${CS_RESOURCE_ID}"
fi
if ! grep -q "${NULL_RESOURCE_ID}" "$BODY"; then
    fail "Admin (All-scope) ResourceCatalog must contain NULL-dept resource ${NULL_RESOURCE_ID}"
fi
echo "  PASS: Both resources visible in admin's All-scope export"

echo ""
echo "ALL CHECKS PASSED: report_catalog_scope"
