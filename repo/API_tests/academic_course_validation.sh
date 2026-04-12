#!/usr/bin/env bash
# Phase 4 — course create validation: bad code, bad credit hours, missing
# title, and duplicate-code conflict.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

# CS department — seeds/002_seed_departments.sql
DEPT_CS="20000000-0000-0000-0000-000000000001"

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

# ── Lowercase code → validation failure ───────────────────────────────────
echo "[academic_course_validation] POST with lowercase code"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses" \
    -d "{\"code\":\"cs101\",\"title\":\"Bad Code\",\"department_id\":\"${DEPT_CS}\",\"credit_hours\":3,\"contact_hours\":3}")
expect_status "422" "$CODE" "lowercase code -> 422"

# ── credit_hours out of range → validation failure ────────────────────────
echo "[academic_course_validation] POST with credit_hours=25"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses" \
    -d "{\"code\":\"VAL101\",\"title\":\"Over cap\",\"department_id\":\"${DEPT_CS}\",\"credit_hours\":25,\"contact_hours\":3}")
expect_status "422" "$CODE" "credit_hours=25 -> 422"

# ── Missing title → JSON deserialization failure → 422 ────────────────────
echo "[academic_course_validation] POST without title"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses" \
    -d "{\"code\":\"VAL102\",\"department_id\":\"${DEPT_CS}\",\"credit_hours\":3,\"contact_hours\":3}")
expect_status "422" "$CODE" "missing title -> 422"

# ── Duplicate code → first 200, second 409 ────────────────────────────────
SUFFIX=$(date +%s | tail -c 5)
DUP_CODE="DUP${SUFFIX}"
echo "[academic_course_validation] create ${DUP_CODE} twice"

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses" \
    -d "{\"code\":\"${DUP_CODE}\",\"title\":\"First\",\"department_id\":\"${DEPT_CS}\",\"credit_hours\":3,\"contact_hours\":3}")
expect_status "200" "$CODE" "first ${DUP_CODE} create"

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses" \
    -d "{\"code\":\"${DUP_CODE}\",\"title\":\"Second\",\"department_id\":\"${DEPT_CS}\",\"credit_hours\":3,\"contact_hours\":3}")
expect_status "409" "$CODE" "duplicate ${DUP_CODE} -> 409"
