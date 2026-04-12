#!/usr/bin/env bash
# Phase 5 — authenticated GET /checkins/retry-reasons returns the
# controlled list seeded by 006_seed_checkin_retry_reasons.sql.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for checkin_retry_reasons_endpoint.sh"
fi

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/checkins/retry-reasons")
expect_status "200" "$CODE" "GET /checkins/retry-reasons -> 200"

for reason in device_glitch wrong_section scanner_misread; do
    FOUND=$(jq --arg r "$reason" '[.[] | select(.reason_code == $r)] | length' "$BODY")
    [ "$FOUND" = "1" ] || fail "expected reason_code=$reason in response"
    pass "reason_code=${reason} present"
done
