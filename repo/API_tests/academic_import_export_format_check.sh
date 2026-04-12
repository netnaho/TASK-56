#!/usr/bin/env bash
# Phase 4 — template endpoints: /template.csv must return "code,title,..."
# text, and /template.xlsx must return a real xlsx starting with "PK".
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/_common.sh"

ADMIN_TOKEN=$(login_as "admin@scholarly.local")

BODY=$(mktemp)
HEADERS=$(mktemp)
trap 'rm -f "$BODY" "$HEADERS"' EXIT

check_csv_template() {
    local path="$1"
    local expected_prefix="$2"
    echo "[academic_import_export_format_check] GET ${path}"
    local code
    code=$(curl -s -o "$BODY" -D "$HEADERS" -w "%{http_code}" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}" \
        "${BASE}${path}")
    expect_status "200" "$code" "${path}"
    if ! grep -iE '^Content-Type:.*text/csv' "$HEADERS" >/dev/null; then
        grep -i '^content-type:' "$HEADERS" || true
        fail "${path} Content-Type missing text/csv"
    fi
    local first_line
    first_line=$(head -n1 "$BODY" | tr -d '\r')
    case "$first_line" in
        "${expected_prefix}"*)
            pass "${path} body starts with '${expected_prefix}'"
            ;;
        *)
            fail "${path} body does not start with '${expected_prefix}': '${first_line}'"
            ;;
    esac
}

check_xlsx_template() {
    local path="$1"
    echo "[academic_import_export_format_check] GET ${path}"
    local code
    code=$(curl -s -o "$BODY" -D "$HEADERS" -w "%{http_code}" \
        -H "Authorization: Bearer ${ADMIN_TOKEN}" \
        "${BASE}${path}")
    expect_status "200" "$code" "${path}"
    if ! grep -iE '^Content-Type:.*spreadsheet' "$HEADERS" >/dev/null; then
        grep -i '^content-type:' "$HEADERS" || true
        fail "${path} Content-Type missing 'spreadsheet'"
    fi
    # First two bytes must be the ZIP magic "PK".
    local magic
    magic=$(head -c 2 "$BODY")
    if [ "$magic" != "PK" ]; then
        fail "${path} first 2 bytes are not 'PK' (got '$magic')"
    fi
    pass "${path} starts with ZIP magic PK"
}

check_csv_template "/courses/template.csv" "code,title,department_code"
check_xlsx_template "/courses/template.xlsx"

check_csv_template "/sections/template.csv" "course_code,section_code,term"
check_xlsx_template "/sections/template.xlsx"
