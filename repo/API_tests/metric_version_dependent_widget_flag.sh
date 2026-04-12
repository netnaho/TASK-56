#!/usr/bin/env bash
# Phase 5 — publishing a new metric version flags dependent widgets
# with verification_needed=TRUE.
#
# Requires `docker compose` to reach the mysql container because there
# is no create-widget HTTP route yet. Skips gracefully if docker is
# missing.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for metric_version_dependent_widget_flag.sh"
fi

if ! command -v docker >/dev/null 2>&1; then
    echo "[metric_version_dependent_widget_flag] SKIP: docker not available"
    exit 0
fi

# Helper to run a one-shot SQL against the scholarly database.
mysql_exec() {
    docker compose exec -T mysql mysql \
        -uscholarly_app -pscholarly_app_pass scholarly -N -s -e "$1"
}

if ! mysql_exec "SELECT 1;" >/dev/null 2>&1; then
    echo "[metric_version_dependent_widget_flag] SKIP: cannot reach mysql via docker compose"
    exit 0
fi

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

ADMIN_TOKEN=$(login_as "admin@scholarly.local")
SUFFIX=$(date +%s | tail -c 5)
KEY="test.widget_flag_${SUFFIX}"

# ── Create metric, approve + publish v1 ──────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/metrics" \
    -d "{\"key_name\":\"${KEY}\",\"display_name\":\"Widget flag test\",\"polarity\":\"neutral\",\"formula\":\"COUNT(*)\",\"metric_type\":\"base\",\"change_summary\":\"v1\"}")
expect_status "200" "$CODE" "create metric"
METRIC_ID=$(jq -r '.id' "$BODY")
V1_ID=$(jq -r '.effective_version.id' "$BODY")

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/metrics/${METRIC_ID}/versions/${V1_ID}/approve")
expect_status "200" "$CODE" "approve v1"

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/metrics/${METRIC_ID}/versions/${V1_ID}/publish")
expect_status "200" "$CODE" "publish v1"

# ── Ensure at least one dashboard_definitions row exists ─────────────────
DASH_ID=$(mysql_exec "SELECT id FROM dashboard_definitions LIMIT 1;" | tr -d '\r')
if [ -z "$DASH_ID" ]; then
    DASH_ID="d0000000-0000-0000-0000-$(printf '%012d' "$(date +%s)")"
    mysql_exec "INSERT INTO dashboard_definitions (id, title, owner_id, is_shared) VALUES ('${DASH_ID}', 'Phase 5 test dashboard', NULL, TRUE);" \
        || fail "failed to insert dashboard_definitions"
    echo "[metric_version_dependent_widget_flag] inserted dashboard ${DASH_ID}"
fi

# ── Insert a widget pinned to v1 with verification_needed=FALSE ──────────
WIDGET_ID=$(mysql_exec "SELECT UUID();" | tr -d '\r')
mysql_exec "INSERT INTO dashboard_widgets (id, dashboard_id, metric_definition_id, widget_type, based_on_version_id, verification_needed) VALUES ('${WIDGET_ID}', '${DASH_ID}', '${METRIC_ID}', 'bar', '${V1_ID}', FALSE);" \
    || fail "failed to insert dashboard_widget"
echo "[metric_version_dependent_widget_flag] inserted widget ${WIDGET_ID}"

# ── Draft, approve, publish v2 via API ───────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X PUT "${BASE}/metrics/${METRIC_ID}" \
    -d '{"formula":"COUNT(DISTINCT user_id)","metric_type":"base","change_summary":"v2"}')
expect_status "200" "$CODE" "draft v2"
V2_ID=$(jq -r '.id' "$BODY")

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/metrics/${METRIC_ID}/versions/${V2_ID}/approve")
expect_status "200" "$CODE" "approve v2"

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/metrics/${METRIC_ID}/versions/${V2_ID}/publish")
expect_status "200" "$CODE" "publish v2"

# ── The widget should now be flagged for re-verification ─────────────────
FLAG=$(mysql_exec "SELECT verification_needed FROM dashboard_widgets WHERE metric_definition_id = '${METRIC_ID}';" | tr -d '\r')
[ "$FLAG" = "1" ] || fail "expected verification_needed=1 after v2 publish, got '${FLAG}'"
pass "dependent widget flagged for re-verification"
