#!/usr/bin/env bash
# Viewer cannot write journals (403); missing auth -> 401.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

VIEWER_TOKEN=$(login_as "viewer@scholarly.local")

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

# Authenticated viewer trying to write -> 403 Forbidden.
echo "[library_journal_unauthorized] viewer POST /journals"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${VIEWER_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/journals" \
    -d '{"title":"Viewer should not be able to do this","body":"hello"}')
expect_status "403" "$CODE" "viewer create -> 403"

# No Authorization header -> 401 Unauthorized.
echo "[library_journal_unauthorized] unauthenticated POST /journals"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/journals" \
    -d '{"title":"Anon cannot write","body":"hello"}')
expect_status "401" "$CODE" "anon create -> 401"
