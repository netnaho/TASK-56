#!/usr/bin/env bash
# Library journal happy-path: create -> draft v2 -> approve -> publish.
# Asserts the draft/approve/publish state machine and the effective_version
# projection returned by the API.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

# ── Create a new journal ────────────────────────────────────────────────────
TITLE="Phase3 Happy Path Journal $(date +%s)"
echo "[library_journal_happy_path] POST ${BASE}/journals"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/journals" \
    -d "{\"title\":\"${TITLE}\",\"body\":\"Initial body text for the journal.\",\"change_summary\":\"create\"}")
expect_status "200" "$CODE" "create journal"

if ! have_jq; then
    fail "jq is required for library_journal_happy_path.sh"
fi

JOURNAL_ID=$(jq -r '.id' "$BODY")
V1_STATE=$(jq -r '.effective_version.state' "$BODY")
V1_NUM=$(jq -r '.effective_version.version_number' "$BODY")
if [ -z "$JOURNAL_ID" ] || [ "$JOURNAL_ID" = "null" ]; then
    cat "$BODY"
    fail "no journal id in create response"
fi
[ "$V1_NUM" = "1" ] || fail "expected v1.version_number=1, got $V1_NUM"
[ "$V1_STATE" = "draft" ] || fail "expected v1.state=draft, got $V1_STATE"
pass "create returned v1 draft (journal=${JOURNAL_ID})"

# ── GET the journal back ────────────────────────────────────────────────────
echo "[library_journal_happy_path] GET ${BASE}/journals/${JOURNAL_ID}"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/journals/${JOURNAL_ID}")
expect_status "200" "$CODE" "get journal"

GOT_NUM=$(jq -r '.effective_version.version_number' "$BODY")
GOT_STATE=$(jq -r '.effective_version.state' "$BODY")
[ "$GOT_NUM" = "1" ] || fail "get: expected version_number=1, got $GOT_NUM"
[ "$GOT_STATE" = "draft" ] || fail "get: expected state=draft, got $GOT_STATE"
pass "get returned draft v1"

# ── PUT creates v2 ──────────────────────────────────────────────────────────
echo "[library_journal_happy_path] PUT ${BASE}/journals/${JOURNAL_ID}"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X PUT "${BASE}/journals/${JOURNAL_ID}" \
    -d '{"body":"Revised body for v2","change_summary":"edit"}')
expect_status "200" "$CODE" "put (draft v2)"

V2_ID=$(jq -r '.id' "$BODY")
V2_NUM=$(jq -r '.version_number' "$BODY")
V2_STATE=$(jq -r '.state' "$BODY")
[ "$V2_NUM" = "2" ] || fail "put: expected version_number=2, got $V2_NUM"
[ "$V2_STATE" = "draft" ] || fail "put: expected state=draft, got $V2_STATE"
pass "draft v2 created (version_id=${V2_ID})"

# ── Approve v2 ──────────────────────────────────────────────────────────────
echo "[library_journal_happy_path] POST approve v2"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/journals/${JOURNAL_ID}/versions/${V2_ID}/approve")
expect_status "200" "$CODE" "approve v2"
APPROVED_STATE=$(jq -r '.state' "$BODY")
[ "$APPROVED_STATE" = "approved" ] || fail "expected state=approved, got $APPROVED_STATE"
pass "v2 approved"

# ── Publish v2 ──────────────────────────────────────────────────────────────
echo "[library_journal_happy_path] POST publish v2"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/journals/${JOURNAL_ID}/versions/${V2_ID}/publish")
expect_status "200" "$CODE" "publish v2"

IS_PUBLISHED=$(jq -r '.is_published' "$BODY")
PUB_NUM=$(jq -r '.effective_version.version_number' "$BODY")
[ "$IS_PUBLISHED" = "true" ] || fail "expected is_published=true, got $IS_PUBLISHED"
[ "$PUB_NUM" = "2" ] || fail "expected effective_version.version_number=2, got $PUB_NUM"
pass "journal baseline moved to v2"

# ── Version list retains v1 ────────────────────────────────────────────────
echo "[library_journal_happy_path] GET versions list"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/journals/${JOURNAL_ID}/versions")
expect_status "200" "$CODE" "list versions"

LEN=$(jq 'length' "$BODY")
if [ "${LEN:-0}" -lt 2 ]; then
    cat "$BODY"
    fail "expected >= 2 versions, got $LEN"
fi
HAS_V1=$(jq '[.[] | select(.version_number==1)] | length' "$BODY")
[ "$HAS_V1" -ge 1 ] || fail "v1 was not retained after later edits"
pass "version list retains v1 (len=$LEN)"
