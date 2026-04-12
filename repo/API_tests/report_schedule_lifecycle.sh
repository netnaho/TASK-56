#!/usr/bin/env bash
# Report schedule CRUD: create, list, update, delete.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

echo "[report_schedule_lifecycle] testing report schedule CRUD"

BODY=$(mktemp)

ADMIN_TOKEN=$(login_as "admin@scholarly.local")
VIEWER_TOKEN=$(login_as "viewer@scholarly.local")

# ── Create a report to schedule ──────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/reports" \
    -d '{
        "title": "Schedule Test Report",
        "query_definition": {"report_type": "checkin_activity", "filters": {}},
        "default_format": "csv"
    }')
expect_status "200" "$CODE" "create report for scheduling"
REPORT_ID=$(json_field "$BODY" id)
pass "report created (${REPORT_ID})"

# ── Create a schedule ────────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/reports/${REPORT_ID}/schedules" \
    -d '{
        "cron_expression": "0 0 7 * * Mon *",
        "format": "csv",
        "is_active": true
    }')
expect_status "200" "$CODE" "create schedule (every Monday 07:00)"
SCHED_ID=$(json_field "$BODY" id)
NEXT_RUN=$(json_field "$BODY" next_run_at)
if [ -z "$SCHED_ID" ] || [ "$SCHED_ID" = "null" ]; then
    cat "$BODY"; fail "no schedule id"
fi
pass "schedule created (id=${SCHED_ID}, next_run_at=${NEXT_RUN})"

# ── Invalid cron expression → 422 ────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/reports/${REPORT_ID}/schedules" \
    -d '{"cron_expression": "not a valid cron", "format": "csv"}')
expect_status "422" "$CODE" "invalid cron expression rejected"

# ── List schedules ────────────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/reports/${REPORT_ID}/schedules")
expect_status "200" "$CODE" "list schedules"
pass "list schedules OK"

# ── Viewer cannot create schedule ────────────────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${VIEWER_TOKEN}" \
    -X POST "${BASE}/reports/${REPORT_ID}/schedules" \
    -d '{"cron_expression": "0 0 7 * * Mon *", "format": "csv"}')
expect_status "403" "$CODE" "viewer cannot create schedule"

# ── Update schedule (disable) ────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X PUT "${BASE}/reports/schedules/${SCHED_ID}" \
    -d '{"is_active": false}')
expect_status "200" "$CODE" "disable schedule"
if have_jq; then
    IS_ACTIVE=$(jq -r '.is_active' "$BODY")
    if [ "$IS_ACTIVE" = "false" ]; then
        pass "schedule disabled"
    else
        fail "is_active should be false after disable"
    fi
else
    pass "schedule update returned 200 (jq not available)"
fi

# ── Delete schedule ───────────────────────────────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X DELETE "${BASE}/reports/schedules/${SCHED_ID}")
expect_status "200" "$CODE" "delete schedule"

# ── Deleted schedule not found ───────────────────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/reports/schedules/${SCHED_ID}")
expect_status "404" "$CODE" "deleted schedule not found"

rm -f "$BODY"
echo "[report_schedule_lifecycle] ALL PASS"
