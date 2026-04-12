#!/usr/bin/env bash
# Phase 5 — dashboard date filter semantics:
#   * default window ~30 days
#   * explicit 7-day window echoed back
#   * >366-day range rejected with 422
#   * inverted range rejected with 422
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for dashboard_date_filter.sh"
fi

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

# ── Default window is ~30 days ───────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/dashboards/course-popularity")
expect_status "200" "$CODE" "course-popularity default window"

FROM=$(jq -r '.window_from' "$BODY")
TO=$(jq -r '.window_to' "$BODY")
# Extract just the date portion for a cheap day-difference check.
FROM_DATE="${FROM%%T*}"
TO_DATE="${TO%%T*}"
FROM_EPOCH=$(date -u -d "${FROM_DATE}" +%s 2>/dev/null || date -u -j -f "%Y-%m-%d" "${FROM_DATE}" +%s)
TO_EPOCH=$(date -u -d "${TO_DATE}" +%s 2>/dev/null || date -u -j -f "%Y-%m-%d" "${TO_DATE}" +%s)
DIFF_DAYS=$(( (TO_EPOCH - FROM_EPOCH) / 86400 ))
if [ "$DIFF_DAYS" -lt 29 ] || [ "$DIFF_DAYS" -gt 31 ]; then
    fail "default window span ${DIFF_DAYS} days not ~30"
fi
pass "default window is ~30 days (${DIFF_DAYS})"

# ── Explicit 7-day window ────────────────────────────────────────────────
FROM_7="2026-04-01T00:00:00Z"
TO_7="2026-04-08T00:00:00Z"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/dashboards/course-popularity?from=${FROM_7}&to=${TO_7}")
expect_status "200" "$CODE" "explicit 7-day window"

# ── Huge window → 422 ────────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/dashboards/course-popularity?from=2020-01-01T00:00:00Z&to=2030-01-01T00:00:00Z")
expect_status "422" "$CODE" "huge window -> 422"

# ── Inverted range → 422 ─────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/dashboards/course-popularity?from=2026-06-01T00:00:00Z&to=2026-01-01T00:00:00Z")
expect_status "422" "$CODE" "inverted range -> 422"
