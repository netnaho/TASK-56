# Scholarly

Offline-first scholarly resources and teaching operations management system.

## Current State: Phase 7 complete + Hardening Pass applied

Phase 1 delivered the clean-architecture skeleton. Phase 2 landed the
security backbone. Phase 3 wired journals, teaching resources, and
attachments. Phase 4 added the academic catalog with prerequisites,
bulk import/export (real CSV + XLSX), and dry-run reporting.
Phase 5 shipped the engagement and analytics core: one-tap check-ins,
a versioned metric semantic layer, and seven dashboards.
Phase 6 completed the offline operational compliance layer: scheduled
local report generation with downloadable CSV/XLSX artifacts,
department-scoped exports, configurable retention policies with secure
deletion, AES-256-GCM field-level encryption for sensitive columns,
and full reporting/retention UI pages.
**Phase 7 closes all integration gaps**: real health check, full user
CRUD, roles API, admin settings page, audit logs page, UX polish
across all screens, expanded unit and API test coverage, and
repository alignment.
A subsequent **hardening pass** addressed three security/correctness gaps:
department scope enforcement on single-object report fetches, instructor
ownership enforcement on resource draft creation, and unknown-role
validation on user creation.
Full details in [`docs/hardening_summary.md`](docs/hardening_summary.md).

## Phase 7: Integration, UX Completion, and Test Expansion

What is now live (full details in [`docs/phase_7_summary.md`](docs/phase_7_summary.md)):

- **Real health check** — `GET /api/v1/health` executes `SELECT 1`
  against the DB pool; returns `{"status":"ok","database":"ok"}` or
  `{"status":"degraded","database":"error","message":"..."}`.
- **Full user CRUD** — `POST /api/v1/users` (create with Argon2id-hashed
  password), `PUT /api/v1/users/<id>` (update name/status/department),
  `DELETE /api/v1/users/<id>` (soft-deactivate with session revocation).
  All admin-only; protected by `AdminOnly` guard and audited.
- **Roles API** — `GET /api/v1/roles` and `GET /api/v1/roles/<id>` return
  role rows with their permission key names from the DB. Admin-only read.
- **Admin settings page** — fully wired: loads all `admin_settings` rows,
  renders inline-editable input per row, saves via `PUT /admin/config/<key>`.
- **Audit logs page** — paginated table with action/date-range filters,
  chain verification button, empty/loading/error states.
- **Expanded test coverage** — unit tests for `retention_service`
  (entity-type validation, audit_logs anonymize-only enforcement),
  `report_service` (cron parsing, format DB round-trip). Three new API
  test scripts: `user_crud.sh`, `audit_log_search.sh`, `admin_config.sh`.
- **`run_tests.sh` improvements** — `--unit-only` and `--api-only` flags,
  pass/fail counters, readable banner output.
- **Repository hygiene** — removed unused `import_export.rs`, cleaned
  `header.rs` and `sidebar.rs` stubs, added
  `JWT_EXPIRATION_HOURS`/`MAX_FAILED_LOGINS`/`LOCKOUT_DURATION_MINUTES`
  to `docker-compose.yml`.

### New environment variables (Phase 7 docker-compose alignment)

| Variable | docker-compose value | Notes |
|----------|---------------------|-------|
| `JWT_EXPIRATION_HOURS` | `8` | Previously only a code default; now explicit |
| `MAX_FAILED_LOGINS` | `5` | Previously only a code default; now explicit |
| `LOCKOUT_DURATION_MINUTES` | `15` | Previously only a code default; now explicit |

## Phase 6: Reporting, Retention, and Encryption

What is now live (full details in [`docs/phase_6_summary.md`](docs/phase_6_summary.md)):

- **Local report scheduler** — in-process Tokio background task polls
  every 60 seconds for due `report_schedules` and generates artifacts.
  No external job-queue infrastructure required.
- **Six report types** — `journal_catalog`, `resource_catalog`,
  `course_catalog`, `checkin_activity`, `audit_summary`, `section_roster`.
  Output formats: CSV and XLSX.
- **Downloadable artifacts** — `GET /api/v1/reports/runs/{id}/download`
  requires a valid bearer token; the server streams the file.
- **Department scope enforcement** — Admin/Librarian see all; others
  see only their department. Filter is applied in SQL, not post-hoc.
- **Retention policies** — four configurable entity types: `audit_logs`
  (default 7 years, anonymise), `sessions` (30 days, delete),
  `operational_events` (3 years, delete), `report_runs` (1 year, delete).
  Dry-run mode reports counts without mutating data.
- **Secure deletion** — artifact files are overwritten with zeros before
  unlinking; MySQL rows are hard-deleted. See
  [`docs/phase_6_summary.md`](docs/phase_6_summary.md#secure-deletion)
  for honest limitations on containerised OverlayFS.
- **AES-256-GCM field encryption** — `section_versions.notes` encrypted
  at rest with a random nonce per write. Storage format:
  `enc:<base64url(nonce||ciphertext||tag)>`. Legacy plaintext rows are
  passed through unchanged (forward-only migration, no re-encryption needed).
  Key set via `FIELD_ENCRYPTION_KEY` env var.

### New environment variables (Phase 6)

| Variable | Default | Notes |
|----------|---------|-------|
| `FIELD_ENCRYPTION_KEY` | 32 zero bytes (base64url) | **Must rotate in production.** Generate: `openssl rand -base64 32 \| tr '+/' '-_' \| tr -d '='` |
| `REPORTS_STORAGE_PATH` | `$ATTACHMENT_STORAGE_PATH/reports` | Where generated report artifact files are stored |

## Phase 5: Engagement & Analytics

What is now live (every item has a code reference in
[`docs/phase_5_summary.md`](docs/phase_5_summary.md)):

- **One-tap check-in** — `POST /api/v1/checkins` with an
  optimistic UI in `frontend/src/pages/checkin.rs`. Device fingerprint
  captured from `navigator.userAgent`, `.platform`, `.language`,
  `window.screen`, and `Intl.DateTimeFormat().resolvedOptions().timeZone`.
- **Duplicate suppression** — configurable via
  `admin_settings.checkin.duplicate_window_minutes` (default 10).
  Duplicate attempts return `409 conflict` and are **persisted** as
  `is_duplicate_attempt = true` so the evidence trail is preserved.
- **Single reasoned retry** — `POST /api/v1/checkins/<id>/retry`
  requires a `reason_code` from the controlled
  `checkin_retry_reasons` table (5 seed values). Max 1 retry per
  original (configurable via `checkin.max_retry_count`).
- **Truthful local-network enforcement** — browsers **cannot** read
  Wi-Fi SSID in normal web security contexts. Phase 5 therefore
  enforces "on-campus" server-side via an admin-maintained
  `checkin.allowed_client_cidrs` JSON array. The canonical check is
  `application::checkin_service::ip_matches_any_cidr`. The
  browser-reported `network_hint` field is recorded for audit but
  never trusted for authorization.
- **Metric semantic layer** — full draft → approved → published →
  archived lifecycle on `metric_definition_versions`, with
  `lineage_refs` (JSON array of `{definition_id, version_id}` pointers)
  validated at write time. `publish_version` requires
  `Capability::MetricApprove` (**admin only**) and atomically flags
  every dependent `dashboard_widgets` row as
  `verification_needed = TRUE`.
- **Seven dashboards**, all computed from stored data:
  `course_popularity`, `fill_rate`, `drop_rate` (approximation),
  `instructor_workload`, `foot_traffic`, `dwell_time` (approximation),
  `interaction_quality`. Every panel surfaces `notes` so callers can
  see the caveats.
- **Role-based masking at the API layer** — `instructor_workload`
  returns a hashed `user:` label for callers without
  `DashboardViewSensitive`; `load_checkin_view` nulls out
  `user_email`, `user_id`, `client_ip`, and `device_fingerprint` for
  non-authorized viewers.

### Route surface (Phase 5)

| Method | Path                                                                | Capability                |
|--------|---------------------------------------------------------------------|---------------------------|
| POST   | `/api/v1/checkins`                                                  | `CheckinWrite`            |
| POST   | `/api/v1/checkins/<id>/retry`                                       | `CheckinWrite`            |
| GET    | `/api/v1/checkins?section_id=...`                                   | `CheckinRead`             |
| GET    | `/api/v1/checkins/retry-reasons`                                    | (authenticated)           |
| GET    | `/api/v1/metrics`                                                   | `MetricRead`              |
| POST   | `/api/v1/metrics`                                                   | `MetricWrite`             |
| GET    | `/api/v1/metrics/<id>`                                              | `MetricRead`              |
| PUT    | `/api/v1/metrics/<id>`                                              | `MetricWrite`             |
| GET    | `/api/v1/metrics/<id>/versions`                                     | `MetricRead`              |
| POST   | `/api/v1/metrics/<id>/versions/<vid>/approve`                       | `MetricWrite`             |
| POST   | `/api/v1/metrics/<id>/versions/<vid>/publish`                       | `MetricApprove` (admin)   |
| POST   | `/api/v1/metrics/widgets/<widget_id>/verify`                        | `MetricApprove` (admin)   |
| GET    | `/api/v1/dashboards/course-popularity?from=&to=&department_id=`     | `DashboardRead`           |
| GET    | `/api/v1/dashboards/fill-rate`                                      | `DashboardRead`           |
| GET    | `/api/v1/dashboards/drop-rate`                                      | `DashboardRead`           |
| GET    | `/api/v1/dashboards/instructor-workload`                            | `DashboardRead`*          |
| GET    | `/api/v1/dashboards/foot-traffic`                                   | `DashboardRead`           |
| GET    | `/api/v1/dashboards/dwell-time`                                     | `DashboardRead`           |
| GET    | `/api/v1/dashboards/interaction-quality`                            | `DashboardRead`           |

\* Instructor names in `instructor_workload` are masked unless the
caller also holds `DashboardViewSensitive` (DepartmentHead + Admin).

### Date filter rules

- `from` / `to` are optional RFC3339. Default window is the last 30 days.
- Maximum window: 366 days. A request outside the range returns
  `422 validation`.
- `from > to` is rejected with `422 validation`.
- Non-admin / non-librarian callers are **pinned to their own
  `department_id`** — passing an explicit `department_id` query
  parameter is ignored for them (verified by a unit test).

## Phase 4: Academic Scheduling & Bulk I/O

## Phase 2: Security Backbone

What is now live (every item has a code reference in
[`docs/security_model.md`](docs/security_model.md)):

- **Real auth endpoints** — `POST /api/v1/auth/login`,
  `POST /api/v1/auth/logout`, `GET /api/v1/auth/me`, and
  `GET /api/v1/users/me`. Tokens are **opaque bearer tokens** (not JWT):
  32 random bytes base64url-encoded on the wire, SHA-256 hashed at rest
  in the `sessions` table, revocable instantaneously.
- **Seed users + bootstrap password** — seed rows carry
  `password_hash = '__BOOTSTRAP__'` so no credential lives in SQL. On
  every startup, `infrastructure::bootstrap::ensure_seed_passwords`
  replaces each sentinel with a real Argon2id hash of
  `ChangeMe!Scholarly2026`. **Rotate this password immediately** on any
  deployment you care about.
- **Argon2id password hashing** with a per-password random salt and a
  **12-character minimum** enforced at set and change time
  (`application::password::MIN_PASSWORD_LENGTH`).
- **Account lockout** — `5` failed attempts within `15` minutes lock the
  account by email. Checked **before** the password is verified to
  avoid timing leaks. Successful logins clear the counter.
- **RBAC capability matrix** — named capabilities across 6 roles
  (Admin, Librarian, Auditor, DepartmentHead, Instructor, Viewer),
  canonical in `application::authorization::role_allows`. Three
  enforcement layers: route guards (`AuthedPrincipal`, `AdminOnly`),
  service-level `require(cap)`, and object-level
  `scope::require_object_visible`. The `Auditor` role (added in the
  hardening pass) holds `AuditRead` and `ReportRead`; it has no write
  or admin capabilities.
- **Admin-only endpoints** — every route under `/api/v1/admin/config/*`
  and `/api/v1/audit-logs/verify-chain` is behind `AdminOnly`. Audit
  log search (`GET /api/v1/audit-logs`) requires `Capability::AuditRead`
  (Admin and Auditor). Audit log CSV export requires the separate
  `Capability::AuditExport` (Admin only).
- **Audit hash chain** — SHA-256 chain appended inside a transaction
  with `SELECT ... FOR UPDATE` on the tip. `verify-chain` walks every
  entry and reports the first inconsistency.
- **Standardized error envelope** — every failure returns the same
  JSON shape, and `Internal` / `Database` detail is never leaked to
  clients. Example:

  ```json
  {
    "error": {
      "code": "unauthorized",
      "message": "Authentication required",
      "request_id": "b6c71f42-2d0c-4c1c-9c91-28f2e3a26c09"
    }
  }
  ```

  `code` is snake_case; `fields` is present only for per-field
  validation errors; `request_id` correlates with server-side logs.

### Running the API test suite

Phase 2 ships a shell-based end-to-end security suite in `API_tests/`
that exercises login success, bad-password failure, lockout,
logout-revokes-session, unauthenticated access, admin endpoint
protection, admin config writes, and full audit-log search + chain
verification. Because the tests hit the live HTTP surface, the backend
must be up:

```bash
# Terminal 1 — stand up MySQL, backend, frontend
docker compose up

# Terminal 2 — run unit tests + API tests
./run_tests.sh
```

`run_tests.sh` will automatically skip the API tests if the backend
health probe is unreachable, so it is safe to run in CI without the
compose stack as well.

## Phase 4: Academic Scheduling & Bulk I/O

What is now live (every item has a code reference in
[`docs/api_surface.md`](docs/api_surface.md),
[`docs/domain_model.md`](docs/domain_model.md), and
[`docs/phase_4_summary.md`](docs/phase_4_summary.md)):

- **Course catalog CRUD + versioning** — `create_course`,
  `create_draft_version`, `approve_version`, `publish_version`,
  `list_courses`, `get_course_by_id`, `list_versions` in
  `backend/src/application/course_service.rs`. Same state machine
  and two-pointer invariant as Phase 3 journals. Validators for course
  code format (`[A-Z]{2,5}[-_]?[0-9]{3,4}[A-Z]?`), title (3..=500),
  credit hours (0.5..=20.0), contact hours (0.5..=30.0).
- **Section management + versioning** —
  `backend/src/application/section_service.rs`. Term enum
  (`fall|spring|summer|winter`, normalised to lowercase), year
  2000..=2100, capacity 1..=1000, `section_code` 20-char alphanumeric.
  Unique `(course_id, section_code, term, year)`. Instructors can only
  edit sections they personally teach; `DepartmentHead` is the
  "Academic Scheduler" role for their department.
- **Prerequisite graph with cycle detection** —
  `course_prerequisites` adjacency list (migration 013). Service layer
  rejects self-references (422), duplicate edges (409), and cycles via
  a DFS reachability walk (`ensure_no_cycle` in `course_service.rs`).
  Routes: `GET/POST /courses/<id>/prerequisites` and
  `DELETE /courses/<id>/prerequisites/<prereq_id>`.
- **Department-scoped exports** — CSV and real XLSX (via
  `rust_xlsxwriter`). Scope is enforced **in SQL**
  (`export_service::scope_department`); non-admin/non-librarian
  callers get `AND c.department_id = ?` appended with no bypass.
  Responses flow through `api::download::BinaryDownload`, which sets
  `Content-Type` and `Content-Disposition: attachment; filename=...`.
- **Empty templates** — `GET /{courses|sections}/template.{csv|xlsx}`
  returns a header-only file that round-trips through the importer.
  XLSX templates are real ZIP archives (test
  `xlsx_template_is_real_xlsx` asserts the `PK` magic bytes).
- **Bulk CSV + real XLSX import** — `POST /{courses|sections}/import`
  (multipart `file` + `mode=dry_run|commit`). CSV via the `csv` crate,
  XLSX via `calamine` reading the first worksheet. Row-level
  validation uses the exact same validators as the direct create
  path. Header detection is case-insensitive; required columns
  rejected up front with `422 validation`.
- **Dry-run vs. commit semantics** — dry-run **never writes** to
  `courses`, `sections`, `course_versions`, `section_versions`, or
  `course_prerequisites`. Commit is **all-or-nothing**: if any row
  has errors the commit is aborted with a `failed` `import_jobs`
  envelope and `422 validation`; otherwise every insert runs inside a
  single `pool.begin()...commit()` transaction.
- **Row-level error reporting** — the response is an `ImportReport`
  carrying `total_rows`, `valid_rows`, `error_rows`, `committed`, and
  `rows: [{ row_index, ok, errors: [{field, message}], parsed? }]`.
  `row_index` is 1-based with the header at row 1 (first data row =
  2), identical for CSV and XLSX. `parsed` shows the exact values
  that would be committed, populated only when `ok = true`.
- **RBAC additions** — `CourseApprove`, `CoursePublish`,
  `SectionApprove`, `SectionPublish`, `ImportCourses`,
  `ImportSections`, `ExportCourses`, `ExportSections` capabilities
  in `application::authorization::Capability`, granted via
  `role_allows`: DepartmentHead gets full course/section CRUD +
  approve + publish + import + export; Instructor gets read +
  section drafts + course/section exports; Librarian keeps read-only
  on the catalog plus export; Admin keeps full access.

### Route surface (Phase 4)

| Method | Path                                                           | Capability        |
|--------|----------------------------------------------------------------|-------------------|
| GET    | `/api/v1/courses?department_id=&limit=&offset=`                | `CourseRead`      |
| GET    | `/api/v1/courses/<id>`                                         | `CourseRead`      |
| POST   | `/api/v1/courses`                                              | `CourseWrite`     |
| PUT    | `/api/v1/courses/<id>`                                         | `CourseWrite`     |
| GET    | `/api/v1/courses/<id>/versions`                                | `CourseWrite`     |
| GET    | `/api/v1/courses/<id>/versions/<vid>`                          | `CourseWrite`     |
| POST   | `/api/v1/courses/<id>/versions/<vid>/approve`                  | `CourseApprove`   |
| POST   | `/api/v1/courses/<id>/versions/<vid>/publish`                  | `CoursePublish`   |
| GET    | `/api/v1/courses/<id>/prerequisites`                           | `CourseRead`      |
| POST   | `/api/v1/courses/<id>/prerequisites`                           | `CourseWrite`     |
| DELETE | `/api/v1/courses/<id>/prerequisites/<prereq_id>`               | `CourseWrite`     |
| GET    | `/api/v1/courses/template.csv`, `/template.xlsx`               | `AuthedPrincipal` |
| GET    | `/api/v1/courses/export.csv`, `/export.xlsx`                   | `ExportCourses`   |
| POST   | `/api/v1/courses/import?mode=dry_run\|commit`                  | `ImportCourses`   |
| GET    | `/api/v1/sections?course_id=&department_id=&limit=&offset=`    | `SectionRead`     |
| GET    | `/api/v1/sections/<id>`                                        | `SectionRead`     |
| POST   | `/api/v1/sections`                                             | `SectionWrite`    |
| PUT    | `/api/v1/sections/<id>`                                        | `SectionWrite`    |
| GET    | `/api/v1/sections/<id>/versions`                               | `SectionWrite`    |
| POST   | `/api/v1/sections/<id>/versions/<vid>/approve`                 | `SectionApprove`  |
| POST   | `/api/v1/sections/<id>/versions/<vid>/publish`                 | `SectionPublish`  |
| GET    | `/api/v1/sections/template.csv`, `/template.xlsx`              | `AuthedPrincipal` |
| GET    | `/api/v1/sections/export.csv`, `/export.xlsx`                  | `ExportSections`  |
| POST   | `/api/v1/sections/import?mode=dry_run\|commit`                 | `ImportSections`  |

### Column contract — `courses.csv` / `courses.xlsx`

| Column            | Required | Notes                                                                 |
|-------------------|:--------:|-----------------------------------------------------------------------|
| `code`            |    Y     | `[A-Z]{2,5}[-_]?[0-9]{3,4}[A-Z]?` — e.g. `CS101`, `MATH-210A`.        |
| `title`           |    Y     | 3..=500 chars.                                                        |
| `department_code` |    Y     | Resolved via `departments.code` (case-sensitive). Must match the caller's own department unless the caller is admin. Empty value is admin-only. |
| `credit_hours`    |    Y     | Number, 0.5..=20.0.                                                   |
| `contact_hours`   |    Y     | Number, 0.5..=30.0.                                                   |
| `description`     |    N     | Optional free text, max 4,000 chars.                                  |
| `prerequisites`   |    N     | `;`-separated list of course codes. Each must be a valid code; self-references rejected. Unresolved codes are silently skipped in the commit second pass. |

### Column contract — `sections.csv` / `sections.xlsx`

| Column             | Required | Notes                                                                 |
|--------------------|:--------:|-----------------------------------------------------------------------|
| `course_code`      |    Y     | Must match an existing course. Non-admins can only target courses in their own department. |
| `section_code`     |    Y     | Alphanumeric + `-`/`_`, ≤ 20 chars.                                   |
| `term`             |    Y     | One of `fall`, `spring`, `summer`, `winter` (case-insensitive).       |
| `year`             |    Y     | Integer, 2000..=2100.                                                 |
| `capacity`         |    Y     | Integer, 1..=1000.                                                    |
| `instructor_email` |    N     | Must resolve to an existing user if provided.                         |
| `location`         |    N     | Optional, ≤ 255 chars.                                                |
| `schedule_note`    |    N     | Optional, ≤ 500 chars. Stored in `section_versions.schedule_json`.    |
| `notes`            |    N     | Optional, ≤ 2,000 chars.                                              |

Header names are case-insensitive. Extra columns are ignored; missing
required columns return `422 validation` before any row is validated.

### Dry-run semantics

- **Dry-run never writes to the business tables.** The only side
  effects are a single `import_jobs` envelope row (status
  `validated`) and an `import.dry_run` audit event.
- **Commit is all-or-nothing.** If any row has errors the commit is
  aborted with `422 validation` and no business rows are written; a
  `failed` envelope row and an `import.commit.failed` audit event are
  persisted. Otherwise every insert runs inside a single transaction
  that rolls back on any mid-flight failure.

## Phase 3: Library Serials & Teaching Resources

What is now live (every item has a code reference in
[`docs/api_surface.md`](docs/api_surface.md) and
[`docs/domain_model.md`](docs/domain_model.md)):

- **Versioning workflow** — journals and teaching resources both
  support `create_journal` / `create_resource` (version #1 in `draft`)
  and `create_draft_version` (version_number = max + 1, locked with
  `SELECT ... FOR UPDATE`). Version content columns are append-only.
- **State machine** — `draft → approved → published`, with archive
  legal from any non-archived state. Defined once in
  `backend/src/domain/versioning.rs::validate_transition` and called by
  both services before every state-changing write. Illegal transitions
  return `409 conflict`.
- **Two-pointer invariant** — every `journals` / `teaching_resources`
  row tracks `current_version_id` (the published baseline, at most one
  per parent) and `latest_version_id` (the head draft pointer). A
  single transaction in `publish_version` archives any previously
  published version and repoints `current_version_id`.
- **Attachment upload with SHA-256** — `POST /api/v1/attachments`
  accepts `multipart/form-data` with `file`, `parent_type`,
  `parent_id`, and optional `category`. The service computes a hex
  SHA-256 server-side before the database insert and writes bytes to
  `{storage_root}/{entity_type}/{uuid}` via
  `infrastructure::storage::LocalAttachmentStorage`. 50 MiB cap,
  10-entry MIME whitelist, filename sanitation.
- **Preview whitelist** — `GET /api/v1/attachments/<id>/preview`
  streams the stored bytes with the original `Content-Type`, an
  `X-Attachment-Checksum: sha256:<hex>` header, and an
  `X-Attachment-Filename` header (ASCII-stripped). Only MIME types in
  `PREVIEWABLE_MIME` (a strict subset of the upload whitelist, no
  Office formats) are served; anything else returns `422 validation`.
  The SHA-256 is recomputed from the bytes read from disk and compared
  to the stored checksum before the body leaves the server.
- **RBAC additions** — `JournalApprove`, `JournalPublish`,
  `ResourceApprove`, `ResourcePublish`, `AttachmentRead`,
  `AttachmentWrite`, and `AttachmentDelete` capabilities, all granted
  via the canonical `role_allows` matrix in
  `application::authorization`. Librarians and admins get the full
  library workflow; instructors can draft resources and upload
  attachments they own; viewers can read published content and
  previews.

### Route surface (Phase 3)

| Method | Path                                                         | Capability        |
|--------|--------------------------------------------------------------|-------------------|
| GET    | `/api/v1/journals`                                           | `JournalRead`     |
| GET    | `/api/v1/journals/<id>`                                      | `JournalRead`     |
| POST   | `/api/v1/journals`                                           | `JournalWrite`    |
| PUT    | `/api/v1/journals/<id>`                                      | `JournalWrite`    |
| GET    | `/api/v1/journals/<id>/versions`                             | `JournalWrite`    |
| GET    | `/api/v1/journals/<id>/versions/<vid>`                       | `JournalRead`     |
| POST   | `/api/v1/journals/<id>/versions/<vid>/approve`               | `JournalApprove`  |
| POST   | `/api/v1/journals/<id>/versions/<vid>/publish`               | `JournalPublish`  |
| GET    | `/api/v1/teaching-resources`                                 | `ResourceRead`    |
| GET    | `/api/v1/teaching-resources/<id>`                            | `ResourceRead`    |
| POST   | `/api/v1/teaching-resources`                                 | `ResourceWrite`   |
| PUT    | `/api/v1/teaching-resources/<id>`                            | `ResourceWrite`   |
| GET    | `/api/v1/teaching-resources/<id>/versions`                   | `ResourceWrite`   |
| GET    | `/api/v1/teaching-resources/<id>/versions/<vid>`             | `ResourceRead`    |
| POST   | `/api/v1/teaching-resources/<id>/versions/<vid>/approve`     | `ResourceApprove` |
| POST   | `/api/v1/teaching-resources/<id>/versions/<vid>/publish`     | `ResourcePublish` |
| GET    | `/api/v1/attachments?parent_type=&parent_id=`                | `AttachmentRead`  |
| GET    | `/api/v1/attachments/<id>`                                   | `AttachmentRead`  |
| POST   | `/api/v1/attachments` (multipart)                            | `AttachmentWrite` |
| GET    | `/api/v1/attachments/<id>/preview`                           | `AttachmentRead`  |
| DELETE | `/api/v1/attachments/<id>`                                   | `AttachmentDelete`|

### Example `JournalView` response

```json
{
  "id": "b1a2c3d4-5678-4abc-9def-0123456789ab",
  "title": "Library Quarterly",
  "abstract_text": "Summary shown in listings",
  "author_id": "30000000-0000-0000-0000-000000000002",
  "is_published": true,
  "current_version_id": "f0e1d2c3-b4a5-4968-8778-6f5e4d3c2b1a",
  "latest_version_id": "f0e1d2c3-b4a5-4968-8778-6f5e4d3c2b1a",
  "created_at": "2026-04-11T10:11:12",
  "updated_at": "2026-04-11T11:22:33",
  "effective_version": {
    "id": "f0e1d2c3-b4a5-4968-8778-6f5e4d3c2b1a",
    "journal_id": "b1a2c3d4-5678-4abc-9def-0123456789ab",
    "version_number": 3,
    "title": "Library Quarterly",
    "body": "Full body text of version 3",
    "change_summary": "Corrected the introduction",
    "state": "published",
    "created_by": "30000000-0000-0000-0000-000000000002",
    "created_at": "2026-04-11T10:11:12",
    "approved_by": "30000000-0000-0000-0000-000000000002",
    "approved_at": "2026-04-11T10:20:00",
    "published_by": "30000000-0000-0000-0000-000000000002",
    "published_at": "2026-04-11T10:25:00"
  }
}
```

`effective_version` is the published baseline for read-only callers
and the head draft for editors (callers holding `JournalWrite`); a
reader who asks for a journal with no baseline receives `404 not_found`.

### Version retention and publish semantics

Prior versions are **retained after later edits and archival**: every
draft append inserts a new row in `journal_versions` /
`resource_versions` and repoints `latest_version_id`, never mutating
the previous row. `publish_version` is the **only** code path that
moves `current_version_id`; it runs inside a single transaction that
archives the previously-published row and sets the new one, so
`journals.current_version_id` can never point at a row whose `state`
is not `published`.

## Phase 1 — Architecture Foundation

This repository contains the **architectural skeleton** of the Scholarly system. All module boundaries, database schema, Docker automation, and requirement traceability are in place. *(Historical record as of Phase 1: domain business features were stubs at that point; all are now fully implemented through Phase 7.)*

## Stack

| Layer    | Technology          | Version |
|----------|---------------------|---------|
| Frontend | Dioxus (Web)        | 0.6     |
| Backend  | Rocket (Rust)       | 0.5     |
| Database | MySQL               | 8.0     |
| Auth     | Opaque bearer + Argon2id | —   |
| Storage  | Local filesystem    | —       |
| Deploy   | Docker Compose      | 3.9     |

## Architecture

```
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│   Frontend   │───▶│   Backend    │───▶│    MySQL     │
│  Dioxus SPA  │    │  Rocket API  │    │    8.0       │
│  nginx:3000  │    │  :8000       │    │  :3306       │
└──────────────┘    └──────────────┘    └──────────────┘
                           │
                    ┌──────┴──────┐
                    │ Attachments │
                    │ (local vol) │
                    └─────────────┘
```

Backend follows clean architecture: `api → application → domain ← infrastructure`

## Quick Start

```bash
docker compose up
```

That's it. The system:
1. Starts MySQL and waits for it to be healthy
2. Runs all database migrations automatically
3. Seeds default roles, permissions, departments, and users
4. Starts the Rocket API server on port 8000
5. Builds and serves the Dioxus frontend on port 3000

### Exposed Ports

| Service  | Port | Description               |
|----------|------|---------------------------|
| Frontend | 3000 | Dioxus SPA (via nginx)    |
| Backend  | 8000 | Rocket REST API           |
| MySQL    | 3306 | Database (direct access)  |

### Default Seed Users

| Role            | Email                        | Default password         |
|-----------------|------------------------------|--------------------------|
| Admin           | admin@scholarly.local        | `ChangeMe!Scholarly2026` |
| Librarian       | librarian@scholarly.local    | `ChangeMe!Scholarly2026` |
| Instructor      | instructor@scholarly.local   | `ChangeMe!Scholarly2026` |
| Department Head | depthead@scholarly.local     | `ChangeMe!Scholarly2026` |
| Viewer          | viewer@scholarly.local       | `ChangeMe!Scholarly2026` |
| Auditor         | auditor@scholarly.local      | `ChangeMe!Scholarly2026` |

Seed rows ship with `password_hash = '__BOOTSTRAP__'` so no credential
material lives in SQL. On every startup,
`infrastructure::bootstrap::ensure_seed_passwords` rewrites each sentinel
with a fresh Argon2id hash of the default above. **Rotate these
credentials immediately** — the bootstrap runs unconditionally and is
intended for first-boot provisioning only.

## Folder Structure

```
repo/
├── README.md
├── docker-compose.yml
├── .gitignore
├── .dockerignore
├── run_tests.sh
├── docs/
│   ├── architecture.md
│   ├── domain_model.md
│   ├── api_surface.md
│   ├── requirement_traceability.md
│   ├── security_model.md
│   ├── phase_1_summary.md
│   ├── phase_2_summary.md
│   ├── phase_3_summary.md
│   └── phase_4_summary.md
├── backend/
│   ├── Cargo.toml
│   ├── Dockerfile
│   ├── src/
│   │   ├── main.rs
│   │   ├── lib.rs
│   │   ├── api/          # Route handlers (16 modules)
│   │   ├── application/  # Use-case services (11 modules)
│   │   ├── domain/       # Business entities (13 modules)
│   │   ├── infrastructure/ # Repos, DB, storage
│   │   ├── config/       # Environment configuration
│   │   └── errors/       # Unified error types
│   ├── migrations/       # 13 SQL migration files
│   ├── seeds/            # 4 deterministic seed files
│   ├── scripts/          # entrypoint.sh (startup automation)
│   └── tests/            # Integration tests
├── frontend/
│   ├── Cargo.toml
│   ├── Dioxus.toml
│   ├── Dockerfile
│   ├── nginx.conf
│   ├── assets/
│   └── src/
│       ├── main.rs
│       ├── router.rs
│       ├── pages/        # 13 page components
│       ├── layouts/      # Main layout with role-aware nav
│       ├── components/   # Reusable UI components
│       ├── hooks/        # Auth and API hooks
│       ├── api/          # HTTP client
│       ├── state/        # Application state
│       ├── types/        # Shared types
│       └── routes/       # (reserved)
├── unit_tests/
└── API_tests/
```

## Current Implementation Status

All phases through Phase 7 are complete, with a subsequent security and
correctness hardening pass applied. The list below summarises what is
fully implemented and in production. See the per-phase sections above
for code references.

| Domain | Status |
|--------|--------|
| Auth (login / logout / me / sessions / lockout) | Implemented — Phase 2 |
| User CRUD + roles API | Implemented — Phase 7 |
| Admin config + retention policies | Implemented — Phase 2 / 6 |
| Journals + teaching resources (draft → approve → publish → archive) | Implemented — Phase 3 |
| Attachments (upload / preview / delete, checksums, MIME whitelist) | Implemented — Phase 3 |
| Course catalog + section management + prerequisite graph | Implemented — Phase 4 |
| Bulk CSV + XLSX import/export with dry-run and all-or-nothing commit | Implemented — Phase 4 |
| Check-ins with duplicate detection, retry, and network enforcement | Implemented — Phase 5 |
| Metric semantic layer (versioned definitions, publish lifecycle) | Implemented — Phase 5 |
| Seven dashboards with role-based masking | Implemented — Phase 5 |
| Scheduled local report generation (CSV + XLSX artifacts) | Implemented — Phase 6 |
| Configurable retention policies with secure deletion | Implemented — Phase 6 |
| AES-256-GCM field-level encryption (`section_versions.notes`) | Implemented — Phase 6 |
| Audit log search + hash-chain verification | Implemented — Phase 2 / 7 |
| Audit log CSV export (`GET /api/v1/audit-logs/export.csv`) | Implemented — hardening pass |
| `Auditor` role (AuditRead + ReportRead, no write/admin capabilities) | Implemented — hardening pass |
| Department scope enforcement on single-object report fetches | Implemented — hardening pass |
| Frontend SPA with role-aware navigation (all 6 roles) | Implemented — Phase 7 |

## What Is Implemented (Phase 1 – Phase 4 detail)

- Full repository structure with clean module boundaries
- Docker Compose with MySQL 8.0, backend, and frontend services
- Automated startup: DB wait → migrations → seeds → server
- 13 SQL migration files (migration 012 added the Phase 3 versioning
  and attachment columns; migration 013 adds the Phase 4 academic
  extensions — course/section state machine, contact hours, two
  `latest_version_id` pointers, `course_prerequisites`, `import_jobs`)
- Seed files with deterministic default data
- Phase 2: real auth endpoints, opaque bearer sessions, Argon2id
  password hashing, account lockout, audit hash chain, admin config,
  standardized error envelope
- Phase 3: real journal and teaching-resource workflows with
  draft / approve / publish state machine, two-pointer baseline
  invariant, and append-only version history
- Phase 3: real attachment upload / list / get / preview / delete
  with server-side SHA-256 checksums, MIME whitelists (upload and
  preview), path-traversal defenses, soft-delete with best-effort
  unlink
- Phase 4: real course catalog and section management with the same
  versioning state machine and two-pointer invariant, prerequisite
  adjacency graph with cycle detection, validator bounds for code /
  title / credits / contact hours / capacity / term / year
- Phase 4: real CSV and XLSX bulk import (dry-run and all-or-nothing
  commit, row-level error reporting via `ImportReport`) and
  department-scoped CSV and XLSX exports enforced in SQL, plus
  empty templates
- Frontend: page components, role-aware navigation, routing, library
  API modules (`frontend/src/api/journals.rs`,
  `frontend/src/api/resources.rs`, `frontend/src/api/courses.rs`,
  `frontend/src/api/sections.rs`, `frontend/src/api/imports.rs`)
- Unified error handling structure
- Configuration from environment variables
- Comprehensive documentation and requirement traceability

## Known Gaps and Future Work

The items below are genuinely not implemented. Everything crossed off
from an earlier version of this list has been delivered and is tracked
in the "Current Implementation Status" table above.

**Encryption scope** — AES-256-GCM field-level encryption covers
`section_versions.notes`. MySQL Transparent Data Encryption (TDE) and
attachment-file encryption are not implemented; the attachment store
relies on filesystem permissions on the Docker volume only.

**Auth hardening (no roadmap date)**
- Password self-service reset / forgot-password flow
- Multi-factor authentication

**Frontend richness (no roadmap date)**
- Rich-text editor in the journal body field (current: plain textarea)
- Diff view between versions of a course, journal, or section

**Import / upload limits (no roadmap date)**
- Multi-attachment atomic uploads and resumable large-file uploads
- Background-job execution for very large imports (current path is
  synchronous and will time out on payloads beyond a few thousand rows)

**Workflow completeness (no roadmap date)**
- Explicit rejection path in the approval workflow (current: objects
  can only be returned to draft by re-editing, not via a formal reject
  action with a required reason)
- Prerequisite OR-groups (Phase 4 models prerequisites as a strict
  AND-list; `course_prerequisites` has no group discriminator column)
- Schedule conflict detection across sections (room / time / instructor
  overlap is not checked server-side)

**Operational hardening (no roadmap date)**
- Virus scanning on uploaded attachment bytes
- Richer field-level input validation beyond shape checks (UUID,
  RFC3339, string length)

## Running Tests

### Default (local dev — no live DB required)

```bash
./run_tests.sh
```

Runs backend unit tests, cargo integration tests, frontend tests, and the
shell-based API suite (skipped automatically if the backend is not
reachable). The 5 DB-backed authz integration tests in
`backend/tests/api_routes_test.rs` are **skipped with a visible notice**
when `SCHOLARLY_TEST_DB_URL` is not set — the run still exits 0.

### With DB integration tests (local, stack running)

```bash
# Start the compose stack first (needed once per session)
docker compose up -d

SCHOLARLY_TEST_DB_URL=mysql://scholarly_app:scholarly_app_pass@localhost:3306/scholarly \
  ./run_tests.sh
```

The 5 tests cover critical authorization and scope-enforcement paths that
unit tests cannot exercise end-to-end:

| # | Test | Assertion |
|---|------|-----------|
| 1 | `db_unauthenticated_request_is_rejected_with_401` | No `Authorization` header → 401 on a guarded route |
| 2 | `db_non_admin_is_forbidden_on_admin_only_endpoint` | Librarian → 403 on `AdminOnly` route; Admin → 200 |
| 3 | `db_report_schedule_listing_enforces_department_scope` | DeptHead → 403 on out-of-scope report schedules |
| 4 | `db_audit_export_denied_to_non_exporting_roles` | Viewer and Librarian → 403 on export endpoint |
| 5 | `db_admin_audit_export_returns_200_with_valid_csv` | Admin → 200, `text/csv`, correct 10-column header |

### Strict integration mode (CI — fails if DB is absent)

Add `--strict-integration` to make the run exit non-zero when
`SCHOLARLY_TEST_DB_URL` is unset. Use this in CI pipelines to prevent the
DB integration tests from being silently skipped:

```bash
SCHOLARLY_TEST_DB_URL=mysql://scholarly_app:scholarly_app_pass@localhost:3306/scholarly \
  ./run_tests.sh --strict-integration
```

Without `SCHOLARLY_TEST_DB_URL` set this exits 1 with a clear error message.

### Other flags

```bash
./run_tests.sh --unit-only   # cargo tests only (no shell API suite)
./run_tests.sh --api-only    # shell API suite only (requires live backend)
```

### Running DB integration tests in isolation

```bash
cd backend
SCHOLARLY_TEST_DB_URL=mysql://scholarly_app:scholarly_app_pass@localhost:3306/scholarly \
  cargo test --test api_routes_test -- --test-threads=1
```

### CI enforcement

CI is configured under `.github/workflows/` and runs on every push to `main`
and on every pull request.  Two workflows are active:

| Workflow | File | Purpose |
|----------|------|---------|
| **CI** | `ci.yml` | Full suite: lint, backend unit, DB integration (strict), frontend |
| **DB Security Gate** | `db-security.yml` | Dedicated check for `api_routes_test` — appears as its own PR status |

#### How silent skips are prevented in CI

The five DB-backed security tests in `backend/tests/api_routes_test.rs` skip
gracefully when `SCHOLARLY_TEST_DB_URL` is absent.  `cargo test` still reports
them as "passed", creating a false-green risk.  CI uses three independent
layers to prevent this:

1. **Job-level `env:`** — `SCHOLARLY_TEST_DB_URL` is hard-coded in every DB
   job so the variable is always set for all steps in the job.
2. **Guard step** — a pre-flight shell check (`[ -z "${SCHOLARLY_TEST_DB_URL:-}" ]`)
   fails the job before the test runner starts if the variable is somehow empty.
3. **`--strict-integration` flag** — `./run_tests.sh --strict-integration` exits
   1 on its own when `SCHOLARLY_TEST_DB_URL` is absent, providing a third
   independent check independent of the guard step.
4. **`[SKIP]` output scan** (`db-security.yml` only) — after the test run,
   CI greps the captured output for `[SKIP]` tokens.  If any appear it means
   the DB was unreachable despite the URL being set; the job fails with a
   diagnostic message.

#### Required environment variables

No secrets need to be configured in repository settings for CI: the MySQL
credentials are the default seed values used in local development and are
embedded directly in the workflow files.

| Variable | Set in CI | Value |
|----------|-----------|-------|
| `SCHOLARLY_TEST_DB_URL` | Yes (job `env:`) | `mysql://scholarly_app:scholarly_app_pass@127.0.0.1:3306/scholarly` |
| `SCHOLARLY_TEST_DB_URL` | Local dev (optional) | `mysql://scholarly_app:scholarly_app_pass@localhost:3306/scholarly` |

#### MySQL service provisioning in CI

CI uses a GitHub Actions [service container](https://docs.github.com/en/actions/using-containerized-services)
instead of `docker compose`.  Before running tests, CI applies all
`backend/migrations/*.sql` and `backend/seeds/*.sql` files in sort order —
the same sequence that `backend/scripts/entrypoint.sh` follows.  Seed users
are inserted with `__BOOTSTRAP__` password sentinels; the Rocket test harness
calls `build_rocket()` which triggers `infrastructure/bootstrap.rs` on startup
and replaces them with real Argon2id hashes automatically.

The shell-based API tests (`API_tests/*.sh`) are skipped in CI because they
require a live HTTP server.  They remain part of the local dev workflow via
`./run_tests.sh` (with the backend running) or `./run_tests.sh --api-only`.

## Constraints and Assumptions

- **Offline-only**: No external network dependencies. All services run locally.
- **Single-command startup**: `docker compose up` is the only required command.
- **Local file storage**: Attachments stored on a Docker volume, not cloud storage.
- **Strict RBAC**: 6 roles (Admin, Librarian, Auditor, DepartmentHead, Instructor, Viewer) with granular capabilities. Department-scoped data access.
- **Audit immutability**: Append-only audit log with SHA-256 chained hashes.
- **Versioning**: Journals, resources, courses, sections, and metric definitions are versioned entities.
- **MySQL 8.0**: Chosen for JSON column support, window functions, and CTEs.

## Documentation

| Document | Description |
|----------|-------------|
| [Architecture](docs/architecture.md) | System design and service topology |
| [Domain Model](docs/domain_model.md) | Entity relationships and versioning strategy |
| [API Surface](docs/api_surface.md) | All endpoints with methods and permissions |
| [Requirement Traceability](docs/requirement_traceability.md) | Requirement-to-module mapping |
| [Security Model](docs/security_model.md) | Auth, RBAC, audit, masking, error envelope (Phase 2) |
| [Phase 1 Summary](docs/phase_1_summary.md) | Architecture skeleton |
| [Phase 2 Summary](docs/phase_2_summary.md) | Security backbone completion report |
| [Phase 3 Summary](docs/phase_3_summary.md) | Library serials, teaching resources, versioning, attachments |
| [Phase 4 Summary](docs/phase_4_summary.md) | Academic scheduling, prerequisites, bulk CSV/XLSX import/export |
