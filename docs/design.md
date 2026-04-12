# Scholarly System Design

## 1. Purpose and Scope

`Scholarly` is an offline-first scholarly resources and teaching-operations platform designed to run fully on local infrastructure (localhost or local LAN), including:

- Library serial/journal lifecycle management
- Teaching resource management with attachment handling
- Academic catalog and section operations with prerequisite modeling
- Bulk import/export (CSV and XLSX) with dry-run validation
- In-person engagement check-ins with duplicate/retry controls
- Dashboards and a metric semantic layer
- Scheduled local report generation and downloadable artifacts
- Audit, retention, and encryption controls for compliance

This document describes the design **as implemented in the repository**.

---

## 2. Design Drivers

### 2.1 Core Requirements

- Fully offline/air-gapped operation
- Role-based UI and API access boundaries
- Immutable audit trail with tamper detection
- Versioned master data with explicit approve/publish lifecycle
- Department and ownership scope isolation
- Operational reliability with local-only dependencies

### 2.2 Constraints

- No dependency on external cloud services for core workflows
- Browser-only clients cannot reliably access Wi-Fi SSID (network controls must be server-verifiable)
- Compliance behaviors (retention, masking, encryption) must be enforceable in API layer, not only UI

---

## 3. High-Level Architecture

Three-tier deployment in Docker:

1. **Frontend**: Dioxus SPA (WASM), served by nginx on `:3000`
2. **Backend**: Rocket REST API on `:8000`
3. **Database**: MySQL 8 on `:3306`

nginx proxies `/api/*` to the backend. Frontend and backend communicate over local HTTP.

### 3.1 Backend Internal Architecture

The backend follows clean architecture boundaries:

- `api/` — Rocket handlers and request/response mapping
- `application/` — business use cases, authorization, orchestration
- `domain/` — pure entities, value objects, shared state machines
- `infrastructure/` — repositories, scheduler, file storage, bootstrap, DB pool
- `config/` — environment-backed app config
- `errors/` — canonical application errors and HTTP mapping

Design rule: route handlers call application services; they do not access repositories directly.

### 3.2 Frontend Architecture

- Route-driven Dioxus SPA (`dioxus-router`)
- Auth state persisted locally and propagated through context
- Thin API modules wrapping backend endpoints
- Page-level forms and workflow screens for each capability area

---

## 4. Security and Access Model

## 4.1 Authentication

- Offline username/password login
- Passwords hashed with **Argon2id** and random salt
- Minimum password length: **12**
- Account lockout: **5 failed attempts / 15 minutes**

The system uses **opaque bearer tokens** (not JWT). Tokens are random values returned once to the client and stored hashed (`SHA-256`) in `sessions`.

## 4.2 Authorization

Enforcement is layered:

1. **Route guards** (authenticated/admin guards)
2. **Capability checks** at service entry points
3. **Object-level scope checks** (department/ownership visibility)

Implemented roles include: `admin`, `librarian`, `department_head`, `instructor`, `auditor`, `viewer`.

Prompt role mapping used by product semantics:

- System Administrator → `admin`
- Library Staff → `librarian`
- Academic Scheduler → `department_head`
- Instructor → `instructor`
- Auditor → `auditor`

## 4.3 Data Protection Controls

- API response masking for sensitive fields where role lacks sensitive-view capability
- Field-level encryption (AES-256-GCM) for sensitive columns (currently section notes)
- Audit hash chain integrity verification endpoint

---

## 5. Core Domain Design

## 5.1 Identity and Organizational Scope

Primary entities: `departments`, `users`, `roles`, `permissions`, `user_roles`, `sessions`, `failed_login_attempts`.

Department is a key scope boundary used across exports, reports, and object visibility enforcement.

## 5.2 Versioned Content Pattern

The following domains use version histories:

- Journals (`journals`, `journal_versions`)
- Teaching resources (`teaching_resources`, `resource_versions`)
- Courses (`courses`, `course_versions`)
- Sections (`sections`, `section_versions`)
- Metrics (`metric_definitions`, `metric_definition_versions`)

Shared lifecycle: `draft -> approved -> published -> archived`.

The design maintains two pointers per root record:

- **current/published baseline pointer** for operational reads
- **latest/head pointer** for editor workflows

This supports reviewing prior/effective versions without accidental overwrite.

## 5.3 Academic Relationships

- `courses` belong to departments
- `sections` belong to courses and are assigned to instructors
- `course_prerequisites` uses adjacency-list edges
- Prerequisite write path enforces:
  - no self-loop
  - no duplicate edge
  - no cycle (DFS reachability check)

## 5.4 Attachments

- Attachments are stored on local disk
- Metadata (path, hash, mime, size) stored in MySQL
- Previewable MIME types are allowlisted
- Preview path includes checksum verification before serving bytes

---

## 6. Functional Workflows

## 6.1 Library Workflows

- Journal/resource creation and draft editing
- Approve/publish transitions
- Attachment upload, metadata persistence, and preview
- Unauthorized writes blocked at capability layer

## 6.2 Academic Operations

- Course and section create/read/update lifecycle with versioning
- Dry-run import validates rows and reports structured row-level errors
- Commit import is all-or-nothing transaction
- CSV/XLSX template endpoints return valid header templates
- Department-scoped exports enforced in SQL

## 6.3 Check-in Operations

- One-tap check-in creation path
- Duplicate suppression within configurable window (default 10 min)
- Duplicate attempts preserved as evidence records
- Single retry path requiring controlled reason code
- Device/browser fingerprint attributes captured for audit context
- Optional local-network enforcement implemented via server-side CIDR checks

## 6.4 Metrics and Dashboards

- Versioned metric definitions with lineage references
- Admin-gated publish operation
- Dependent dashboard widgets flagged for verification when definitions change
- Dashboard endpoints support date filtering and masking for non-authorized viewers

## 6.5 Reporting and Scheduling

- Report definitions, on-demand runs, and run artifacts
- Local scheduler polls due schedules periodically and generates artifacts
- Download endpoint serves stored report files
- Department scope enforced on list and single-object access paths

---

## 7. Audit, Retention, and Compliance

## 7.1 Audit Logging

- Immutable application-layer append model
- Captures authentication, permissions/config changes, content edits, imports/exports, report actions
- Search by actor/object/time with pagination
- CSV export endpoint for audit logs
- Hash chain (`previous_hash`, `current_hash`) for tamper detection

## 7.2 Retention and Deletion

Retention policies are configurable by target entity and action (e.g., anonymize/delete), with dry-run support before execution.

Configured defaults in current docs include long retention for audit events and shorter windows for operational artifacts/sessions.

Secure deletion flow attempts destructive cleanup for local artifacts before unlinking and removes expired DB rows as policy requires.

---

## 8. API Design Principles

- REST-style resource routes under `/api/v1`
- Standardized error envelope:
  - `error.code`
  - `error.message`
  - `error.request_id`
  - optional `error.fields` for validation details
- Capability-aware responses (including masking)
- Binary download responses for export/report artifacts
- Multipart support for attachment and import file workflows

---

## 9. Data and Storage Design

- MySQL as system of record for transactional and historical data
- Local filesystem for binary artifacts and attachments
- Hash metadata in DB to detect drift/tampering of files on disk
- Migrations and seeds are idempotent and applied at startup via container entrypoint

---

## 10. Operations and Deployment

- Standard local run path via `docker compose up`
- Health endpoint includes DB check status for liveness/degraded signaling
- Config driven via environment variables:
  - auth/session/lockout settings
  - attachment and report storage roots
  - encryption key material
  - logging level

No external scheduler, auth provider, or queue is required for baseline operation.

---

## 11. Quality and Verification Strategy

The repository uses layered validation:

- Backend unit tests (`cargo test --lib`)
- Backend integration tests
- Frontend tests
- Shell-based end-to-end API tests in `API_tests/`

`run_tests.sh` supports grouped execution and API-only/unit-only modes.

API tests cover key business areas including auth, RBAC, journals/resources, academic import/export, check-ins, metrics, dashboards, reports, retention, encryption, and audit-chain behavior.

---

## 12. Notable Design Decisions and Trade-offs

1. **Opaque tokens over JWT** for immediate revocation and simpler offline trust boundaries.
2. **Capability + object-scope enforcement** instead of UI-only role gating.
3. **Two-pointer version model** to separate published baseline from editable head.
4. **All-or-nothing import commit** to avoid partial catalog mutation.
5. **CIDR-based network enforcement** because SSID cannot be trusted in standard web clients.
6. **In-process scheduler** optimized for single-instance offline deployment (not multi-node distributed lock design).

---

## 13. Future Extension Points

- Expand field-level encryption coverage to additional sensitive columns
- Add explicit tenant/site timezone configuration for schedule semantics
- Add stronger retention policy constraints for version-history minimum windows
- Add richer dashboard verification workflows after metric definition changes
- Add optional multi-instance scheduler coordination if deployment model changes
