#!/usr/bin/env bash
# Strict-mode retention + backfill readiness — end-to-end shell tests.
#
# Tests:
#   1. Unauthenticated strict-mode retention execute → 401
#   2. Non-admin (viewer) strict-mode retention execute → 403
#   3. Compat mode (strict_mode=false, dry_run=true) always → 200
#   4. Strict-mode dry-run: either 200 (no legacy) or 409 strict_mode_blocked
#   5. 409 response includes machine-readable code "strict_mode_blocked"
#   6. Backfill dry-run includes strict_retention_ready field
#   7. Backfill dry-run includes actionable_legacy_count_after_run field
#   8. Live backfill → 200 with strict_retention_ready boolean
#   9. After successful backfill: strict-mode execute → 200 (if ready)
#  10. Compat mode is always backward-compatible regardless of legacy state
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

echo "[artifact_backfill_strict] testing strict-mode retention + backfill readiness"

BODY=$(mktemp)
BODY2=$(mktemp)

ADMIN_TOKEN=$(login_as "admin@scholarly.local")
VIEWER_TOKEN=$(login_as "viewer@scholarly.local")

# ── 1. Unauthenticated strict-mode → 401 ──────────────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -X POST "${BASE}/admin/retention/execute" \
    -H "Content-Type: application/json" \
    -d '{"dry_run":true,"strict_mode":true}')
expect_status "401" "$CODE" "unauthenticated strict-mode retention → 401"

# ── 2. Viewer strict-mode → 403 ───────────────────────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -X POST "${BASE}/admin/retention/execute" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${VIEWER_TOKEN}" \
    -d '{"dry_run":true,"strict_mode":true}')
expect_status "403" "$CODE" "viewer strict-mode retention → 403"

# ── 3. Compat mode (strict_mode=false) dry-run always → 200 ───────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -X POST "${BASE}/admin/retention/execute" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -d '{"dry_run":true,"strict_mode":false}')
expect_status "200" "$CODE" "compat mode (strict_mode=false) dry-run → 200"

if have_jq; then
    # Response must include the new strict-mode summary fields.
    STRICT_READY=$(jq -r '.strict_retention_ready' "$BODY")
    STRICT_MODE_VAL=$(jq -r '.strict_mode' "$BODY")
    if [ "$STRICT_MODE_VAL" = "false" ]; then
        pass "compat mode: strict_mode field is false in summary"
    else
        fail "compat mode: strict_mode field should be false, got: ${STRICT_MODE_VAL}"
    fi
fi

# ── 4. Strict-mode dry-run: 200 (no legacy) or 409 (legacy exists) ────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -X POST "${BASE}/admin/retention/execute" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -d '{"dry_run":true,"strict_mode":true}')

if [ "$CODE" = "200" ]; then
    pass "strict-mode dry-run → 200 (no unresolved legacy artifacts)"
    LEGACY_UNRESOLVED=0
elif [ "$CODE" = "409" ]; then
    pass "strict-mode dry-run → 409 (unresolved legacy artifacts exist — backfill needed)"
    LEGACY_UNRESOLVED=1

    # ── 5. 409 body must include machine-readable error code ───────────────────
    if have_jq; then
        ERROR_CODE=$(jq -r '.error.code' "$BODY")
        if [ "$ERROR_CODE" = "strict_mode_blocked" ]; then
            pass "409 error code is strict_mode_blocked"
        else
            fail "409 error code must be strict_mode_blocked, got: ${ERROR_CODE}"
        fi

        ERROR_MSG=$(jq -r '.error.message' "$BODY")
        if echo "$ERROR_MSG" | grep -qi "backfill"; then
            pass "409 message includes remediation hint mentioning backfill"
        else
            fail "409 message should include remediation hint; got: ${ERROR_MSG}"
        fi
    fi
else
    fail "strict-mode dry-run must return 200 or 409, got: ${CODE}"
    LEGACY_UNRESOLVED=0
fi

# ── 6. Backfill dry-run includes strict_retention_ready ───────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -X POST "${BASE}/admin/artifact-backfill" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -d '{"dry_run":true}')
expect_status "200" "$CODE" "backfill dry-run → 200"

if have_jq; then
    if jq -e '.strict_retention_ready' "$BODY" > /dev/null 2>&1; then
        READY_FIELD=$(jq -r '.strict_retention_ready' "$BODY")
        pass "backfill dry-run includes strict_retention_ready=${READY_FIELD}"
    else
        fail "backfill dry-run response must include strict_retention_ready field"
    fi
fi

# ── 7. Backfill dry-run includes actionable_legacy_count_after_run ────────────
if have_jq; then
    if jq -e '.actionable_legacy_count_after_run' "$BODY" > /dev/null 2>&1; then
        ACTIONABLE=$(jq -r '.actionable_legacy_count_after_run' "$BODY")
        pass "backfill dry-run includes actionable_legacy_count_after_run=${ACTIONABLE}"
    else
        fail "backfill dry-run response must include actionable_legacy_count_after_run"
    fi
    if jq -e '.unresolved_run_ids_count' "$BODY" > /dev/null 2>&1; then
        UNRESOLVED=$(jq -r '.unresolved_run_ids_count' "$BODY")
        pass "backfill dry-run includes unresolved_run_ids_count=${UNRESOLVED}"
    else
        fail "backfill dry-run response must include unresolved_run_ids_count"
    fi
fi

# ── 8. Live backfill → 200 with strict_retention_ready boolean ────────────────
CODE=$(curl -s -o "$BODY2" -w "%{http_code}" \
    -X POST "${BASE}/admin/artifact-backfill" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -d '{"dry_run":false,"batch_size":100}')
expect_status "200" "$CODE" "live backfill → 200"

if have_jq; then
    READY_AFTER=$(jq -r '.strict_retention_ready' "$BODY2")
    REMAINING=$(jq -r '.actionable_legacy_count_after_run' "$BODY2")
    FAILED=$(jq -r '.encrypt_failed_count' "$BODY2")
    ENC=$(jq -r '.encrypted_count' "$BODY2")
    pass "live backfill: encrypted=${ENC} remaining=${REMAINING} \
failed=${FAILED} strict_retention_ready=${READY_AFTER}"

    if [ "$READY_AFTER" = "true" ]; then
        pass "live backfill: strict_retention_ready=true (all legacy resolved)"
    elif [ "$FAILED" != "0" ]; then
        pass "live backfill: strict_retention_ready=false due to ${FAILED} \
encrypt_failed rows (retryable — acceptable)"
    else
        # If no failures and not ready, something is wrong.
        fail "live backfill: strict_retention_ready=false with no failures — unexpected"
    fi
fi

# ── 9. Post-backfill: strict-mode execute → 200 if ready ──────────────────────
if have_jq; then
    READY_AFTER=$(jq -r '.strict_retention_ready' "$BODY2")
    if [ "$READY_AFTER" = "true" ]; then
        CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
            -X POST "${BASE}/admin/retention/execute" \
            -H "Content-Type: application/json" \
            -H "Authorization: Bearer ${ADMIN_TOKEN}" \
            -d '{"dry_run":true,"strict_mode":true}')
        expect_status "200" "$CODE" \
            "post-backfill strict-mode execute → 200 (strict_retention_ready=true)"

        SUMMARY_READY=$(jq -r '.strict_retention_ready' "$BODY")
        if [ "$SUMMARY_READY" = "true" ]; then
            pass "execute summary confirms strict_retention_ready=true"
        else
            fail "execute summary strict_retention_ready should be true; \
got ${SUMMARY_READY}"
        fi
    else
        pass "post-backfill strict-mode check skipped (encrypt_failed rows remain)"
    fi
fi

# ── 10. Backward compat: strict_mode absent from body → treated as false → 200 ─
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -X POST "${BASE}/admin/retention/execute" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -d '{"dry_run":true}')
expect_status "200" "$CODE" "no strict_mode field → defaults to false → 200"

echo "[artifact_backfill_strict] all checks passed"
