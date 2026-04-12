#!/usr/bin/env bash
# At most one published version per journal: publishing v3 must archive v2.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for library_publish_baseline_invariant.sh"
fi

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

# ── Create the journal ──────────────────────────────────────────────────────
TITLE="Baseline Invariant Probe $(date +%s)"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/journals" \
    -d "{\"title\":\"${TITLE}\",\"body\":\"v1 body\"}")
expect_status "200" "$CODE" "create journal"
JOURNAL_ID=$(jq -r '.id' "$BODY")

# ── PUT -> v2, approve, publish ────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X PUT "${BASE}/journals/${JOURNAL_ID}" \
    -d '{"body":"v2 body"}')
expect_status "200" "$CODE" "put v2"
V2_ID=$(jq -r '.id' "$BODY")

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/journals/${JOURNAL_ID}/versions/${V2_ID}/approve")
expect_status "200" "$CODE" "approve v2"

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/journals/${JOURNAL_ID}/versions/${V2_ID}/publish")
expect_status "200" "$CODE" "publish v2"
IS_PUB=$(jq -r '.is_published' "$BODY")
[ "$IS_PUB" = "true" ] || fail "after publish v2, is_published should be true"

# ── PUT -> v3, approve, publish ────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X PUT "${BASE}/journals/${JOURNAL_ID}" \
    -d '{"body":"v3 body"}')
expect_status "200" "$CODE" "put v3"
V3_ID=$(jq -r '.id' "$BODY")
V3_NUM=$(jq -r '.version_number' "$BODY")
[ "$V3_NUM" = "3" ] || fail "expected v3 version_number=3, got $V3_NUM"

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/journals/${JOURNAL_ID}/versions/${V3_ID}/approve")
expect_status "200" "$CODE" "approve v3"

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/journals/${JOURNAL_ID}/versions/${V3_ID}/publish")
expect_status "200" "$CODE" "publish v3"

IS_PUB=$(jq -r '.is_published' "$BODY")
EV_NUM=$(jq -r '.effective_version.version_number' "$BODY")
[ "$IS_PUB" = "true" ] || fail "after publish v3, is_published should be true"
[ "$EV_NUM" = "3" ] || fail "effective_version.version_number should be 3, got $EV_NUM"

# ── Exactly one published version; v2 should be archived ──────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/journals/${JOURNAL_ID}/versions")
expect_status "200" "$CODE" "list versions"

PUB_COUNT=$(jq 'map(select(.state=="published")) | length' "$BODY")
[ "$PUB_COUNT" = "1" ] || fail "expected exactly one published version, got $PUB_COUNT"

V2_STATE=$(jq -r '[.[] | select(.version_number==2)][0].state' "$BODY")
[ "$V2_STATE" = "archived" ] || fail "v2 should be archived after publishing v3, got $V2_STATE"

V3_STATE=$(jq -r '[.[] | select(.version_number==3)][0].state' "$BODY")
[ "$V3_STATE" = "published" ] || fail "v3 should be the published baseline, got $V3_STATE"

pass "baseline invariant holds: v3 published, v2 archived, v1 retained"
