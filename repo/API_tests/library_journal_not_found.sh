#!/usr/bin/env bash
# GETs against missing/bad journal ids return 404 / 422 appropriately.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

# Well-formed UUID that does not exist -> 404.
DEAD_ID="00000000-0000-0000-0000-00000000dead"
echo "[library_journal_not_found] GET /journals/${DEAD_ID}"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/journals/${DEAD_ID}")
expect_status "404" "$CODE" "missing journal -> 404"

# Non-UUID path segment -> validation 422.
echo "[library_journal_not_found] GET /journals/not-a-uuid"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/journals/not-a-uuid")
expect_status "422" "$CODE" "non-UUID id -> 422"
