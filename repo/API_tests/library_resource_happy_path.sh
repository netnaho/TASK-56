#!/usr/bin/env bash
# Teaching-resource happy path: create -> v2 draft -> approve -> publish.
# Mirrors library_journal_happy_path.sh.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for library_resource_happy_path.sh"
fi

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

TITLE="Phase3 Resource Happy Path $(date +%s)"

# ── Create the resource ────────────────────────────────────────────────────
echo "[library_resource_happy_path] POST /teaching-resources"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/teaching-resources" \
    -d "{\"title\":\"${TITLE}\",\"resource_type\":\"document\",\"description\":\"an initial description\",\"change_summary\":\"create\"}")
expect_status "200" "$CODE" "create resource"

RESOURCE_ID=$(jq -r '.id' "$BODY")
V1_NUM=$(jq -r '.effective_version.version_number' "$BODY")
V1_STATE=$(jq -r '.effective_version.state' "$BODY")
[ -n "$RESOURCE_ID" ] && [ "$RESOURCE_ID" != "null" ] || fail "no resource id"
[ "$V1_NUM" = "1" ] || fail "expected v1.version_number=1, got $V1_NUM"
[ "$V1_STATE" = "draft" ] || fail "expected v1.state=draft, got $V1_STATE"
pass "resource created (id=${RESOURCE_ID}) with draft v1"

# ── GET ────────────────────────────────────────────────────────────────────
echo "[library_resource_happy_path] GET /teaching-resources/${RESOURCE_ID}"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/teaching-resources/${RESOURCE_ID}")
expect_status "200" "$CODE" "get resource"

[ "$(jq -r '.effective_version.version_number' "$BODY")" = "1" ] || fail "get: wrong version_number"
[ "$(jq -r '.effective_version.state' "$BODY")" = "draft" ] || fail "get: wrong state"

# ── PUT creates v2 ─────────────────────────────────────────────────────────
echo "[library_resource_happy_path] PUT /teaching-resources/${RESOURCE_ID}"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X PUT "${BASE}/teaching-resources/${RESOURCE_ID}" \
    -d '{"description":"updated description for v2","change_summary":"edit"}')
expect_status "200" "$CODE" "put v2"

V2_ID=$(jq -r '.id' "$BODY")
V2_NUM=$(jq -r '.version_number' "$BODY")
V2_STATE=$(jq -r '.state' "$BODY")
[ "$V2_NUM" = "2" ] || fail "put: expected version_number=2, got $V2_NUM"
[ "$V2_STATE" = "draft" ] || fail "put: expected state=draft, got $V2_STATE"
pass "draft v2 created (version=${V2_ID})"

# ── Approve v2 ─────────────────────────────────────────────────────────────
echo "[library_resource_happy_path] approve v2"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/teaching-resources/${RESOURCE_ID}/versions/${V2_ID}/approve")
expect_status "200" "$CODE" "approve v2"
[ "$(jq -r '.state' "$BODY")" = "approved" ] || fail "approve: state should be approved"

# ── Publish v2 ─────────────────────────────────────────────────────────────
echo "[library_resource_happy_path] publish v2"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/teaching-resources/${RESOURCE_ID}/versions/${V2_ID}/publish")
expect_status "200" "$CODE" "publish v2"

IS_PUB=$(jq -r '.is_published' "$BODY")
EV_NUM=$(jq -r '.effective_version.version_number' "$BODY")
EV_STATE=$(jq -r '.effective_version.state' "$BODY")
[ "$IS_PUB" = "true" ] || fail "is_published should be true"
[ "$EV_NUM" = "2" ] || fail "effective_version.version_number should be 2, got $EV_NUM"
[ "$EV_STATE" = "published" ] || fail "effective_version.state should be published"
pass "resource baseline moved to v2"
