#!/usr/bin/env bash
# Phase 5 — metric CRUD happy path: create → draft v2 → approve → publish.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for metric_crud_happy_path.sh"
fi

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

ADMIN_TOKEN=$(login_as "admin@scholarly.local")
SUFFIX=$(date +%s | tail -c 5)
KEY="test.foot_traffic_${SUFFIX}"

# ── Create metric (v1 draft) ─────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/metrics" \
    -d "{\"key_name\":\"${KEY}\",\"display_name\":\"Test foot traffic\",\"polarity\":\"higher_is_better\",\"formula\":\"COUNT(*)\",\"metric_type\":\"base\",\"change_summary\":\"phase 5 test\"}")
expect_status "200" "$CODE" "create metric"
METRIC_ID=$(jq -r '.id' "$BODY")
[ -n "$METRIC_ID" ] && [ "$METRIC_ID" != "null" ] || fail "no metric id"

# ── GET metric — effective version should be v1 draft ────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/metrics/${METRIC_ID}")
expect_status "200" "$CODE" "get metric"

V1_NUM=$(jq -r '.effective_version.version_number' "$BODY")
V1_STATE=$(jq -r '.effective_version.state' "$BODY")
[ "$V1_NUM" = "1" ] || fail "expected effective_version.version_number=1, got $V1_NUM"
[ "$V1_STATE" = "draft" ] || fail "expected effective_version.state=draft, got $V1_STATE"
pass "v1 is draft"

# ── PUT draft v2 ──────────────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X PUT "${BASE}/metrics/${METRIC_ID}" \
    -d '{"formula":"COUNT(DISTINCT user_id)","metric_type":"base","change_summary":"de-dup"}')
expect_status "200" "$CODE" "put draft v2"

V2_ID=$(jq -r '.id' "$BODY")
V2_NUM=$(jq -r '.version_number' "$BODY")
V2_STATE=$(jq -r '.state' "$BODY")
[ "$V2_NUM" = "2" ] || fail "expected version_number=2, got $V2_NUM"
[ "$V2_STATE" = "draft" ] || fail "expected state=draft, got $V2_STATE"

# ── Approve v2 ────────────────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/metrics/${METRIC_ID}/versions/${V2_ID}/approve")
expect_status "200" "$CODE" "approve v2"
V2_STATE=$(jq -r '.state' "$BODY")
[ "$V2_STATE" = "approved" ] || fail "expected approved, got $V2_STATE"

# ── Publish v2 ────────────────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/metrics/${METRIC_ID}/versions/${V2_ID}/publish")
expect_status "200" "$CODE" "publish v2"
CURRENT=$(jq -r '.current_version_id' "$BODY")
[ "$CURRENT" = "$V2_ID" ] || fail "expected current_version_id=${V2_ID}, got $CURRENT"
pass "v2 is now the baseline"
