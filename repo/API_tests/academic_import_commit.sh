#!/usr/bin/env bash
# Phase 4 — course import commit path: a fully-valid CSV commits rows;
# a CSV with any bad row rejects the whole batch (all-or-nothing).
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

if ! have_jq; then
    fail "jq is required for academic_import_commit.sh"
fi

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

BODY=$(mktemp)
CSV_OK=$(mktemp --suffix=.csv)
CSV_MIXED=$(mktemp --suffix=.csv)
trap 'rm -f "$BODY" "$CSV_OK" "$CSV_MIXED"' EXIT

# Unique-per-run codes so reruns don't hit the duplicate-code guard.
SUFFIX=$(date +%s | tail -c 5)
CODE_A="IMPA${SUFFIX}"
CODE_B="IMPB${SUFFIX}"

# ── CSV with two valid rows ───────────────────────────────────────────────
cat > "$CSV_OK" <<EOF
code,title,department_code,credit_hours,contact_hours,description,prerequisites
${CODE_A},Import A,CS,3,3,desc A,
${CODE_B},Import B,CS,3,3,desc B,
EOF

echo "[academic_import_commit] POST /courses/import?mode=commit (all-valid)"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/courses/import?mode=commit" \
    -F "file=@${CSV_OK};type=text/csv" \
    -F "mode=commit")
expect_status "200" "$CODE" "commit import"

COMMITTED=$(jq -r '.committed' "$BODY")
VALID=$(jq -r '.valid_rows' "$BODY")
ERRS=$(jq -r '.error_rows' "$BODY")
[ "$COMMITTED" = "true" ] || fail "committed should be true, got $COMMITTED"
[ "$VALID" = "2" ] || fail "valid_rows should be 2, got $VALID"
[ "$ERRS" = "0" ] || fail "error_rows should be 0, got $ERRS"
pass "commit report says both rows persisted"

# ── List and assert both codes exist ──────────────────────────────────────
echo "[academic_import_commit] GET /courses and assert both codes exist"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/courses?limit=500")
expect_status "200" "$CODE" "list courses after commit"

HAS_A=$(jq --arg c "$CODE_A" '[.[] | select(.code==$c)] | length' "$BODY")
HAS_B=$(jq --arg c "$CODE_B" '[.[] | select(.code==$c)] | length' "$BODY")
[ "$HAS_A" = "1" ] || fail "expected ${CODE_A} to exist after commit, got $HAS_A"
[ "$HAS_B" = "1" ] || fail "expected ${CODE_B} to exist after commit, got $HAS_B"
pass "both imported courses are listed"

# ── Mixed CSV: 1 valid + 1 invalid row → 422, nothing persists ───────────
MIX_OK="MIXA${SUFFIX}"
# Lowercase code fails is_valid_course_code.
MIX_BAD="mixb${SUFFIX}"
cat > "$CSV_MIXED" <<EOF
code,title,department_code,credit_hours,contact_hours,description,prerequisites
${MIX_OK},Mixed valid,CS,3,3,,
${MIX_BAD},Mixed invalid,CS,3,3,,
EOF

echo "[academic_import_commit] POST /courses/import?mode=commit (mixed rows)"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/courses/import?mode=commit" \
    -F "file=@${CSV_MIXED};type=text/csv" \
    -F "mode=commit")
expect_status "422" "$CODE" "mixed commit -> 422"

# Neither code should appear in the DB.
echo "[academic_import_commit] verify neither row persisted"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/courses?limit=500")
expect_status "200" "$CODE" "list courses after failed commit"

HAS_OK=$(jq --arg c "$MIX_OK" '[.[] | select(.code==$c)] | length' "$BODY")
HAS_BAD=$(jq --arg c "$MIX_BAD" '[.[] | select(.code==$c)] | length' "$BODY")
[ "$HAS_OK" = "0" ] || fail "all-or-nothing violated: ${MIX_OK} was persisted"
[ "$HAS_BAD" = "0" ] || fail "bad row was persisted: ${MIX_BAD}"
pass "all-or-nothing: neither mixed row persisted"
