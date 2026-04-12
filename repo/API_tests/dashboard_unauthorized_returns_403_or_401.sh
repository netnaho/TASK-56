#!/usr/bin/env bash
# Phase 5 — unauthenticated GET /dashboards/course-popularity -> 401.
# Viewer has DashboardRead in the capability matrix, so we do NOT
# assert 403 for them — this test only covers the anonymous path.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

BODY=$(mktemp)
trap 'rm -f "$BODY"' EXIT

CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    "${BASE}/dashboards/course-popularity")
expect_status "401" "$CODE" "anon GET /dashboards/course-popularity -> 401"
