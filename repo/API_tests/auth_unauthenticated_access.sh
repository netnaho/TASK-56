#!/usr/bin/env bash
# Requests without a bearer token must be rejected with 401.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

for path in /auth/me /users/me /admin/config /audit-logs; do
    CODE=$(curl -s -o /dev/null -w "%{http_code}" "${BASE}${path}")
    expect_status "401" "$CODE" "unauthenticated GET ${path}"
done
