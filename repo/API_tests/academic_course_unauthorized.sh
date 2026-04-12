#!/usr/bin/env bash
# Phase 4 — course authorization: viewer role cannot write courses,
# anonymous caller must authenticate.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

# CS department — seeds/002_seed_departments.sql
DEPT_CS="20000000-0000-0000-0000-000000000001"

VIEWER_TOKEN=$(login_as "viewer@scholarly.local")

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

# ── Authenticated viewer cannot create courses → 403 ──────────────────────
echo "[academic_course_unauthorized] viewer POST /courses"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${VIEWER_TOKEN}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses" \
    -d "{\"code\":\"UNA101\",\"title\":\"Viewer attempt\",\"department_id\":\"${DEPT_CS}\",\"credit_hours\":3,\"contact_hours\":3}")
expect_status "403" "$CODE" "viewer create -> 403"

# ── Unauthenticated caller → 401 ──────────────────────────────────────────
echo "[academic_course_unauthorized] anon POST /courses"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/courses" \
    -d "{\"code\":\"UNA102\",\"title\":\"Anon attempt\",\"department_id\":\"${DEPT_CS}\",\"credit_hours\":3,\"contact_hours\":3}")
expect_status "401" "$CODE" "anon create -> 401"
