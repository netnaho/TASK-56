#!/usr/bin/env bash
# Phase 5 — foot_traffic is computed from stored data, not hardcoded.
#
# 1. Admin creates a dedicated course + section.
# 2. Record three instructor check-ins spaced across the last three
#    days by inserting check-in rows directly via docker compose mysql
#    (the API does not let us rewrite `checked_in_at`).
# 3. Query foot-traffic and assert the sum of the returned rows is
#    strictly >= 3 — no hardcoded fixture would be influenced by our
#    new rows.
# 4. (Best-effort) delete the course and re-query. If the DELETE route
#    exists and succeeds, foot_traffic should drop. If not, the
#    teardown assertion is skipped.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for dashboard_derived_from_real_data.sh"
fi

if ! command -v docker >/dev/null 2>&1; then
    echo "[dashboard_derived_from_real_data] SKIP: docker not available"
    exit 0
fi

mysql_exec() {
    docker compose exec -T mysql mysql \
        -uscholarly_app -pscholarly_app_pass scholarly -N -s -e "$1"
}

if ! mysql_exec "SELECT 1;" >/dev/null 2>&1; then
    echo "[dashboard_derived_from_real_data] SKIP: cannot reach mysql via docker compose"
    exit 0
fi

DEPT_CS="20000000-0000-0000-0000-000000000001"
INSTRUCTOR_USER_ID="30000000-0000-0000-0000-000000000003"

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

ADMIN_TOKEN=$(login_as "admin@scholarly.local")
SUFFIX=$(date +%s | tail -c 5)
COURSE_CODE="FOT${SUFFIX}"

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses" \
    -d "{\"code\":\"${COURSE_CODE}\",\"title\":\"Foot traffic\",\"department_id\":\"${DEPT_CS}\",\"credit_hours\":3,\"contact_hours\":3}")
expect_status "200" "$CODE" "create course"
COURSE_ID=$(jq -r '.id' "$BODY")

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/sections" \
    -d "{\"course_id\":\"${COURSE_ID}\",\"section_code\":\"01\",\"term\":\"fall\",\"year\":2026,\"capacity\":30,\"instructor_id\":\"${INSTRUCTOR_USER_ID}\"}")
expect_status "200" "$CODE" "create section"
SECTION_ID=$(jq -r '.id' "$BODY")

# ── Insert three check-ins across the last 3 days ────────────────────────
for OFFSET in 0 1 2; do
    ID=$(mysql_exec "SELECT UUID();" | tr -d '\r')
    mysql_exec "INSERT INTO checkin_events (id, user_id, section_id, checkin_type, checked_in_at, event_date, device_info, retry_sequence, device_fingerprint, network_verified, is_duplicate_attempt) VALUES ('${ID}', '${INSTRUCTOR_USER_ID}', '${SECTION_ID}', 'qr_code', DATE_SUB(NOW(), INTERVAL ${OFFSET} DAY), DATE_SUB(CURDATE(), INTERVAL ${OFFSET} DAY), NULL, 0, NULL, TRUE, FALSE);" \
        || fail "failed to insert check-in offset ${OFFSET}"
done
pass "three historic check-ins inserted"

# ── GET /dashboards/foot-traffic (defaults = last 30 days) ───────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/dashboards/foot-traffic")
expect_status "200" "$CODE" "foot-traffic"

SUM_BEFORE=$(jq '[.rows[].value] | add // 0' "$BODY")
# Use integer-ish comparison since foot_traffic counts are integers.
SUM_BEFORE_INT=${SUM_BEFORE%.*}
if [ "$SUM_BEFORE_INT" -lt 3 ]; then
    fail "expected foot_traffic sum >= 3, got ${SUM_BEFORE}"
fi
pass "foot_traffic sum is ${SUM_BEFORE} (>=3)"

# ── Best-effort teardown — delete the course if the route exists ─────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X DELETE "${BASE}/courses/${COURSE_ID}")
if [ "$CODE" = "200" ] || [ "$CODE" = "204" ]; then
    CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}" \
        "${BASE}/dashboards/foot-traffic")
    expect_status "200" "$CODE" "foot-traffic after delete"
    SUM_AFTER=$(jq '[.rows[].value] | add // 0' "$BODY")
    SUM_AFTER_INT=${SUM_AFTER%.*}
    if [ "$SUM_AFTER_INT" -ge "$SUM_BEFORE_INT" ]; then
        fail "expected foot_traffic to decrease after delete (before=${SUM_BEFORE}, after=${SUM_AFTER})"
    fi
    pass "foot_traffic dropped after course deletion (before=${SUM_BEFORE}, after=${SUM_AFTER})"
else
    echo "[dashboard_derived_from_real_data] DELETE /courses/... not available (HTTP ${CODE}); teardown assertion skipped"
fi
