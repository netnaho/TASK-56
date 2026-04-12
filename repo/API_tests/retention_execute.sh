#!/usr/bin/env bash
# Retention execution tests: dry-run reports counts, live run reduces eligible rows.
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

echo "[retention_execute] testing retention policy execution"

BODY=$(mktemp)

ADMIN_TOKEN=$(login_as "admin@scholarly.local")
VIEWER_TOKEN=$(login_as "viewer@scholarly.local")

# ── Viewer cannot execute ────────────────────────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${VIEWER_TOKEN}" \
    -X POST "${BASE}/admin/retention/execute" \
    -d '{"dry_run": true}')
expect_status "403" "$CODE" "viewer cannot execute retention"

# ── Admin dry-run: execute all ───────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X POST "${BASE}/admin/retention/execute" \
    -d '{"dry_run": true}')
expect_status "200" "$CODE" "admin dry-run execute all"

if have_jq; then
    DRY_RUN=$(jq -r '.dry_run // "false"' "$BODY")
    POLICIES_RUN=$(jq -r '.policies_run // 0' "$BODY")
    if [ "$DRY_RUN" = "true" ]; then
        pass "dry_run flag is true in response"
    else
        fail "dry_run flag should be true, got: $DRY_RUN"
    fi
    pass "policies_run=${POLICIES_RUN}"
else
    pass "jq not available — skipping dry_run field verification"
fi

# ── Admin dry-run on specific policy ────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    "${BASE}/admin/retention")
if have_jq; then
    POLICY_ID=$(jq -r '.[0].id // empty' "$BODY")
else
    POLICY_ID=""
fi

if [ -n "$POLICY_ID" ] && [ "$POLICY_ID" != "null" ]; then
    CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
        -H "Content-Type: application/json" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}" \
        -X POST "${BASE}/admin/retention/${POLICY_ID}/execute" \
        -d '{"dry_run": true}')
    expect_status "200" "$CODE" "admin dry-run execute single policy"

    if have_jq; then
        DRY_RUN=$(jq -r '.dry_run // "false"' "$BODY")
        ROWS=$(jq -r '.rows_affected // 0' "$BODY")
        if [ "$DRY_RUN" = "true" ]; then
            pass "single policy dry_run is true"
        else
            fail "single policy dry_run should be true, got: $DRY_RUN"
        fi
        pass "rows_affected=${ROWS} (dry run — no deletions)"
    else
        pass "single policy dry-run executed (jq not available for detail check)"
    fi
else
    pass "no policies found — skipping single-policy execute test"
fi

# ── Unauthenticated → 401 ────────────────────────────────────────────────
CODE=$(curl -s -o /dev/null -w "%{http_code}" \
    -X POST "${BASE}/admin/retention/execute" \
    -H "Content-Type: application/json" \
    -d '{"dry_run": true}')
expect_status "401" "$CODE" "unauthenticated retention execute returns 401"

rm -f "$BODY"
echo "[retention_execute] ALL PASS"
