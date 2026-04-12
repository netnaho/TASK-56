#!/usr/bin/env bash
# Uploading an allowed-but-not-previewable MIME type (.docx) should store
# fine, mark is_previewable=false, and refuse preview with 422.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for library_preview_unsupported_type.sh"
fi

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

BODY=$(mktemp)
DOCX_FILE=$(mktemp --suffix=.docx)
trap 'rm -f "$BODY" "$DOCX_FILE"' EXIT

# ── Create parent journal ──────────────────────────────────────────────────
TITLE="Preview Unsupported Parent $(date +%s)"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/journals" \
    -d "{\"title\":\"${TITLE}\",\"body\":\"parent for non-previewable upload\"}")
expect_status "200" "$CODE" "create parent journal"
JOURNAL_ID=$(jq -r '.id' "$BODY")

# ── Upload a tiny .docx (dummy bytes) with the real docx mime ──────────────
# A .docx is a ZIP; we just need valid bytes — the server only checks the
# declared MIME against its whitelist. The preview path is mime-gated, so
# we never decode the content.
printf 'PK\x03\x04dummy-docx-content' > "$DOCX_FILE"

DOCX_MIME="application/vnd.openxmlformats-officedocument.wordprocessingml.document"

echo "[library_preview_unsupported_type] upload .docx"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/attachments" \
    -F "file=@${DOCX_FILE};type=${DOCX_MIME}" \
    -F "parent_type=journal" \
    -F "parent_id=${JOURNAL_ID}" \
    -F "category=contract")
expect_status "200" "$CODE" "upload .docx"

ATTACHMENT_ID=$(jq -r '.id' "$BODY")
PREVIEWABLE=$(jq -r '.is_previewable' "$BODY")
[ -n "$ATTACHMENT_ID" ] && [ "$ATTACHMENT_ID" != "null" ] || fail "no attachment id"
[ "$PREVIEWABLE" = "false" ] \
    || fail "docx should not be previewable, got $PREVIEWABLE"
pass "docx uploaded and correctly marked not previewable"

# ── Metadata GET confirms is_previewable=false ─────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/attachments/${ATTACHMENT_ID}")
expect_status "200" "$CODE" "metadata GET"
[ "$(jq -r '.is_previewable' "$BODY")" = "false" ] \
    || fail "metadata.is_previewable should be false"

# ── Preview request must be rejected with 422 ─────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/attachments/${ATTACHMENT_ID}/preview")
expect_status "422" "$CODE" "preview of docx -> 422"

# The error envelope should explicitly mention preview being unavailable.
if ! grep -qi 'preview not available' "$BODY"; then
    cat "$BODY"
    fail "error body should mention 'preview not available'"
fi
pass "preview endpoint refuses non-previewable mime"
