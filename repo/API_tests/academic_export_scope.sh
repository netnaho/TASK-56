#!/usr/bin/env bash
# Phase 4 — department-scoped export: a CS department head must see CS
# courses and must NOT see courses from other departments.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for academic_export_scope.sh"
fi

# Department UUIDs from seeds/002_seed_departments.sql
DEPT_CS="20000000-0000-0000-0000-000000000001"
DEPT_MATH="20000000-0000-0000-0000-000000000003"

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

BODY=$(mktemp)
EXPORT_CS=$(mktemp --suffix=.csv)
trap 'rm -f "$BODY" "$EXPORT_CS"' EXIT

SUFFIX=$(date +%s | tail -c 5)
CS_CODE="EXPCS${SUFFIX}"      # 5 letters + 4 digits → valid
MATH_CODE="EXPMA${SUFFIX}"    # 5 letters + 4 digits → valid

# ── Admin: create one CS course and one MATH course ──────────────────────
echo "[academic_export_scope] admin creates ${CS_CODE} in CS"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses" \
    -d "{\"code\":\"${CS_CODE}\",\"title\":\"CS export check\",\"department_id\":\"${DEPT_CS}\",\"credit_hours\":3,\"contact_hours\":3}")
expect_status "200" "$CODE" "create CS course"

echo "[academic_export_scope] admin creates ${MATH_CODE} in MATH"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses" \
    -d "{\"code\":\"${MATH_CODE}\",\"title\":\"MATH export check\",\"department_id\":\"${DEPT_MATH}\",\"credit_hours\":3,\"contact_hours\":3}")
expect_status "200" "$CODE" "create MATH course"

# Confirm admin can see both (sanity check — admin is unscoped).
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/courses?limit=500")
expect_status "200" "$CODE" "admin list courses"
[ "$(jq --arg c "$CS_CODE" '[.[] | select(.code==$c)] | length' "$BODY")" = "1" ] \
    || fail "admin listing missing ${CS_CODE}"
[ "$(jq --arg c "$MATH_CODE" '[.[] | select(.code==$c)] | length' "$BODY")" = "1" ] \
    || fail "admin listing missing ${MATH_CODE}"
pass "admin sees both seeded courses"

# ── Department head (CS) exports courses ─────────────────────────────────
DEPT_TOKEN=$(login_as "depthead@scholarly.local")

echo "[academic_export_scope] dept head GET /courses/export.csv"
CODE=$(curl -s -o "$EXPORT_CS" -w "%{http_code}" \
    -H "Authorization: Bearer ${DEPT_TOKEN}" \
    "${BASE}/courses/export.csv")
expect_status "200" "$CODE" "dept head export csv"

# Must contain the CS course code.
if ! grep -q "${CS_CODE}" "$EXPORT_CS"; then
    cat "$EXPORT_CS"
    fail "dept head export missing ${CS_CODE}"
fi
pass "dept head export contains ${CS_CODE}"

# Must NOT contain the MATH course code. `grep -v` returns 0 when no match.
if grep -q "${MATH_CODE}" "$EXPORT_CS"; then
    cat "$EXPORT_CS"
    fail "dept head export leaked ${MATH_CODE} from a different department"
fi
pass "dept head export excludes ${MATH_CODE}"
