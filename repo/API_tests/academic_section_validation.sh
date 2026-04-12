#!/usr/bin/env bash
# Phase 4 — section validation: capacity, term, year bounds, duplicate.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for academic_section_validation.sh"
fi

# CS department — seeds/002_seed_departments.sql
DEPT_CS="20000000-0000-0000-0000-000000000001"

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

SUFFIX=$(date +%s | tail -c 5)
COURSE_CODE="SEV${SUFFIX}"

# ── Create a parent course ────────────────────────────────────────────────
echo "[academic_section_validation] create parent course ${COURSE_CODE}"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses" \
    -d "{\"code\":\"${COURSE_CODE}\",\"title\":\"Section validation parent\",\"department_id\":\"${DEPT_CS}\",\"credit_hours\":3,\"contact_hours\":3}")
expect_status "200" "$CODE" "create parent course"
COURSE_ID=$(jq -r '.id' "$BODY")
[ -n "$COURSE_ID" ] && [ "$COURSE_ID" != "null" ] || fail "no parent course id"

# ── Capacity above upper bound → 422 ──────────────────────────────────────
echo "[academic_section_validation] capacity 1500"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/sections" \
    -d "{\"course_id\":\"${COURSE_ID}\",\"section_code\":\"01\",\"term\":\"fall\",\"year\":2026,\"capacity\":1500}")
expect_status "422" "$CODE" "capacity 1500 -> 422"

# ── Invalid term → 422 ────────────────────────────────────────────────────
echo "[academic_section_validation] term autumn"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/sections" \
    -d "{\"course_id\":\"${COURSE_ID}\",\"section_code\":\"01\",\"term\":\"autumn\",\"year\":2026,\"capacity\":30}")
expect_status "422" "$CODE" "term autumn -> 422"

# ── Year below lower bound → 422 ──────────────────────────────────────────
echo "[academic_section_validation] year 1999"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/sections" \
    -d "{\"course_id\":\"${COURSE_ID}\",\"section_code\":\"01\",\"term\":\"fall\",\"year\":1999,\"capacity\":30}")
expect_status "422" "$CODE" "year 1999 -> 422"

# ── Duplicate (course_id, section_code, term, year) → 409 ────────────────
echo "[academic_section_validation] duplicate section"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/sections" \
    -d "{\"course_id\":\"${COURSE_ID}\",\"section_code\":\"02\",\"term\":\"spring\",\"year\":2026,\"capacity\":30}")
expect_status "200" "$CODE" "first section create"

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/sections" \
    -d "{\"course_id\":\"${COURSE_ID}\",\"section_code\":\"02\",\"term\":\"spring\",\"year\":2026,\"capacity\":25}")
expect_status "409" "$CODE" "duplicate section -> 409"
