#!/usr/bin/env bash
# Phase 5 — lineage_refs pointing at a nonexistent version are rejected.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for metric_lineage_validation.sh"
fi

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

ADMIN_TOKEN=$(login_as "admin@scholarly.local")
SUFFIX=$(date +%s | tail -c 5)
KEY="test.lineage_${SUFFIX}"

# ── Create metric A (no lineage) ─────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/metrics" \
    -d "{\"key_name\":\"${KEY}\",\"display_name\":\"Lineage target\",\"polarity\":\"neutral\",\"formula\":\"COUNT(*)\",\"metric_type\":\"base\",\"change_summary\":\"v1\"}")
expect_status "200" "$CODE" "create metric with no lineage"
METRIC_ID=$(jq -r '.id' "$BODY")

# ── PUT with a fake lineage version_id → 422 ─────────────────────────────
DEAD_VID="00000000-0000-0000-0000-00000000dead"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X PUT "${BASE}/metrics/${METRIC_ID}" \
    -d "{\"formula\":\"COUNT(*)\",\"metric_type\":\"derived\",\"change_summary\":\"bad lineage\",\"lineage_refs\":[{\"definition_id\":\"${METRIC_ID}\",\"version_id\":\"${DEAD_VID}\"}]}")
expect_status "422" "$CODE" "bad lineage -> 422"
pass "nonexistent lineage ref rejected"
