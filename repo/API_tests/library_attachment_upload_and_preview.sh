#!/usr/bin/env bash
# Attachment happy path: upload a small text file, fetch metadata + preview,
# assert checksum round-trip, then soft-delete.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for library_attachment_upload_and_preview.sh"
fi

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

BODY=$(mktemp)
UPLOAD_FILE=$(mktemp --suffix=.txt)
PREVIEW_OUT=$(mktemp)
PREVIEW_HEADERS=$(mktemp)
trap 'rm -f "$BODY" "$UPLOAD_FILE" "$PREVIEW_OUT" "$PREVIEW_HEADERS"' EXIT

# ── Create a parent journal so we have a place to hang the attachment ─────
TITLE="Attachment Parent $(date +%s)"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/journals" \
    -d "{\"title\":\"${TITLE}\",\"body\":\"parent for attachment test\"}")
expect_status "200" "$CODE" "create parent journal"
JOURNAL_ID=$(jq -r '.id' "$BODY")

# ── Write the upload file and compute the expected hash ───────────────────
# Exact content "hello scholarly" (no trailing newline) -> 15 bytes.
printf 'hello scholarly' > "$UPLOAD_FILE"
EXPECTED_SIZE=$(wc -c < "$UPLOAD_FILE" | tr -d ' ')
[ "$EXPECTED_SIZE" = "15" ] || fail "expected 15-byte upload file, got $EXPECTED_SIZE"
EXPECTED_SHA=$(sha256sum "$UPLOAD_FILE" | awk '{print $1}')

# ── Upload ─────────────────────────────────────────────────────────────────
echo "[library_attachment_upload_and_preview] POST /attachments"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/attachments" \
    -F "file=@${UPLOAD_FILE};type=text/plain" \
    -F "parent_type=journal" \
    -F "parent_id=${JOURNAL_ID}" \
    -F "category=sample_issue")
expect_status "200" "$CODE" "upload attachment"

ATTACHMENT_ID=$(jq -r '.id' "$BODY")
GOT_SHA=$(jq -r '.sha256_checksum' "$BODY")
GOT_SIZE=$(jq -r '.size_bytes' "$BODY")
GOT_PREVIEWABLE=$(jq -r '.is_previewable' "$BODY")
[ -n "$ATTACHMENT_ID" ] && [ "$ATTACHMENT_ID" != "null" ] || fail "no attachment id"
[ "$GOT_SHA" = "$EXPECTED_SHA" ] || fail "sha256 mismatch: expected $EXPECTED_SHA got $GOT_SHA"
[ "$GOT_SIZE" = "15" ] || fail "size_bytes should be 15, got $GOT_SIZE"
[ "$GOT_PREVIEWABLE" = "true" ] || fail "text/plain should be previewable"
pass "upload stored (id=${ATTACHMENT_ID}, sha=${GOT_SHA:0:12}...)"

# ── List for parent ────────────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/attachments?parent_type=journal&parent_id=${JOURNAL_ID}")
expect_status "200" "$CODE" "list attachments"
LEN=$(jq 'length' "$BODY")
[ "${LEN:-0}" -ge 1 ] || fail "expected >= 1 attachment in list, got $LEN"
pass "attachment list contains the upload"

# ── Get metadata ───────────────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/attachments/${ATTACHMENT_ID}")
expect_status "200" "$CODE" "get metadata"
[ "$(jq -r '.is_previewable' "$BODY")" = "true" ] || fail "metadata.is_previewable should be true"

# ── Preview ────────────────────────────────────────────────────────────────
echo "[library_attachment_upload_and_preview] GET /attachments/${ATTACHMENT_ID}/preview"
CODE=$(curl -s -o "$PREVIEW_OUT" -D "$PREVIEW_HEADERS" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/attachments/${ATTACHMENT_ID}/preview")
expect_status "200" "$CODE" "preview attachment"

PREVIEW_CONTENT=$(cat "$PREVIEW_OUT")
[ "$PREVIEW_CONTENT" = "hello scholarly" ] \
    || fail "preview body mismatch: got '$PREVIEW_CONTENT'"

# Case-insensitive header match for X-Attachment-Checksum.
CHECKSUM_HEADER=$(grep -iE '^X-Attachment-Checksum:' "$PREVIEW_HEADERS" \
    | tr -d '\r\n' | sed -E 's/^[Xx]-[Aa]ttachment-[Cc]hecksum:[[:space:]]*//')
EXPECTED_HEADER="sha256:${EXPECTED_SHA}"
[ "$CHECKSUM_HEADER" = "$EXPECTED_HEADER" ] \
    || fail "X-Attachment-Checksum mismatch: expected '$EXPECTED_HEADER' got '$CHECKSUM_HEADER'"
pass "preview bytes and checksum header match"

# ── Delete ─────────────────────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X DELETE "${BASE}/attachments/${ATTACHMENT_ID}")
expect_status "200" "$CODE" "delete attachment"

# ── After delete, GET returns 404 ──────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/attachments/${ATTACHMENT_ID}")
expect_status "404" "$CODE" "get after delete -> 404"
