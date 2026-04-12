#!/usr/bin/env bash
# After logout, the previously-valid token must yield 401 on /auth/me.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

# Use librarian so we don't interfere with the admin account.
TOKEN=$(login_as "librarian@scholarly.local")

# Sanity: token is currently valid.
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer ${TOKEN}" "${BASE}/auth/me")
expect_status "200" "$CODE" "pre-logout /auth/me"

# Logout.
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer ${TOKEN}" \
    -X POST "${BASE}/auth/logout")
expect_status "200" "$CODE" "logout"

# Token should now be rejected.
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Authorization: Bearer ${TOKEN}" "${BASE}/auth/me")
expect_status "401" "$CODE" "post-logout /auth/me"
