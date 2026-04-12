#!/usr/bin/env bash
# User CRUD — create, read, update, deactivate.
# All mutating operations require Admin; viewer/instructor must be rejected.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

echo "[user_crud] testing user management API"

BODY=$(mktemp)

ADMIN_TOKEN=$(login_as "admin@scholarly.local")
VIEWER_TOKEN=$(login_as "viewer@scholarly.local")

# ── GET /me ──────────────────────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/users/me")
expect_status "200" "$CODE" "GET /users/me"

if have_jq; then
    EMAIL=$(jq -r '.email // empty' "$BODY")
    if echo "$EMAIL" | grep -q "@"; then
        pass "me returns email: ${EMAIL}"
    else
        fail "me response missing email"
    fi
fi

# ── GET /users (admin) ────────────────────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/users")
expect_status "200" "$CODE" "list users (admin)"

# ── GET /users (viewer → 403) ────────────────────────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer ${VIEWER_TOKEN}" \
    "${BASE}/users")
expect_status "403" "$CODE" "list users (viewer forbidden)"

# ── POST /users — create new user ─────────────────────────────────────────────
TS=$(date +%s)
NEW_EMAIL="api_test_user_${TS}@scholarly.local"
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/users" \
    -d "{
        \"email\": \"${NEW_EMAIL}\",
        \"display_name\": \"API Test User ${TS}\",
        \"password\": \"TestPassword12!\",
        \"roles\": [\"viewer\"]
    }")
expect_status "200" "$CODE" "create user"

NEW_ID=""
if have_jq; then
    NEW_ID=$(jq -r '.id // empty' "$BODY")
    CREATED_EMAIL=$(jq -r '.email // empty' "$BODY")
    if [ "$CREATED_EMAIL" = "$NEW_EMAIL" ]; then
        pass "created user email matches: ${CREATED_EMAIL}"
    else
        fail "created user email mismatch (expected ${NEW_EMAIL}, got ${CREATED_EMAIL})"
    fi
fi

# ── POST /users — duplicate email → 409 ──────────────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/users" \
    -d "{
        \"email\": \"${NEW_EMAIL}\",
        \"display_name\": \"Duplicate\",
        \"password\": \"TestPassword12!\"
    }")
expect_status "409" "$CODE" "duplicate email returns 409"

# ── POST /users — short password → 422 ───────────────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/users" \
    -d '{"email":"newuser2@test.local","display_name":"x","password":"short"}'
)
expect_status "422" "$CODE" "short password returns 422"

# ── PUT /users/<id> — update display name ────────────────────────────────────
if [ -n "$NEW_ID" ]; then
    CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}" \
        -X PUT "${BASE}/users/${NEW_ID}" \
        -d '{"display_name": "Updated Name"}')
    expect_status "200" "$CODE" "update user display name"

    if have_jq; then
        DNAME=$(jq -r '.display_name // empty' "$BODY")
        if [ "$DNAME" = "Updated Name" ]; then
            pass "display_name updated"
        else
            fail "display_name not updated (got: $DNAME)"
        fi
    fi

    # ── DELETE /users/<id> — deactivate ──────────────────────────────────────
    CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}" \
        -X DELETE "${BASE}/users/${NEW_ID}")
    expect_status "200" "$CODE" "deactivate user"

    if have_jq; then
        STATUS=$(jq -r '.status // empty' "$BODY")
        if [ "$STATUS" = "deactivated" ]; then
            pass "user status is deactivated"
        else
            fail "expected status=deactivated, got: ${STATUS}"
        fi
    fi
fi

# ── POST /users — unknown role name → 422 (hardening-pass validation) ────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/users" \
    -d "{\"email\":\"unknownrole_$(date +%s)@test.local\",\"display_name\":\"Bad Role\",\"password\":\"TestPassword12!\",\"roles\":[\"superuser\"]}")
expect_status "422" "$CODE" "unknown role name returns 422"

# ── Viewer cannot create users ────────────────────────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${VIEWER_TOKEN}" \
    -X POST "${BASE}/users" \
    -d '{"email":"x@x.local","display_name":"x","password":"TestPassword12!"}')
expect_status "403" "$CODE" "viewer cannot create users"

rm -f "$BODY"
echo "[user_crud] ALL PASS"
