#!/usr/bin/env bash
# Phase 5 — DepartmentHead can create and approve metric versions
# (MetricWrite) but cannot publish — MetricApprove is admin-only.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for metric_publish_requires_admin.sh"
fi

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

DEPT_TOKEN=$(login_as "depthead@scholarly.local")
SUFFIX=$(date +%s | tail -c 5)
KEY="test.dept_only_${SUFFIX}"

# ── Create metric (v1 draft) as DepartmentHead ───────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${DEPT_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/metrics" \
    -d "{\"key_name\":\"${KEY}\",\"display_name\":\"Dept owned\",\"polarity\":\"neutral\",\"formula\":\"COUNT(*)\",\"metric_type\":\"base\",\"change_summary\":\"dept test\"}")
expect_status "200" "$CODE" "dept head create metric"
METRIC_ID=$(jq -r '.id' "$BODY")
V1_ID=$(jq -r '.effective_version.id' "$BODY")
[ -n "$METRIC_ID" ] && [ "$METRIC_ID" != "null" ] || fail "no metric id"
[ -n "$V1_ID" ] && [ "$V1_ID" != "null" ] || fail "no v1 id"

# ── Approve v1 as DepartmentHead (MetricWrite covers approve) ─────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${DEPT_TOKEN}" \
    -X POST "${BASE}/metrics/${METRIC_ID}/versions/${V1_ID}/approve")
expect_status "200" "$CODE" "dept head approve v1"
pass "dept head can approve"

# ── Publish v1 as DepartmentHead → 403 ───────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${DEPT_TOKEN}" \
    -X POST "${BASE}/metrics/${METRIC_ID}/versions/${V1_ID}/publish")
expect_status "403" "$CODE" "dept head publish -> 403"
pass "publish is admin-only"
