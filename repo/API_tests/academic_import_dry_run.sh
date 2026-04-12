#!/usr/bin/env bash
# Phase 4 — course import dry-run: good row + bad row should report
# error_rows=1, valid_rows=1, committed=false, and must NOT write any new
# rows to the DB.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for academic_import_dry_run.sh"
fi

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

BODY=$(mktemp)
CSV_FILE=$(mktemp --suffix=.csv)
trap 'rm -f "$BODY" "$CSV_FILE"' EXIT

# ── Build the CSV in place ────────────────────────────────────────────────
cat > "$CSV_FILE" <<'EOF'
code,title,department_code,credit_hours,contact_hours,description,prerequisites
GOOD-101,Good Course,CS,3,3,desc,
cs-101,Bad Case,CS,abc,3,bad,
EOF

# ── Dry-run the import ────────────────────────────────────────────────────
echo "[academic_import_dry_run] POST /courses/import?mode=dry_run"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/courses/import?mode=dry_run" \
    -F "file=@${CSV_FILE};type=text/csv" \
    -F "mode=dry_run")
expect_status "200" "$CODE" "dry_run import"

COMMITTED=$(jq -r '.committed' "$BODY")
VALID=$(jq -r '.valid_rows' "$BODY")
ERRS=$(jq -r '.error_rows' "$BODY")
TOTAL=$(jq -r '.total_rows' "$BODY")
[ "$COMMITTED" = "false" ] || fail "committed should be false, got $COMMITTED"
[ "$VALID" = "1" ] || fail "valid_rows should be 1, got $VALID"
[ "$ERRS" = "1" ] || fail "error_rows should be 1, got $ERRS"
[ "$TOTAL" = "2" ] || fail "total_rows should be 2, got $TOTAL"
pass "dry_run report counts correct (total=2, valid=1, errors=1)"

# The bad row is the second data row — row_index 3 in the spreadsheet-style
# numbering (header is row 1, GOOD-101 is row 2).
HAS_CREDIT_HOURS_ERR=$(jq '[.rows[] | select(.ok==false) | .errors[] | select(.field=="credit_hours")] | length' "$BODY")
[ "${HAS_CREDIT_HOURS_ERR:-0}" -ge 1 ] \
    || fail "expected an errors[].field=credit_hours on the bad row"
pass "bad row carries credit_hours field error"

# ── Confirm the DB did NOT pick up GOOD-101 ───────────────────────────────
echo "[academic_import_dry_run] GET /courses and assert GOOD-101 absent"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/courses?limit=200")
expect_status "200" "$CODE" "list courses"

EXISTS=$(jq '[.[] | select(.code=="GOOD-101")] | length' "$BODY")
[ "$EXISTS" = "0" ] || fail "dry_run must not create rows; GOOD-101 exists"
pass "GOOD-101 was not persisted"
