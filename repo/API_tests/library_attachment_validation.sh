#!/usr/bin/env bash
# Attachment upload validation + auth negative cases.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for library_attachment_validation.sh"
fi

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

BODY=$(mktemp)
EVIL_FILE=$(mktemp --suffix=.sh)
trap 'rm -f "$BODY" "$EVIL_FILE"' EXIT

# ── Parent journal so that authorization passes far enough to hit validation
TITLE="Attachment Validation Parent $(date +%s)"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/journals" \
    -d "{\"title\":\"${TITLE}\",\"body\":\"validation parent\"}")
expect_status "200" "$CODE" "create parent journal"
JOURNAL_ID=$(jq -r '.id' "$BODY")

# ── Unsupported MIME type -> 422 ───────────────────────────────────────────
printf '#!/bin/sh\necho pwned\n' > "$EVIL_FILE"
echo "[library_attachment_validation] POST with application/x-shellscript"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/attachments" \
    -F "file=@${EVIL_FILE};type=application/x-shellscript" \
    -F "parent_type=journal" \
    -F "parent_id=${JOURNAL_ID}")
expect_status "422" "$CODE" "shellscript upload -> 422"

# ── Missing parent_type field -> 422 ───────────────────────────────────────
echo "[library_attachment_validation] POST missing parent_type"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/attachments" \
    -F "file=@${EVIL_FILE};type=text/plain" \
    -F "parent_id=${JOURNAL_ID}")
expect_status "422" "$CODE" "missing parent_type -> 422"

# ── Unauthenticated upload -> 401 ─────────────────────────────────────────
echo "[library_attachment_validation] POST unauthenticated"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -X POST "${BASE}/attachments" \
    -F "file=@${EVIL_FILE};type=text/plain" \
    -F "parent_type=journal" \
    -F "parent_id=${JOURNAL_ID}")
expect_status "401" "$CODE" "unauth upload -> 401"

# ── Viewer upload -> 403 ──────────────────────────────────────────────────
VIEWER_TOKEN=$(login_as "viewer@scholarly.local")
echo "[library_attachment_validation] POST as viewer"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${VIEWER_TOKEN}" \
    -X POST "${BASE}/attachments" \
    -F "file=@${EVIL_FILE};type=text/plain" \
    -F "parent_type=journal" \
    -F "parent_id=${JOURNAL_ID}")
expect_status "403" "$CODE" "viewer upload -> 403"
