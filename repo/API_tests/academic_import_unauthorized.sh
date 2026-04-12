#!/usr/bin/env bash
# Phase 4 — course import authorization: viewer role cannot import,
# unauthenticated caller must authenticate.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

VIEWER_TOKEN=$(login_as "viewer@scholarly.local")

BODY=$(mktemp)
CSV_FILE=$(mktemp --suffix=.csv)
trap 'rm -f "$BODY" "$CSV_FILE"' EXIT

cat > "$CSV_FILE" <<'EOF'
code,title,department_code,credit_hours,contact_hours,description,prerequisites
UNA101,Unauthorized attempt,CS,3,3,,
EOF

# ── Authenticated viewer → 403 ────────────────────────────────────────────
echo "[academic_import_unauthorized] viewer POST /courses/import"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${VIEWER_TOKEN}" \
    -X POST "${BASE}/courses/import?mode=dry_run" \
    -F "file=@${CSV_FILE};type=text/csv" \
    -F "mode=dry_run")
expect_status "403" "$CODE" "viewer import -> 403"

# ── Unauthenticated → 401 ─────────────────────────────────────────────────
echo "[academic_import_unauthorized] anon POST /courses/import"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -X POST "${BASE}/courses/import?mode=dry_run" \
    -F "file=@${CSV_FILE};type=text/csv" \
    -F "mode=dry_run")
expect_status "401" "$CODE" "anon import -> 401"
