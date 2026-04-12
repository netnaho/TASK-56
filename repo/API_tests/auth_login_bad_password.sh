#!/usr/bin/env bash
# Wrong password must yield 401, not 200 and not 500.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

# Unique email to avoid colliding with the lockout tests which reuse the
# admin account.
EMAIL="librarian@scholarly.local"

echo "[auth_login_bad_password] POST ${BASE}/auth/login (wrong password)"
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -X POST "${BASE}/auth/login" \
    -d "{\"email\":\"${EMAIL}\",\"password\":\"DefinitelyWrong123456\"}")
expect_status "401" "$CODE" "wrong password -> 401"
