#!/usr/bin/env bash
# Scholarly — full test suite runner
#
# The script starts the Docker Compose stack automatically, waits for the
# backend to be healthy, then runs all test suites in order:
#
#   1. Backend unit tests       (cargo test --lib)
#   2. Backend integration tests (cargo test --test '*')
#   3. DB authz integration tests (api_routes_test.rs — requires live DB)
#   4. Frontend unit tests      (cargo test)
#   5. API shell tests          (API_tests/*.sh against live backend)
#
# Usage:
#   ./run_tests.sh                          # start stack + run all suites
#   ./run_tests.sh --api-only               # start stack + run API shell tests only
#   ./run_tests.sh --unit-only              # run cargo unit tests only (no stack needed)
#   ./run_tests.sh --no-docker-up           # skip 'docker compose up' (stack already running)
#   ./run_tests.sh --no-docker-down         # leave stack running after tests finish
#   ./run_tests.sh --strict-integration     # fail if SCHOLARLY_TEST_DB_URL is absent
#
# DB integration tests are automatically enabled when the compose stack is up
# (SCHOLARLY_TEST_DB_URL is set automatically to the compose MySQL).
# Override with: SCHOLARLY_TEST_DB_URL=mysql://... ./run_tests.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BACKEND_URL="${BACKEND_URL:-http://localhost:8000}"

API_ONLY=0
UNIT_ONLY=0
STRICT_INTEGRATION=0
NO_DOCKER_UP=0
NO_DOCKER_DOWN=0

for arg in "$@"; do
  case "$arg" in
    --api-only)           API_ONLY=1 ;;
    --unit-only)          UNIT_ONLY=1 ;;
    --strict-integration) STRICT_INTEGRATION=1 ;;
    --no-docker-up)       NO_DOCKER_UP=1 ;;
    --no-docker-down)     NO_DOCKER_DOWN=1 ;;
  esac
done

PASS=0
FAIL=0
SKIP=0

pass()  { echo "  [PASS] $*"; PASS=$((PASS + 1)); }
fail()  { echo "  [FAIL] $*"; FAIL=$((FAIL + 1)); }
skip()  { echo "  [SKIP] $*"; SKIP=$((SKIP + 1)); }
banner(){ echo ""; echo "━━━  $*  ━━━"; }

have_cargo()  { command -v cargo  >/dev/null 2>&1; }
have_docker() { command -v docker >/dev/null 2>&1; }

# ─── Compose stack management ─────────────────────────────────────────────────

COMPOSE_CMD=""
if have_docker; then
  if docker compose version >/dev/null 2>&1; then
    COMPOSE_CMD="docker compose"
  elif command -v docker-compose >/dev/null 2>&1; then
    COMPOSE_CMD="docker-compose"
  fi
fi

# Bring the compose stack up in detached mode, then wait for the backend
# health endpoint to return 200.
#
# Why the long timeout: on first run the backend container must compile
# migrations, run all SQL files, and Argon2id-hash every seed user password
# before Rocket starts accepting connections.  On a mid-range laptop this
# takes 3–5 minutes after the MySQL service becomes healthy.
bring_stack_up() {
  if [ -z "$COMPOSE_CMD" ]; then
    echo "  [WARN] docker / docker compose not found — skipping stack startup."
    echo "         Install Docker or start the stack manually before running tests."
    return 1
  fi

  banner "Starting Docker Compose stack"
  echo "  Running: $COMPOSE_CMD up -d"
  echo "  (Building images on first run takes several minutes — please wait)"
  echo ""

  cd "$SCRIPT_DIR"

  # Always build images from the current Dockerfiles before starting.
  # Docker layer caching makes this fast after the first build.
  # This prevents stale cached images (from other projects or old builds)
  # from being used in place of the correct backend or frontend image.
  echo "  Building images from current Dockerfiles..."
  $COMPOSE_CMD build
  echo ""

  $COMPOSE_CMD up -d

  echo ""
  echo "  Waiting for backend to finish migrations and start serving..."
  echo "  Health endpoint: ${BACKEND_URL}/api/v1/health"
  echo "  (First-run migrations + Argon2id seeding can take 3–5 minutes)"
  echo ""

  # Give the entrypoint a head-start before we start hammering the health endpoint.
  sleep 10

  local waited=10
  local max_wait=360   # 6 minutes — covers slow first-run migration + seeding
  local poll=5         # poll every 5 seconds

  while [ "$waited" -lt "$max_wait" ]; do
    local code
    code=$(curl -s -o /dev/null -w "%{http_code}" \
           --max-time 4 "${BACKEND_URL}/api/v1/health" 2>/dev/null || echo "000")
    if [ "$code" = "200" ]; then
      echo "  Backend is healthy (HTTP 200) — took ${waited}s total."
      return 0
    fi
    # Show a live progress line so the user sees the script is still running.
    printf "  [%3ds / %ds] HTTP %s — still waiting for backend...\r" \
           "$waited" "$max_wait" "$code"
    sleep "$poll"
    waited=$((waited + poll))
  done

  echo ""
  echo "  [FAIL] Backend did not become healthy within ${max_wait}s."
  echo ""
  echo "  Last few lines from the backend container:"
  $COMPOSE_CMD logs --tail=20 backend 2>/dev/null || true
  echo ""
  echo "  Tip: run '$COMPOSE_CMD logs -f backend' to watch the startup in real time."
  return 1
}

bring_stack_down() {
  if [ -z "$COMPOSE_CMD" ]; then return; fi
  banner "Stopping Docker Compose stack"
  cd "$SCRIPT_DIR"
  $COMPOSE_CMD down
}

# ─── Stack startup ────────────────────────────────────────────────────────────
# --unit-only skips the stack entirely (no API or DB tests needed).
# --no-docker-up skips bring-up (caller guarantees stack is already running).

STACK_STARTED=0
if [ "$UNIT_ONLY" -eq 0 ] && [ "$NO_DOCKER_UP" -eq 0 ]; then
  if bring_stack_up; then
    STACK_STARTED=1
  else
    echo ""
    echo "  Stack startup failed. Unit tests will still run; API and DB tests will be skipped."
    echo ""
  fi
fi

# When the compose stack is up, automatically enable DB integration tests
# using the containerized MySQL (port 3307 on the host, as mapped in compose).
if [ "$STACK_STARTED" -eq 1 ] && [ -z "${SCHOLARLY_TEST_DB_URL:-}" ]; then
  export SCHOLARLY_TEST_DB_URL="mysql://scholarly_app:scholarly_app_pass@localhost:3307/scholarly"
  echo "  Auto-set SCHOLARLY_TEST_DB_URL to compose MySQL (port 3307)."
  echo ""
fi

# ─── 1. Backend unit tests (cargo test --lib) ─────────────────────────────────
if [ "$API_ONLY" -eq 0 ]; then
  banner "Backend unit tests  (cargo test --lib)"
  if ! have_cargo; then
    skip "Backend unit tests (cargo not in PATH — install Rust to run locally)"
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
if [ "$API_ONLY" -eq 0 ]; then
  banner "Backend integration tests  (cargo test --test '*')"
  if ! have_cargo; then
    skip "Backend integration tests (cargo not in PATH)"
  else
    cd "$SCRIPT_DIR/backend"
    if cargo test --test '*' --quiet 2>&1; then
      pass "Backend integration tests"
    else
      fail "Backend integration tests"
    fi
  fi

  # ── DB-backed authz integration tests ────────────────────────────────────
  echo ""
  if [ -n "${SCHOLARLY_TEST_DB_URL:-}" ]; then
    banner "DB authz integration tests  (SCHOLARLY_TEST_DB_URL is set)"
    cd "$SCRIPT_DIR/backend"
    if SCHOLARLY_TEST_DB_URL="${SCHOLARLY_TEST_DB_URL}" \
        cargo test --test api_routes_test -- --test-threads=1 --quiet 2>&1; then
      pass "DB authz integration tests (ran against live DB)"
    else
      fail "DB authz integration tests"
    fi
  elif [ "$STRICT_INTEGRATION" -eq 1 ]; then
    banner "DB authz integration tests  (strict-integration mode)"
    fail "DB authz integration tests — SCHOLARLY_TEST_DB_URL is not set"
    echo ""
    echo "  strict-integration mode requires a live MySQL connection."
    echo "  Start the stack and retry:"
    echo ""
    echo "    ./run_tests.sh --strict-integration"
    echo ""
  else
    skip "DB authz integration tests (SCHOLARLY_TEST_DB_URL not set; stack not started)"
    echo ""
    echo "  ┌──────────────────────────────────────────────────────────────────┐"
    echo "  │  NOTICE  DB-backed authz/scope integration tests were SKIPPED    │"
    echo "  │  (backend/tests/api_routes_test.rs).                             │"
    echo "  │                                                                  │"
    echo "  │  These run automatically when the compose stack is started by    │"
    echo "  │  this script. To run them without the full suite:                │"
    echo "  │    SCHOLARLY_TEST_DB_URL=mysql://scholarly_app:scholarly_app_pass│"
    echo "  │      @localhost:3307/scholarly ./run_tests.sh --no-docker-up     │"
    echo "  └──────────────────────────────────────────────────────────────────┘"
    echo ""
  fi
fi

# ─── 3. Frontend tests (cargo test) ───────────────────────────────────────────
if [ "$API_ONLY" -eq 0 ]; then
  banner "Frontend tests  (cargo test)"
  if ! have_cargo; then
    skip "Frontend tests (cargo not in PATH)"
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
if [ "$UNIT_ONLY" -eq 0 ]; then
  banner "API tests  (shell scripts vs ${BACKEND_URL})"

  health_response=$(curl -s -o /dev/null -w "%{http_code}" "${BACKEND_URL}/api/v1/health" 2>/dev/null || echo "000")
  if [ "$health_response" != "200" ]; then
    skip "Backend not reachable at ${BACKEND_URL} (HTTP ${health_response}) — API tests skipped."
    if [ "$health_response" = "502" ] || [ "$health_response" = "503" ]; then
      echo "  HTTP ${health_response}: nginx is up but backend may still be starting."
      echo "  Check logs: $COMPOSE_CMD logs backend"
    fi
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

# ─── Stack teardown ───────────────────────────────────────────────────────────
if [ "$STACK_STARTED" -eq 1 ] && [ "$NO_DOCKER_DOWN" -eq 0 ]; then
  bring_stack_down
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
