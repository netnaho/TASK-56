#!/usr/bin/env bash
# Phase 5 — unauthenticated POST /checkins -> 401, viewer POST -> 403.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

# Any section id will do — the capability guard runs before section lookup.
DUMMY_SECTION="00000000-0000-0000-0000-000000000001"

# ── Unauthenticated → 401 ────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/checkins" \
    -d "{\"section_id\":\"${DUMMY_SECTION}\",\"checkin_type\":\"qr_code\"}")
expect_status "401" "$CODE" "anon POST /checkins -> 401"

# ── Viewer → 403 ─────────────────────────────────────────────────────────
VIEWER_TOKEN=$(login_as "viewer@scholarly.local")
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${VIEWER_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/checkins" \
    -d "{\"section_id\":\"${DUMMY_SECTION}\",\"checkin_type\":\"qr_code\"}")
expect_status "403" "$CODE" "viewer POST /checkins -> 403"
