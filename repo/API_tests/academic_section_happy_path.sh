#!/usr/bin/env bash
# Phase 4 — section happy path: create a parent course, create a section,
# draft v2, approve, publish, then list via the filtered GET.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for academic_section_happy_path.sh"
fi

# CS department — seeds/002_seed_departments.sql
DEPT_CS="20000000-0000-0000-0000-000000000001"

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

SUFFIX=$(date +%s | tail -c 5)
COURSE_CODE="SEC${SUFFIX}"

# ── Create parent course ──────────────────────────────────────────────────
echo "[academic_section_happy_path] create parent course ${COURSE_CODE}"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses" \
    -d "{\"code\":\"${COURSE_CODE}\",\"title\":\"Section parent\",\"department_id\":\"${DEPT_CS}\",\"credit_hours\":3,\"contact_hours\":3}")
expect_status "200" "$CODE" "create parent course"
COURSE_ID=$(jq -r '.id' "$BODY")
[ -n "$COURSE_ID" ] && [ "$COURSE_ID" != "null" ] || fail "no parent course id"

# ── Create section 01 / fall 2026 ─────────────────────────────────────────
echo "[academic_section_happy_path] create section"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/sections" \
    -d "{\"course_id\":\"${COURSE_ID}\",\"section_code\":\"01\",\"term\":\"fall\",\"year\":2026,\"capacity\":30,\"location\":\"Room 100\"}")
expect_status "200" "$CODE" "create section"
SECTION_ID=$(jq -r '.id' "$BODY")
[ -n "$SECTION_ID" ] && [ "$SECTION_ID" != "null" ] || fail "no section id"
pass "section created (id=${SECTION_ID})"

# ── GET section back ──────────────────────────────────────────────────────
echo "[academic_section_happy_path] GET section"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/sections/${SECTION_ID}")
expect_status "200" "$CODE" "get section"
NUM=$(jq -r '.effective_version.version_number' "$BODY")
[ "$NUM" = "1" ] || fail "expected effective_version.version_number=1, got $NUM"
pass "effective_version is v1"

# ── PUT creates draft v2 with a schedule_note ─────────────────────────────
echo "[academic_section_happy_path] PUT section (draft v2)"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X PUT "${BASE}/sections/${SECTION_ID}" \
    -d '{"schedule_note":"MWF 9-10am","change_summary":"add schedule"}')
expect_status "200" "$CODE" "put (draft v2)"
V2_ID=$(jq -r '.id' "$BODY")
V2_NUM=$(jq -r '.version_number' "$BODY")
V2_STATE=$(jq -r '.state' "$BODY")
[ "$V2_NUM" = "2" ] || fail "expected v2.version_number=2, got $V2_NUM"
[ "$V2_STATE" = "draft" ] || fail "expected v2.state=draft, got $V2_STATE"

# ── Approve v2 ────────────────────────────────────────────────────────────
echo "[academic_section_happy_path] approve v2"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/sections/${SECTION_ID}/versions/${V2_ID}/approve")
expect_status "200" "$CODE" "approve v2"

# ── Publish v2 ────────────────────────────────────────────────────────────
echo "[academic_section_happy_path] publish v2"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/sections/${SECTION_ID}/versions/${V2_ID}/publish")
expect_status "200" "$CODE" "publish v2"

CURRENT_VID=$(jq -r '.current_version_id' "$BODY")
[ "$CURRENT_VID" = "$V2_ID" ] || fail "expected current_version_id=${V2_ID}, got $CURRENT_VID"
pass "section baseline moved to v2"

# ── List sections filtered by course id ───────────────────────────────────
echo "[academic_section_happy_path] list sections for course"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/sections?course_id=${COURSE_ID}&limit=50")
expect_status "200" "$CODE" "list sections"

FOUND=$(jq --arg id "$SECTION_ID" '[.[] | select(.id == $id)] | length' "$BODY")
[ "$FOUND" = "1" ] || fail "list did not contain section ${SECTION_ID}"
pass "list contains the created section"
