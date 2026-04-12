# Scholarly API Specification

This document describes the **current implemented API surface** for the Scholarly backend.

---

## 1. API Fundamentals

- **Base path:** `/api/v1`
- **Auth model:** Opaque bearer token (not JWT)
- **Auth header:** `Authorization: Bearer <token>`
- **Primary content type:** `application/json`
- **Binary/file endpoints:** CSV/XLSX downloads, attachment preview, multipart uploads

### 1.1 Standard Error Envelope

All failures return this shape:

```json
{
  "error": {
    "code": "validation",
    "message": "Validation error",
    "request_id": "uuid",
    "fields": {
      "field_name": ["reason"]
    }
  }
}
```

- `fields` is only present for validation-style failures.

### 1.2 Common Status Codes

| Status | Meaning                                                   |
| ------ | --------------------------------------------------------- |
| `200`  | Success                                                   |
| `202`  | Accepted/queued (report run trigger)                      |
| `401`  | Missing/invalid token                                     |
| `403`  | Authenticated but not authorized                          |
| `404`  | Not found                                                 |
| `409`  | Conflict (state transition, duplicate, strict-mode block) |
| `422`  | Validation error                                          |
| `429`  | Account locked                                            |
| `500`  | Internal/database/file-system failure                     |

---

## 2. Authentication & Session Endpoints

### 2.1 Routes

| Method | Path           | Auth         |
| ------ | -------------- | ------------ |
| `POST` | `/auth/login`  | Public       |
| `POST` | `/auth/logout` | Bearer token |
| `GET`  | `/auth/me`     | Bearer token |

### 2.2 `POST /auth/login`

Request:

```json
{
  "email": "admin@scholarly.local",
  "password": "ChangeMe!Scholarly2026"
}
```

Success response contains token, expiry, and user identity/roles.

Lockout policy is enforced before password verification (`5` failures in `15` minutes).

---

## 3. Identity, Roles, and Admin Configuration

### 3.1 Users (`/users`)

| Method   | Path          | Guard         |
| -------- | ------------- | ------------- |
| `GET`    | `/users/me`   | Authenticated |
| `GET`    | `/users`      | AdminOnly     |
| `GET`    | `/users/{id}` | AdminOnly     |
| `POST`   | `/users`      | AdminOnly     |
| `PUT`    | `/users/{id}` | AdminOnly     |
| `DELETE` | `/users/{id}` | AdminOnly     |

#### Create user request (`POST /users`)

```json
{
  "email": "new.user@scholarly.local",
  "display_name": "New User",
  "password": "StrongPassword123!",
  "department_id": "uuid-or-null",
  "roles": ["viewer"]
}
```

Notes:

- Password min length is 12.
- Unknown role names return `422 validation`.
- Duplicate email returns `409 conflict`.
- Deactivate is soft-delete (`status=deactivated`) and revokes sessions.

### 3.2 Roles (`/roles`) — read-only

| Method | Path          | Guard     |
| ------ | ------------- | --------- |
| `GET`  | `/roles`      | AdminOnly |
| `GET`  | `/roles/{id}` | AdminOnly |

Role response includes `id`, `name`, `display_name`, `description`, and resolved permission key names.

### 3.3 Admin config (`/admin/config`)

| Method | Path                  | Guard     |
| ------ | --------------------- | --------- |
| `GET`  | `/admin/config`       | AdminOnly |
| `GET`  | `/admin/config/{key}` | AdminOnly |
| `PUT`  | `/admin/config/{key}` | AdminOnly |

Used for operational knobs (check-in windows, allowed CIDRs, auth/retention settings, attachment limits, report settings, etc.).

---

## 4. Audit Endpoints

Base: `/audit-logs`

| Method | Path                       | Capability/Guard |
| ------ | -------------------------- | ---------------- |
| `GET`  | `/audit-logs`              | `AuditRead`      |
| `GET`  | `/audit-logs/export.csv`   | `AuditExport`    |
| `GET`  | `/audit-logs/verify-chain` | AdminOnly        |

Search query params (optional): `actor_id`, `action`, `target_entity_type`, `target_entity_id`, `from`, `to`, `limit`.

Audit CSV export uses the same filter model and returns an attachment file.

---

## 5. Library Domain APIs

## 5.1 Journals (`/journals`)

| Method | Path                                           | Capability       |
| ------ | ---------------------------------------------- | ---------------- |
| `GET`  | `/journals?limit=&offset=`                     | `JournalRead`    |
| `GET`  | `/journals/{id}`                               | `JournalRead`    |
| `POST` | `/journals`                                    | `JournalWrite`   |
| `PUT`  | `/journals/{id}`                               | `JournalWrite`   |
| `GET`  | `/journals/{id}/versions`                      | `JournalWrite`   |
| `GET`  | `/journals/{id}/versions/{version_id}`         | `JournalRead`    |
| `POST` | `/journals/{id}/versions/{version_id}/approve` | `JournalApprove` |
| `POST` | `/journals/{id}/versions/{version_id}/publish` | `JournalPublish` |

Lifecycle: `draft -> approved -> published -> archived` with enforced transition rules.

## 5.2 Teaching resources (`/teaching-resources`)

| Method | Path                                              | Capability        |
| ------ | ------------------------------------------------- | ----------------- |
| `GET`  | `/teaching-resources?limit=&offset=`              | `ResourceRead`    |
| `GET`  | `/teaching-resources/{id}`                        | `ResourceRead`    |
| `POST` | `/teaching-resources`                             | `ResourceWrite`   |
| `PUT`  | `/teaching-resources/{id}`                        | `ResourceWrite`   |
| `GET`  | `/teaching-resources/{id}/versions`               | `ResourceWrite`   |
| `GET`  | `/teaching-resources/{id}/versions/{vid}`         | `ResourceRead`    |
| `POST` | `/teaching-resources/{id}/versions/{vid}/approve` | `ResourceApprove` |
| `POST` | `/teaching-resources/{id}/versions/{vid}/publish` | `ResourcePublish` |

## 5.3 Attachments (`/attachments`)

| Method   | Path                                   | Capability + parent gate             |
| -------- | -------------------------------------- | ------------------------------------ |
| `POST`   | `/attachments`                         | `AttachmentWrite` + parent writable  |
| `GET`    | `/attachments?parent_type=&parent_id=` | `AttachmentRead` + parent readable   |
| `GET`    | `/attachments/{id}`                    | `AttachmentRead` + parent readable   |
| `GET`    | `/attachments/{id}/preview`            | `AttachmentRead` + parent readable   |
| `DELETE` | `/attachments/{id}`                    | `AttachmentDelete` + parent writable |

Upload is `multipart/form-data` with: `file`, `parent_type`, `parent_id`, optional `category`.

Rules:

- 50 MiB size cap
- Upload MIME allowlist
- Preview MIME allowlist (strict subset)
- SHA-256 checksum stored and revalidated on preview

---

## 6. Academic Catalog APIs

## 6.1 Courses (`/courses`)

| Method   | Path                                     | Capability      |
| -------- | ---------------------------------------- | --------------- | --------------- |
| `GET`    | `/courses?department_id=&limit=&offset=` | `CourseRead`    |
| `POST`   | `/courses`                               | `CourseWrite`   |
| `GET`    | `/courses/{id}`                          | `CourseRead`    |
| `PUT`    | `/courses/{id}`                          | `CourseWrite`   |
| `GET`    | `/courses/{id}/versions`                 | `CourseWrite`   |
| `GET`    | `/courses/{id}/versions/{vid}`           | `CourseWrite`   |
| `POST`   | `/courses/{id}/versions/{vid}/approve`   | `CourseApprove` |
| `POST`   | `/courses/{id}/versions/{vid}/publish`   | `CoursePublish` |
| `GET`    | `/courses/{id}/prerequisites`            | `CourseRead`    |
| `POST`   | `/courses/{id}/prerequisites`            | `CourseWrite`   |
| `DELETE` | `/courses/{id}/prerequisites/{pid}`      | `CourseWrite`   |
| `GET`    | `/courses/template.csv`                  | Authenticated   |
| `GET`    | `/courses/template.xlsx`                 | Authenticated   |
| `GET`    | `/courses/export.csv`                    | `ExportCourses` |
| `GET`    | `/courses/export.xlsx`                   | `ExportCourses` |
| `POST`   | `/courses/import?mode=dry_run            | commit`         | `ImportCourses` |

Prerequisite insertion enforces no self-loop, no duplicate edge, and no cycles.

## 6.2 Sections (`/sections`)

| Method | Path                                                 | Capability       |
| ------ | ---------------------------------------------------- | ---------------- | ---------------- |
| `GET`  | `/sections?course_id=&department_id=&limit=&offset=` | `SectionRead`    |
| `POST` | `/sections`                                          | `SectionWrite`   |
| `GET`  | `/sections/{id}`                                     | `SectionRead`    |
| `PUT`  | `/sections/{id}`                                     | `SectionWrite`   |
| `GET`  | `/sections/{id}/versions`                            | `SectionWrite`   |
| `POST` | `/sections/{id}/versions/{vid}/approve`              | `SectionApprove` |
| `POST` | `/sections/{id}/versions/{vid}/publish`              | `SectionPublish` |
| `GET`  | `/sections/template.csv`                             | Authenticated    |
| `GET`  | `/sections/template.xlsx`                            | Authenticated    |
| `GET`  | `/sections/export.csv`                               | `ExportSections` |
| `GET`  | `/sections/export.xlsx`                              | `ExportSections` |
| `POST` | `/sections/import?mode=dry_run                       | commit`          | `ImportSections` |

Section uniqueness: `(course_id, section_code, term, year)`.

## 6.3 Import/Export Semantics

- Import supports CSV and XLSX
- Header matching is case-insensitive
- `dry_run`: validates and reports row errors with no business-table mutations
- `commit`: all-or-nothing transactional apply
- Non-admin/non-librarian export scope is department-constrained in SQL

---

## 7. Engagement and Analytics APIs

## 7.1 Check-ins (`/checkins`)

| Method | Path                                   | Capability     |
| ------ | -------------------------------------- | -------------- |
| `POST` | `/checkins`                            | `CheckinWrite` |
| `POST` | `/checkins/{id}/retry`                 | `CheckinWrite` |
| `GET`  | `/checkins?section_id=&limit=&offset=` | `CheckinRead`  |
| `GET`  | `/checkins/retry-reasons`              | Authenticated  |

Key behaviors:

- Duplicate suppression using configurable window (`checkin.duplicate_window_minutes`, default 10)
- Duplicates are stored as evidence (`is_duplicate_attempt=true`) and return `409`
- Retry requires controlled `reason_code` and max retry cap
- Local network rule is server-side CIDR based (`checkin.allowed_client_cidrs`)
- Response masking removes PII fields for unauthorized viewers

## 7.2 Metrics (`/metrics`)

| Method | Path                                   | Capability                        |
| ------ | -------------------------------------- | --------------------------------- |
| `GET`  | `/metrics?limit=&offset=`              | `MetricRead`                      |
| `POST` | `/metrics`                             | `MetricWrite`                     |
| `GET`  | `/metrics/{id}`                        | `MetricRead`                      |
| `PUT`  | `/metrics/{id}`                        | `MetricWrite` (new draft version) |
| `GET`  | `/metrics/{id}/versions`               | `MetricRead`                      |
| `POST` | `/metrics/{id}/versions/{vid}/approve` | `MetricWrite`                     |
| `POST` | `/metrics/{id}/versions/{vid}/publish` | `MetricApprove` (admin-only)      |
| `POST` | `/metrics/widgets/{widget_id}/verify`  | `MetricApprove` (admin-only)      |

Lineage references are validated against existing metric definition versions.

## 7.3 Dashboards (`/dashboards`)

All endpoints require `DashboardRead`:

- `/dashboards/course-popularity`
- `/dashboards/fill-rate`
- `/dashboards/drop-rate`
- `/dashboards/instructor-workload`
- `/dashboards/foot-traffic`
- `/dashboards/dwell-time`
- `/dashboards/interaction-quality`

Shared query options: `from`, `to`, `department_id`.

Rules:

- `from`/`to` are RFC3339
- Default window is last 30 days
- Max window is 366 days
- Non-admin/non-librarian callers are pinned to own department regardless of requested override

---

## 8. Reporting APIs

Base: `/reports`

### 8.1 Route Map

| Method   | Path                               | Capability      |
| -------- | ---------------------------------- | --------------- |
| `GET`    | `/reports`                         | `ReportRead`    |
| `POST`   | `/reports`                         | `ReportManage`  |
| `GET`    | `/reports/{id}`                    | `ReportRead`    |
| `PUT`    | `/reports/{id}`                    | `ReportManage`  |
| `POST`   | `/reports/{id}/run`                | `ReportExecute` |
| `GET`    | `/reports/{id}/runs`               | `ReportRead`    |
| `GET`    | `/reports/runs/{run_id}`           | `ReportRead`    |
| `GET`    | `/reports/runs/{run_id}/download`  | `ReportRead`    |
| `GET`    | `/reports/{id}/schedules`          | `ReportRead`    |
| `POST`   | `/reports/{id}/schedules`          | `ReportManage`  |
| `PUT`    | `/reports/schedules/{schedule_id}` | `ReportManage`  |
| `DELETE` | `/reports/schedules/{schedule_id}` | `ReportManage`  |

### 8.2 Report DTO Summary

- `ReportView`: id/title/description/query_definition/default_format/creator/timestamps
- `ReportRunView`: status, source (`manual|scheduled`), format (`csv|xlsx`), artifact availability, error details
- `ReportScheduleView`: cron expression, optional department scope, active flag, next/last run

Supported report query types include:

- `journal_catalog`
- `resource_catalog`
- `course_catalog`
- `checkin_activity`
- `audit_summary`
- `section_roster`

Trigger run returns `202 Accepted` with run metadata.

---

## 9. Retention and Artifact Backfill APIs

## 9.1 Retention (`/admin/retention`)

Requires `RetentionManage` (admin role in current matrix).

| Method | Path                            |
| ------ | ------------------------------- |
| `GET`  | `/admin/retention`              |
| `POST` | `/admin/retention`              |
| `GET`  | `/admin/retention/{id}`         |
| `PUT`  | `/admin/retention/{id}`         |
| `POST` | `/admin/retention/execute`      |
| `POST` | `/admin/retention/{id}/execute` |

Execute request body:

```json
{
  "dry_run": false,
  "strict_mode": false
}
```

Supported `target_entity_type` values:

- `audit_logs` (anonymize only)
- `sessions`
- `operational_events`
- `report_runs`

`strict_mode=true` can return `409 strict_mode_blocked` when unresolved legacy artifacts require backfill before cryptographic-erasure guarantees can be enforced.

## 9.2 Artifact backfill (`/admin/artifact-backfill`)

| Method | Path                       | Capability        |
| ------ | -------------------------- | ----------------- |
| `POST` | `/admin/artifact-backfill` | `RetentionManage` |

Request:

```json
{
  "dry_run": false,
  "batch_size": 100
}
```

- `batch_size` is clamped to `[1, 1000]`.
- Dry-run counts eligible rows without mutating files or DB rows.

---

## 10. Health Endpoint

| Method | Path      | Auth   |
| ------ | --------- | ------ |
| `GET`  | `/health` | Public |

Responses (HTTP 200 in both cases):

```json
{ "status": "ok", "database": "ok" }
```

```json
{
  "status": "degraded",
  "database": "error",
  "message": "database connectivity check failed"
}
```

---

## 11. Capability-Oriented Notes

- Route guards and capability checks are both enforced.
- Object-level visibility checks apply for department/ownership boundaries.
- Core lifecycle resources use version transitions with conflict checks on illegal state transitions.
- Sensitive fields are masked unless caller capability allows full visibility.

---

## 12. Implementation Source of Truth

This spec is derived from implemented modules and docs in `repo/`:

- `backend/src/lib.rs` (mount table)
- `backend/src/api/*.rs` (route contracts)
- `backend/src/application/*_service.rs` (input/output DTOs, validations)
- `docs/api_surface.md`, `docs/security_model.md`, `docs/domain_model.md`
