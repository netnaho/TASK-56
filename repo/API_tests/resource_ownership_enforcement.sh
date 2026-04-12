#!/usr/bin/env bash
# Verifies that create_draft_version enforces instructor ownership.
#
# Hardening-pass coverage: the ownership check added to
# resource_service::create_draft_version that returns 403 when an
# Instructor attempts to draft-version a resource they do not own.
#
# Test flow:
# 1. Librarian creates a resource (librarian is the owner).
# 2. Instructor (not the owner) tries to create a draft version → expect 403.
# 3. Librarian creates a draft version → expect 200.
# 4. Admin creates a draft version → expect 200 (admin is unrestricted).
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

echo "[resource_ownership_enforcement] testing instructor ownership gate"

BODY=$(mktemp)

ADMIN_TOKEN=$(login_as "admin@scholarly.local")
LIBRARIAN_TOKEN=$(login_as "librarian@scholarly.local")
INSTRUCTOR_TOKEN=$(login_as "instructor@scholarly.local")

# ── Librarian creates a resource ─────────────────────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${LIBRARIAN_TOKEN}" \
    -X POST "${BASE}/teaching-resources" \
    -d '{
        "title": "Ownership Test Resource",
        "resource_type": "document",
        "description": "Used by resource_ownership_enforcement.sh",
        "content_url": "https://example.edu/test.pdf",
        "mime_type": "application/pdf",
        "change_summary": "initial"
    }')
expect_status "200" "$CODE" "librarian creates resource"
RESOURCE_ID=$(json_field "$BODY" id)
pass "resource created (id=${RESOURCE_ID})"

# ── Instructor cannot create a draft for a resource they do not own ───────
# Route: PUT /teaching-resources/<id>  (creates a new draft version)
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${INSTRUCTOR_TOKEN}" \
    -X PUT "${BASE}/teaching-resources/${RESOURCE_ID}" \
    -d '{
        "description": "instructor tries to draft unowned resource",
        "change_summary": "unauthorized attempt"
    }')
expect_status "403" "$CODE" "instructor cannot draft-version unowned resource"

# ── Librarian (owner) can create a draft version ─────────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${LIBRARIAN_TOKEN}" \
    -X PUT "${BASE}/teaching-resources/${RESOURCE_ID}" \
    -d '{
        "description": "owner creates new draft",
        "change_summary": "librarian draft v2"
    }')
expect_status "200" "$CODE" "librarian (owner) can create draft version"

# ── Admin can create a draft version on any resource ─────────────────────
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${ADMIN_TOKEN}" \
    -X PUT "${BASE}/teaching-resources/${RESOURCE_ID}" \
    -d '{
        "description": "admin creates draft on unowned resource",
        "change_summary": "admin override"
    }')
expect_status "200" "$CODE" "admin can create draft version on any resource"

# ── Viewer cannot create a draft version (capability check) ───────────────
VIEWER_TOKEN=$(login_as "viewer@scholarly.local")
CODE=$(curl -s -o "$BODY" -w "%{http_code}" \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer ${VIEWER_TOKEN}" \
    -X PUT "${BASE}/teaching-resources/${RESOURCE_ID}" \
    -d '{
        "description": "viewer should be rejected",
        "change_summary": "unauthorized"
    }')
if [ "$CODE" = "403" ] || [ "$CODE" = "401" ]; then
    pass "viewer cannot create draft (${CODE})"
else
    fail "viewer expected 403/401, got ${CODE}"
fi

rm -f "$BODY"
echo "[resource_ownership_enforcement] ALL PASS"
