#!/usr/bin/env bash
# Scholarly — full test suite runner
#
# Usage:
#   ./run_tests.sh                          # run all suites; DB integration tests skipped with notice
#   BACKEND_URL=http://localhost:8000 ./run_tests.sh
#   ./run_tests.sh --api-only               # only run API_tests/ scripts (requires live backend)
#   ./run_tests.sh --unit-only              # only run cargo unit/integration tests
#
# DB integration tests (backend/tests/api_routes_test.rs):
#   SCHOLARLY_TEST_DB_URL=mysql://scholarly_app:scholarly_app_pass@localhost:3307/scholarly \
#     ./run_tests.sh
#       — Runs the 5 DB-backed authz/scope tests against a live MySQL instance.
#         The compose stack must be up. Tests run with --test-threads=1.
#
#   SCHOLARLY_TEST_DB_URL=mysql://... ./run_tests.sh --strict-integration
#       — Same as above but FAILS the run if SCHOLARLY_TEST_DB_URL is absent.
#         Use this in CI to prevent silent skips of critical integration tests.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND_URL="${BACKEND_URL:-http://localhost:8000}"
API_ONLY=0
UNIT_ONLY=0
# When set, fails the run if SCHOLARLY_TEST_DB_URL is absent so that DB
# integration tests cannot be silently skipped.  Intended for CI pipelines.
# Local dev: omit the flag (or leave SCHOLARLY_TEST_DB_URL unset) to skip
# the DB integration tests gracefully.
STRICT_INTEGRATION=0
for arg in "$@"; do
  case "$arg" in
    --api-only)           API_ONLY=1 ;;
    --unit-only)          UNIT_ONLY=1 ;;
    --strict-integration) STRICT_INTEGRATION=1 ;;
  esac
done

PASS=0
FAIL=0
SKIP=0

pass()  { echo "  [PASS] $*"; PASS=$((PASS + 1)); }
fail()  { echo "  [FAIL] $*"; FAIL=$((FAIL + 1)); }
skip()  { echo "  [SKIP] $*"; SKIP=$((SKIP + 1)); }
banner(){ echo ""; echo "━━━  $*  ━━━"; }

have_cargo() { command -v cargo >/dev/null 2>&1; }

# ─── 1. Backend unit tests (cargo test --lib) ─────────────────────────────────
if [ $API_ONLY -eq 0 ]; then
  banner "Backend unit tests  (cargo test --lib)"
  if ! have_cargo; then
    skip "Backend unit tests (cargo not in PATH — install Rust or run inside the Docker container)"
  else
    cd "$SCRIPT_DIR/backend"
    if cargo test --lib --quiet 2>&1; then
      pass "Backend unit tests"
    else
      fail "Backend unit tests"
    fi
  fi
fi

# ─── 2. Backend integration tests (cargo test --test '*') ─────────────────────
if [ $API_ONLY -eq 0 ]; then
  banner "Backend integration tests  (cargo test --test '*')"
  if ! have_cargo; then
    skip "Backend integration tests (cargo not in PATH — install Rust or run inside the Docker container)"
  else
    cd "$SCRIPT_DIR/backend"
    if cargo test --test '*' --quiet 2>&1; then
      pass "Backend integration tests"
    else
      fail "Backend integration tests"
    fi
  fi

  # ── DB-backed authz integration tests (opt-in, require live MySQL) ────────
  # tests/api_routes_test.rs contains 5 tests that cover critical authz/scope
  # paths.  When SCHOLARLY_TEST_DB_URL is absent they silently skip inside
  # `cargo test` (reported as "passed" with zero assertions), which is correct
  # for local dev but creates a false-green risk in CI.
  #
  # Three modes:
  #   default              — skip with a visible notice (local dev workflow)
  #   SCHOLARLY_TEST_DB_URL set  — tests run for real against the pointed DB
  #   --strict-integration — fail the run if SCHOLARLY_TEST_DB_URL is absent
  # ─────────────────────────────────────────────────────────────────────────
  echo ""
  if [ -n "${SCHOLARLY_TEST_DB_URL:-}" ]; then
    # Re-run only the DB integration test binary explicitly so its result is
    # surfaced as a distinct line in this script's output even though it was
    # already included in `cargo test --test '*'` above.
    banner "DB authz integration tests  (SCHOLARLY_TEST_DB_URL is set)"
    if SCHOLARLY_TEST_DB_URL="${SCHOLARLY_TEST_DB_URL}" \
        cargo test --test api_routes_test -- --test-threads=1 --quiet 2>&1; then
      pass "DB authz integration tests (5/5 ran against live DB)"
    else
      fail "DB authz integration tests"
    fi
  elif [ "$STRICT_INTEGRATION" -eq 1 ]; then
    banner "DB authz integration tests  (strict-integration mode)"
    fail "DB authz integration tests — SCHOLARLY_TEST_DB_URL is not set"
    echo ""
    echo "  strict-integration mode requires a live MySQL connection."
    echo "  Set SCHOLARLY_TEST_DB_URL and retry, for example:"
    echo ""
    echo "    SCHOLARLY_TEST_DB_URL=mysql://scholarly_app:scholarly_app_pass@localhost:3307/scholarly \\"
    echo "      ./run_tests.sh --strict-integration"
    echo ""
  else
    # Default local-dev path: warn visibly but do not fail.
    skip "DB authz integration tests (SCHOLARLY_TEST_DB_URL not set)"
    echo ""
    echo "  ┌──────────────────────────────────────────────────────────────────┐"
    echo "  │  NOTICE  5 DB-backed authz/scope integration tests were SKIPPED  │"
    echo "  │  (backend/tests/api_routes_test.rs).                             │"
    echo "  │                                                                  │"
    echo "  │  These cover critical paths that unit tests cannot exercise:     │"
    echo "  │    1. Unauthenticated request → 401                              │"
    echo "  │    2. Non-admin on AdminOnly endpoint → 403                      │"
    echo "  │    3. Report schedule scope isolation → 403                      │"
    echo "  │    4. Audit export capability gate (viewer/librarian) → 403      │"
    echo "  │    5. Audit export CSV happy-path (admin) → 200 + text/csv       │"
    echo "  │                                                                  │"
    echo "  │  To run them locally (requires docker compose up first):         │"
    echo "  │    SCHOLARLY_TEST_DB_URL=mysql://scholarly_app:scholarly_app_pass│"
    echo "  │      @localhost:3307/scholarly ./run_tests.sh                    │"
    echo "  │                                                                  │"
    echo "  │  To enforce in CI (fails if env var is absent):                  │"
    echo "  │    SCHOLARLY_TEST_DB_URL=mysql://... ./run_tests.sh \\           │"
    echo "  │      --strict-integration                                        │"
    echo "  └──────────────────────────────────────────────────────────────────┘"
    echo ""
  fi
fi

# ─── 3. Frontend tests (cargo test) ───────────────────────────────────────────
if [ $API_ONLY -eq 0 ]; then
  banner "Frontend tests  (cargo test)"
  if ! have_cargo; then
    skip "Frontend tests (cargo not in PATH — install Rust or run inside the Docker container)"
  else
    cd "$SCRIPT_DIR/frontend"
    if cargo test --quiet 2>&1; then
      pass "Frontend tests"
    else
      fail "Frontend tests"
    fi
  fi
fi

# ─── 4. API tests (shell scripts against live backend) ────────────────────────
if [ $UNIT_ONLY -eq 0 ]; then
  banner "API tests  (shell scripts vs ${BACKEND_URL})"

  health_response=$(curl -s -o /dev/null -w "%{http_code}" "${BACKEND_URL}/api/v1/health" 2>/dev/null || echo "000")
  if [ "$health_response" = "000" ] || [ "$health_response" = "404" ]; then
    skip "Backend not reachable at ${BACKEND_URL}."
    echo "       Start the stack first:  docker compose up"
    echo "       Then:                   ./run_tests.sh"
  else
    total_api=0
    failed_api=0

    for test_file in "$SCRIPT_DIR/API_tests"/*.sh; do
      base=$(basename "$test_file")
      case "$base" in
        _*) continue ;;   # skip helper libraries (_common.sh etc.)
      esac
      total_api=$((total_api + 1))
      echo ""
      echo "  >>> ${base}"
      if BACKEND_URL="${BACKEND_URL}" bash "$test_file" 2>&1; then
        pass "$base"
      else
        fail "$base"
        failed_api=$((failed_api + 1))
      fi
    done

    echo ""
    echo "  API tests: $((total_api - failed_api))/${total_api} passed"
  fi
fi

# ─── Summary ──────────────────────────────────────────────────────────────────
echo ""
echo "═══════════════════════════════════════════"
echo "  Results: ${PASS} passed  ${FAIL} failed  ${SKIP} skipped"
if [ $FAIL -eq 0 ]; then
  echo "  ✓ ALL TESTS PASSED"
  echo "═══════════════════════════════════════════"
  exit 0
else
  echo "  ✗ SOME TESTS FAILED"
  echo "═══════════════════════════════════════════"
  exit 1
fi
