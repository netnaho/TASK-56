# API Tests

Shell-based end-to-end tests that exercise the live HTTP surface of the
Scholarly backend. Every script sources `_common.sh` for shared helpers
(`login_as`, `expect_status`, `json_field`, `pass`, `fail`) and uses the
seed users that are bootstrapped on first `docker compose up`.

## Prerequisites

The compose stack must be running before any test script is executed:

```bash
docker compose up -d
```

Wait for the backend health check to return `{"status":"ok"}` before proceeding:

```bash
curl -s http://localhost:8000/api/v1/health
```

## Running the full API suite

The recommended entrypoint is `run_tests.sh` at the repository root. It
runs unit tests, cargo integration tests, frontend tests, **and** this
API suite in one command, with pass/fail counters and automatic skip if
the backend is unreachable:

```bash
# From the repo root
./run_tests.sh
```

To run only the API test scripts (skip all cargo tests):

```bash
./run_tests.sh --api-only
```

`run_tests.sh` discovers every non-helper script in this directory
(anything not prefixed with `_`) and runs them in alphabetical order.

## Running a single script

Execute any script directly from the repo root. The `BACKEND_URL`
environment variable controls which server is targeted (default:
`http://localhost:8000`):

```bash
# Default target (localhost:8000)
bash API_tests/health_check.sh

# Custom target
BACKEND_URL=http://staging.internal:8000 bash API_tests/health_check.sh
```

Scripts are self-contained: each one logs in, runs its assertions, and
prints `ALL PASS` on success or exits non-zero on the first failure.

## BACKEND_URL

All scripts inherit `BACKEND_URL` from the environment. `_common.sh`
derives `BASE="${BACKEND_URL}/api/v1"` from it, so every `curl` call
in every script automatically targets the right host.

| Scenario | Command prefix |
|---|---|
| Local compose stack (default) | *(no prefix needed)* |
| Custom port | `BACKEND_URL=http://localhost:9000` |
| Remote/staging host | `BACKEND_URL=http://host:8000` |

## Test scripts

| Script | Area covered |
|---|---|
| `health_check.sh` | Liveness probe (`GET /health`) |
| `auth_login_success.sh` | Successful login, token issuance |
| `auth_login_bad_password.sh` | Rejected credentials → 401 |
| `auth_lockout.sh` | Account lockout after repeated failures |
| `auth_unauthenticated_access.sh` | Protected routes reject missing token → 401 |
| `logout_revokes_session.sh` | Token unusable after logout |
| `admin_endpoint_protection.sh` | Non-admin blocked on admin-only routes → 403 |
| `admin_config.sh` | Admin config read/write |
| `admin_config_write.sh` | Config value mutation and audit trail |
| `user_crud.sh` | User create / update / deactivate (admin-only) |
| `audit_log_search.sh` | Audit log search filters and pagination |
| `audit_log_search_and_chain.sh` | Chain verification endpoint |
| `audit_log_export.sh` | CSV export: headers, content, capability gate |
| `library_journal_happy_path.sh` | Journal draft → approve → publish |
| `library_journal_validation.sh` | Invalid journal input → 422 |
| `library_journal_unauthorized.sh` | Journal writes rejected for non-librarian |
| `library_journal_not_found.sh` | Unknown UUID → 404 |
| `library_publish_baseline_invariant.sh` | Two-pointer invariant on publish |
| `library_resource_happy_path.sh` | Teaching resource lifecycle |
| `library_attachment_upload_and_preview.sh` | Attachment upload and preview |
| `library_attachment_validation.sh` | MIME whitelist enforcement |
| `library_preview_unsupported_type.sh` | Unsupported preview MIME → 415 |
| `resource_ownership_enforcement.sh` | Instructor resource ownership scope |
| `academic_course_happy_path.sh` | Course CRUD and versioning |
| `academic_course_validation.sh` | Course code / credit bounds validation |
| `academic_course_unauthorized.sh` | Course writes rejected without capability |
| `academic_prerequisites.sh` | Prerequisite graph, cycle detection |
| `academic_section_happy_path.sh` | Section lifecycle |
| `academic_section_validation.sh` | Section field validation |
| `academic_import_dry_run.sh` | Bulk import dry-run (no mutation) |
| `academic_import_commit.sh` | Bulk import commit (all-or-nothing) |
| `academic_import_unauthorized.sh` | Import blocked without `ImportExport` cap |
| `academic_import_export_format_check.sh` | CSV and XLSX format round-trips |
| `academic_export_scope.sh` | Department-scoped export enforcement |
| `checkin_happy_path.sh` | Check-in creation and retrieval |
| `checkin_duplicate_blocked.sh` | Duplicate within window → 409 |
| `checkin_retry_happy_path.sh` | Retry with valid reason code |
| `checkin_retry_invalid_reason.sh` | Unknown reason code → 422 |
| `checkin_retry_reasons_endpoint.sh` | Reason codes listing |
| `checkin_unauthorized.sh` | Check-in blocked without `CheckinWrite` cap |
| `checkin_masking.sh` | PII masking for non-privileged callers |
| `metric_crud_happy_path.sh` | Metric definition lifecycle |
| `metric_publish_requires_admin.sh` | Publish blocked for non-admin → 403 |
| `metric_lineage_validation.sh` | Invalid lineage ref → 422 |
| `metric_version_dependent_widget_flag.sh` | Widget `verification_needed` flag |
| `dashboard_date_filter.sh` | Date-range filter validation |
| `dashboard_unauthorized_returns_403_or_401.sh` | Dashboard access control |
| `dashboard_masking_instructor_workload.sh` | Workload masking for non-admin |
| `dashboard_derived_from_real_data.sh` | Dashboard panels reflect stored data |
| `report_create_and_run.sh` | Report creation and on-demand run |
| `report_department_scope.sh` | Report listing scoped by department |
| `report_scope_isolation.sh` | Single-object scope enforcement → 403 |
| `report_schedule_lifecycle.sh` | Schedule create / update / delete |
| `retention_policy_update.sh` | Retention policy configuration |
| `retention_execute.sh` | Retention execution (dry-run and live) |
| `encryption_field_masking.sh` | AES-256-GCM field encryption round-trip |

## Helper library (`_common.sh`)

Sourced automatically by every script. Do not invoke it directly.

| Symbol | Purpose |
|---|---|
| `BASE` | `${BACKEND_URL}/api/v1` — base URL for all requests |
| `DEFAULT_PASSWORD` | Seed password (`ChangeMe!Scholarly2026`) |
| `login_as <email> [password]` | Log in and return a bearer token |
| `expect_status <want> <got> <label>` | Assert HTTP status; exit 1 on mismatch |
| `pass <label>` | Print a PASS line |
| `fail <label>` | Print a FAIL line and exit 1 |
| `have_jq` | Returns 0 if `jq` is available |
| `json_field <file> <field>` | Extract a top-level JSON field (uses `jq` or `grep`) |
