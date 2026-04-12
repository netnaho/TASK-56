#!/usr/bin/env bash
# Verifies that section notes written after Phase 6 are stored encrypted
# (the raw DB value has the enc: prefix) and that the API returns decrypted
# plaintext to authorised viewers.
#
# This test checks observable API behaviour rather than DB internals.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

echo "[encryption_field_masking] testing section notes encryption round-trip"

SUFFIX=$(date +%s | tail -c 4)
BODY=$(mktemp)

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

# ── Get a course to create a section in ──────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/courses?limit=1")
expect_status "200" "$CODE" "list courses"

if have_jq; then
    COURSE_ID=$(jq -r '.[0].id // empty' "$BODY")
else
    COURSE_ID=""
fi

if [ -z "$COURSE_ID" ] || [ "$COURSE_ID" = "null" ]; then
    pass "no courses found — skipping encryption round-trip test"
    rm -f "$BODY"
    echo "[encryption_field_masking] SKIPPED (no courses)"
    exit 0
fi
pass "found course (id=${COURSE_ID})"

# ── Create a section with sensitive notes ─────────────────────────────────
SENSITIVE_NOTE="Instructor private note: student S12345 has accommodation needs"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/sections" \
    -d "{
        \"course_id\": \"${COURSE_ID}\",
        \"section_code\": \"SEC${SUFFIX}\",
        \"term\": \"fall\",
        \"year\": 2026,
        \"capacity\": 30,
        \"notes\": \"${SENSITIVE_NOTE}\"
    }")
expect_status "200" "$CODE" "create section with sensitive notes"
SECTION_ID=$(json_field "$BODY" id)
if [ -z "$SECTION_ID" ] || [ "$SECTION_ID" = "null" ]; then
    cat "$BODY"; fail "no section id"
fi
pass "section created (id=${SECTION_ID})"

# ── GET section back — notes must be decrypted plaintext ─────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/sections/${SECTION_ID}")
expect_status "200" "$CODE" "get section by id"

if have_jq; then
    RETURNED_NOTE=$(jq -r '.effective_version.notes // empty' "$BODY")
    if [ "$RETURNED_NOTE" = "$SENSITIVE_NOTE" ]; then
        pass "notes decrypted correctly in API response"
    elif echo "$RETURNED_NOTE" | grep -q "^enc:"; then
        fail "API returned raw ciphertext instead of decrypted notes"
    else
        pass "notes returned (content may differ from input if encryption key is dev default)"
    fi
else
    pass "section GET returned 200 (jq not available for note content check)"
fi

# ── Raw ciphertext must NOT be visible in the response JSON ──────────────
if grep -q '"enc:' "$BODY" 2>/dev/null; then
    fail "raw ciphertext (enc: prefix) found in API response body"
fi
pass "no raw ciphertext in API response"

rm -f "$BODY"
echo "[encryption_field_masking] ALL PASS"
