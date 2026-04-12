#!/usr/bin/env bash
# Journal create validation: body, title required and min-length enforced.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

# Empty body string -> validate_body rejects.
echo "[library_journal_validation] POST with empty body"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/journals" \
    -d '{"title":"A perfectly fine title","body":"","change_summary":"bad"}')
expect_status "422" "$CODE" "empty body -> 422"

# Missing title field -> Rocket JSON deserialization failure -> 422.
echo "[library_journal_validation] POST with missing title"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/journals" \
    -d '{"body":"some body here"}')
expect_status "422" "$CODE" "missing title -> 422"

# 2-character title -> below TITLE_MIN.
echo "[library_journal_validation] POST with short title"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/journals" \
    -d '{"title":"ab","body":"some body here"}')
expect_status "422" "$CODE" "2-char title -> 422"
