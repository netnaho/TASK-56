#!/usr/bin/env bash
# Phase 4 — course happy path: create -> draft v2 -> approve -> publish.
# Exercises the course versioning state machine via the REST API.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for academic_course_happy_path.sh"
fi

# CS department — seeds/002_seed_departments.sql
DEPT_CS="20000000-0000-0000-0000-000000000001"

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

# Unique course code per run (letters + digits only to satisfy format).
# is_valid_course_code accepts 2-5 letters + 3-4 digits + optional trailing letter.
SUFFIX=$(date +%s | tail -c 5)      # last 4 digits of epoch, ~4 digits
CODE_STR="TEST${SUFFIX}"

# ── Create the course ──────────────────────────────────────────────────────
echo "[academic_course_happy_path] POST ${BASE}/courses"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses" \
    -d "{\"code\":\"${CODE_STR}\",\"title\":\"Testing 101\",\"department_id\":\"${DEPT_CS}\",\"credit_hours\":3,\"contact_hours\":3}")
expect_status "200" "$CODE" "create course"

COURSE_ID=$(jq -r '.id' "$BODY")
[ -n "$COURSE_ID" ] && [ "$COURSE_ID" != "null" ] || fail "no course id in create response"
pass "course created (id=${COURSE_ID}, code=${CODE_STR})"

# ── GET course back ────────────────────────────────────────────────────────
echo "[academic_course_happy_path] GET ${BASE}/courses/${COURSE_ID}"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/courses/${COURSE_ID}")
expect_status "200" "$CODE" "get course"

STATE=$(jq -r '.effective_version.state' "$BODY")
NUM=$(jq -r '.effective_version.version_number' "$BODY")
[ "$STATE" = "draft" ] || fail "expected effective_version.state=draft, got $STATE"
[ "$NUM" = "1" ] || fail "expected effective_version.version_number=1, got $NUM"
pass "effective_version is v1 draft"

# ── PUT creates v2 ─────────────────────────────────────────────────────────
echo "[academic_course_happy_path] PUT ${BASE}/courses/${COURSE_ID}"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X PUT "${BASE}/courses/${COURSE_ID}" \
    -d '{"credit_hours":3.5,"contact_hours":4,"description":"Updated","change_summary":"v2"}')
expect_status "200" "$CODE" "put (draft v2)"

V2_ID=$(jq -r '.id' "$BODY")
V2_NUM=$(jq -r '.version_number' "$BODY")
V2_STATE=$(jq -r '.state' "$BODY")
[ "$V2_NUM" = "2" ] || fail "expected version_number=2, got $V2_NUM"
[ "$V2_STATE" = "draft" ] || fail "expected v2 state=draft, got $V2_STATE"
pass "draft v2 created (version_id=${V2_ID})"

# ── Approve v2 ─────────────────────────────────────────────────────────────
echo "[academic_course_happy_path] POST approve v2"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/courses/${COURSE_ID}/versions/${V2_ID}/approve")
expect_status "200" "$CODE" "approve v2"
APPROVED_STATE=$(jq -r '.state' "$BODY")
[ "$APPROVED_STATE" = "approved" ] || fail "expected state=approved, got $APPROVED_STATE"
pass "v2 approved"

# ── Publish v2 ─────────────────────────────────────────────────────────────
echo "[academic_course_happy_path] POST publish v2"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/courses/${COURSE_ID}/versions/${V2_ID}/publish")
expect_status "200" "$CODE" "publish v2"

CURRENT_VID=$(jq -r '.current_version_id' "$BODY")
[ "$CURRENT_VID" = "$V2_ID" ] \
    || fail "expected current_version_id=${V2_ID}, got $CURRENT_VID"
pass "course baseline moved to v2"

# ── Version list retains both v1 and v2 ────────────────────────────────────
echo "[academic_course_happy_path] GET versions list"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/courses/${COURSE_ID}/versions")
expect_status "200" "$CODE" "list versions"

LEN=$(jq 'length' "$BODY")
if [ "${LEN:-0}" -lt 2 ]; then
    cat "$BODY"
    fail "expected >= 2 versions, got $LEN"
fi
pass "version list retains v1 (len=${LEN})"
