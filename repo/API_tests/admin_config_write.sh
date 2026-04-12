#!/usr/bin/env bash
# Updating an admin setting should:
#   1. Return 200 and the new value.
#   2. Create an audit entry with action = admin.config.write.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

# Write a benign test key so we don't clobber real settings.
KEY="test.phase2_write_probe"

echo "[admin_config_write] PUT /admin/config/${KEY}"
BODY=$(mktemp)
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X PUT "${BASE}/admin/config/${KEY}" \
    -d '{"value": {"probe": true, "ts": "2026-04-11"}, "description": "Phase 2 test probe"}')
expect_status "200" "$CODE" "admin config write"

echo "[admin_config_write] GET /admin/config/${KEY}"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/admin/config/${KEY}")
expect_status "200" "$CODE" "admin config read"

echo "[admin_config_write] GET /audit-logs?action=admin.config.write"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/audit-logs?action=admin.config.write&limit=5")
expect_status "200" "$CODE" "audit search by action"

if have_jq; then
    COUNT=$(jq '[.entries[] | select(.action == "admin.config.write")] | length' "$BODY")
    if [ "${COUNT:-0}" -lt 1 ]; then
        cat "$BODY"
        fail "no admin.config.write audit entry after write"
    fi
    pass "admin.config.write recorded in audit log"
fi

rm -f "$BODY"
