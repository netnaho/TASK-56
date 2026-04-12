#!/usr/bin/env bash
# A non-admin token must be rejected from admin-only endpoints with 403.
# An admin token must be accepted with 200.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

# Instructor account has no admin capability in the Phase 2 matrix.
INSTRUCTOR_TOKEN=$(login_as "instructor@scholarly.local")

CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer ${INSTRUCTOR_TOKEN}" \
    "${BASE}/admin/config")
expect_status "403" "$CODE" "instructor -> /admin/config -> 403"

CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer ${INSTRUCTOR_TOKEN}" \
    "${BASE}/audit-logs/verify-chain")
expect_status "403" "$CODE" "instructor -> /audit-logs/verify-chain -> 403"

ADMIN_TOKEN=$(login_as "admin@scholarly.local")
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/admin/config")
expect_status "200" "$CODE" "admin -> /admin/config -> 200"

CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/audit-logs/verify-chain")
expect_status "200" "$CODE" "admin -> /audit-logs/verify-chain -> 200"
