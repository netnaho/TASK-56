#!/usr/bin/env bash
# Phase 4 — course prerequisites: add, list, self-loop, cycle, delete.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for academic_prerequisites.sh"
fi

# CS department — seeds/002_seed_departments.sql
DEPT_CS="20000000-0000-0000-0000-000000000001"

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

SUFFIX=$(date +%s | tail -c 5)
CODE_A="PRA${SUFFIX}"
CODE_B="PRB${SUFFIX}"

# ── Create course A ───────────────────────────────────────────────────────
echo "[academic_prerequisites] create ${CODE_A}"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses" \
    -d "{\"code\":\"${CODE_A}\",\"title\":\"Prereq A\",\"department_id\":\"${DEPT_CS}\",\"credit_hours\":3,\"contact_hours\":3}")
expect_status "200" "$CODE" "create course A"
ID_A=$(jq -r '.id' "$BODY")
[ -n "$ID_A" ] && [ "$ID_A" != "null" ] || fail "no id for course A"

# ── Create course B ───────────────────────────────────────────────────────
echo "[academic_prerequisites] create ${CODE_B}"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses" \
    -d "{\"code\":\"${CODE_B}\",\"title\":\"Prereq B\",\"department_id\":\"${DEPT_CS}\",\"credit_hours\":3,\"contact_hours\":3}")
expect_status "200" "$CODE" "create course B"
ID_B=$(jq -r '.id' "$BODY")
[ -n "$ID_B" ] && [ "$ID_B" != "null" ] || fail "no id for course B"
pass "courses A and B created"

# ── Add B as prerequisite of A ────────────────────────────────────────────
echo "[academic_prerequisites] add B as prereq of A"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses/${ID_A}/prerequisites" \
    -d "{\"prerequisite_course_id\":\"${ID_B}\"}")
expect_status "200" "$CODE" "add prereq A <- B"

# ── List prerequisites of A ───────────────────────────────────────────────
echo "[academic_prerequisites] list prereqs of A"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/courses/${ID_A}/prerequisites")
expect_status "200" "$CODE" "list prereqs A"
LEN=$(jq 'length' "$BODY")
[ "$LEN" = "1" ] || fail "expected 1 prereq, got $LEN"
GOT_CODE=$(jq -r '.[0].prerequisite_code' "$BODY")
[ "$GOT_CODE" = "$CODE_B" ] || fail "expected prereq code ${CODE_B}, got ${GOT_CODE}"
pass "prereq list has ${CODE_B}"

# ── Self-loop → 422 validation ────────────────────────────────────────────
echo "[academic_prerequisites] self-loop A -> A"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses/${ID_A}/prerequisites" \
    -d "{\"prerequisite_course_id\":\"${ID_A}\"}")
expect_status "422" "$CODE" "self-loop -> 422"

# ── Cycle B -> A (A already depends on B) → 409 conflict ─────────────────
echo "[academic_prerequisites] cycle B -> A"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses/${ID_B}/prerequisites" \
    -d "{\"prerequisite_course_id\":\"${ID_A}\"}")
expect_status "409" "$CODE" "cycle -> 409"

# ── Remove the A <- B link → 200 ──────────────────────────────────────────
echo "[academic_prerequisites] delete prereq A <- B"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X DELETE "${BASE}/courses/${ID_A}/prerequisites/${ID_B}")
expect_status "200" "$CODE" "remove prereq"

# ── Second delete → 404 NotFound ──────────────────────────────────────────
echo "[academic_prerequisites] delete again"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X DELETE "${BASE}/courses/${ID_A}/prerequisites/${ID_B}")
expect_status "404" "$CODE" "remove again -> 404"
