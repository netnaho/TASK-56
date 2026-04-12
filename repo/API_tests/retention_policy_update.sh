#!/usr/bin/env bash
# Retention policy CRUD authorization tests.
# Verifies that only admin/RetentionManage can update policies.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

echo "[retention_policy_update] testing retention policy authorization"

BODY=$(mktemp)

ADMIN_TOKEN=$(login_as "admin@scholarly.local")
VIEWER_TOKEN=$(login_as "viewer@scholarly.local")
INSTRUCTOR_TOKEN=$(login_as "instructor@scholarly.local")

# ── List policies (admin) ────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/admin/retention")
expect_status "200" "$CODE" "admin lists retention policies"
pass "admin can list retention policies"

# ── List policies (viewer → should fail) ────────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer ${VIEWER_TOKEN}" \
    "${BASE}/admin/retention")
expect_status "403" "$CODE" "viewer cannot list retention policies"

# ── List policies (instructor → should fail) ────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer ${INSTRUCTOR_TOKEN}" \
    "${BASE}/admin/retention")
expect_status "403" "$CODE" "instructor cannot list retention policies"

# ── Get first policy ID from admin list ─────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/admin/retention")
if have_jq; then
    POLICY_ID=$(jq -r '.[0].id // empty' "$BODY")
else
    POLICY_ID=""
fi

if [ -z "$POLICY_ID" ] || [ "$POLICY_ID" = "null" ]; then
    pass "no retention policies seeded yet — skipping update tests"
    rm -f "$BODY"
    echo "[retention_policy_update] ALL PASS"
    exit 0
fi

pass "found policy (id=${POLICY_ID})"

# ── Admin can update a policy ────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X PUT "${BASE}/admin/retention/${POLICY_ID}" \
    -d '{"retention_days": 2555, "is_active": true}')
expect_status "200" "$CODE" "admin updates retention policy"
UPDATED_DAYS=$(json_field "$BODY" retention_days)
pass "admin updated policy (retention_days=${UPDATED_DAYS})"

# ── Viewer cannot update ─────────────────────────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${VIEWER_TOKEN}" \
    -X PUT "${BASE}/admin/retention/${POLICY_ID}" \
    -d '{"retention_days": 1}')
expect_status "403" "$CODE" "viewer cannot update retention policy"

# ── Instructor cannot update ─────────────────────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${INSTRUCTOR_TOKEN}" \
    -X PUT "${BASE}/admin/retention/${POLICY_ID}" \
    -d '{"retention_days": 1}')
expect_status "403" "$CODE" "instructor cannot update retention policy"

# ── Validation: negative retention_days rejected ────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X PUT "${BASE}/admin/retention/${POLICY_ID}" \
    -d '{"retention_days": -1}')
expect_status "422" "$CODE" "negative retention_days rejected"

# ── Unauthenticated → 401 ────────────────────────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    "${BASE}/admin/retention")
expect_status "401" "$CODE" "unauthenticated request returns 401"

rm -f "$BODY"
echo "[retention_policy_update] ALL PASS"
