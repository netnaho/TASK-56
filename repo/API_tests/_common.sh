#!/usr/bin/env bash
# Shared helpers for API test scripts.
# Sourced by every *.sh script in this directory.

set -euo pipefail

BACKEND_URL="${BACKEND_URL:-http://localhost:8000}"
BASE="${BACKEND_URL}/api/v1"

# Default seed password — matches infrastructure::bootstrap::DEFAULT_SEED_PASSWORD.
DEFAULT_PASSWORD="ChangeMe!Scholarly2026"

# Pretty printing.
pass() { echo "  PASS: $1"; }
fail() { echo "  FAIL: $1"; exit 1; }

# Assert an HTTP status code. Arguments: expected_code actual_code description.
expect_status() {
    local expected="$1"
    local actual="$2"
    local desc="$3"
    if [ "$actual" = "$expected" ]; then
        pass "$desc (status $actual)"
    else
        fail "$desc (expected $expected, got $actual)"
    fi
}

# Require `jq` to be on PATH. If missing, fall back to grep-based parsing.
have_jq() { command -v jq >/dev/null 2>&1; }

# Extract a JSON field from a file. Prefers jq; falls back to a regex.
json_field() {
    local file="$1"
    local key="$2"
    if have_jq; then
        jq -r ".$key // empty" "$file"
    else
        # Very limited fallback — works for flat string fields only.
        grep -oE "\"${key}\"[[:space:]]*:[[:space:]]*\"[^\"]*\"" "$file" \
            | head -n1 \
            | sed -E 's/.*:[[:space:]]*"([^"]*)"/\1/'
    fi
}

# Login as the given email and print the token on stdout.
# Fails the test on any non-200.
login_as() {
    local email="$1"
    local pass="${2:-$DEFAULT_PASSWORD}"
    local body
    body=$(mktemp)
    local code
    code=$(curl -s -o "$body" -w "%{http_code}" \
        -H "Content-Type: application/json" \
        -X POST "${BASE}/auth/login" \
        -d "{\"email\":\"${email}\",\"password\":\"${pass}\"}")
    if [ "$code" != "200" ]; then
        cat "$body" >&2
        rm -f "$body"
        fail "login_as $email (status $code)"
    fi
    json_field "$body" token
    rm -f "$body"
}
