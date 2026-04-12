#!/usr/bin/env bash
# Admin config CRUD — list, get, update settings.
# Non-admin callers must be rejected with 403.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

echo "[admin_config] testing admin settings API"

BODY=$(mktemp)

ADMIN_TOKEN=$(login_as "admin@scholarly.local")
VIEWER_TOKEN=$(login_as "viewer@scholarly.local")

# ── List settings (admin) ────────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/admin/config")
expect_status "200" "$CODE" "list admin settings (admin)"

if have_jq; then
    COUNT=$(jq 'length' "$BODY")
    if [ "$COUNT" -gt "0" ]; then
        pass "got ${COUNT} settings"
    else
        pass "settings list is empty (no seeds run yet)"
    fi
fi

# ── Viewer cannot list settings ───────────────────────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer ${VIEWER_TOKEN}" \
    "${BASE}/admin/config")
expect_status "403" "$CODE" "viewer cannot list settings"

# ── Upsert a setting ─────────────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X PUT "${BASE}/admin/config/api_test.integration_marker" \
    -d '{"value": "phase7_test", "description": "Set by API test suite"}')
expect_status "200" "$CODE" "upsert admin setting"

if have_jq; then
    KEY=$(jq -r '.key // empty' "$BODY")
    VAL=$(jq -r '.value // empty' "$BODY")
    if [ "$KEY" = "api_test.integration_marker" ]; then
        pass "setting key matches"
    else
        fail "setting key mismatch: ${KEY}"
    fi
    if [ "$VAL" = "phase7_test" ]; then
        pass "setting value matches"
    else
        fail "setting value mismatch: ${VAL}"
    fi
fi

# ── Read back the upserted setting ───────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/admin/config/api_test.integration_marker")
expect_status "200" "$CODE" "get setting by key"

# ── Non-existent key → 404 ────────────────────────────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/admin/config/key_that_does_not_exist_xyz123")
expect_status "404" "$CODE" "non-existent key returns 404"

# ── Viewer cannot update settings ─────────────────────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${VIEWER_TOKEN}" \
    -X PUT "${BASE}/admin/config/api_test.viewer_attempt" \
    -d '{"value": "forbidden"}')
expect_status "403" "$CODE" "viewer cannot update settings"

rm -f "$BODY"
echo "[admin_config] ALL PASS"
