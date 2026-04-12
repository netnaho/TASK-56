# Delivery Acceptance & Project Architecture Static Audit (Round 3)

## 1. Verdict

**Overall conclusion: Partial Pass**

Most previously identified Blocker/High security defects are now statically addressed (session table alignment, report-format schema normalization, audit-summary privilege gate, object-level resource/attachment visibility, retry network-rule consistency). Remaining material gaps are primarily:

- a **functional High** on department-scoped report usability for journal/resource report types (hard deny instead of scoped exports), and
- a **security/compliance High** on secure deletion guarantees being explicitly best-effort only on CoW/overlay filesystems.

---

## 2. Scope and Static Verification Boundary

### What was reviewed

- Project docs and run/test instructions:
  - `repo/README.md:739-780`
  - `repo/API_tests/README.md:1-220`
  - `repo/run_tests.sh:1-132`
- Backend entrypoint and route registration:
  - `repo/backend/src/lib.rs:20-101`
- Auth guards, auth policy, lockout/error envelope:
  - `repo/backend/src/api/guards.rs:71-173`
  - `repo/backend/src/application/password.rs:19-35`
  - `repo/backend/src/application/lockout.rs:5-13`, `:45-53`
  - `repo/backend/src/errors/mod.rs:95-104`, `:121-131`
- Core service logic for reports/resources/attachments/check-in/retention/audit:
  - `repo/backend/src/application/report_service.rs:266-428`
  - `repo/backend/src/application/resource_service.rs:500-707`
  - `repo/backend/src/application/attachment_service.rs:239-267`, `:400-456`
  - `repo/backend/src/application/checkin_service.rs:23-31`, `:201-231`, `:400-446`
  - `repo/backend/src/application/retention_service.rs:640-706`, `:721-738`
  - `repo/backend/src/application/audit_service.rs:1-20`, `:153-244`, `:278-338`
- Schema/migration deltas:
  - `repo/backend/migrations/018_normalize_report_formats.sql:33-75`
- Static tests inventory and representative scripts:
  - `repo/backend/tests/api_routes_test.rs:14-25`, `:145`, `:257`, `:507`, `:759`, `:1006`, `:1131`, `:1239`, `:1390`, `:1640`
  - `repo/API_tests/checkin_network_blocked_retry.sh:1-173`
  - `repo/API_tests/report_scope_isolation.sh:1-220`
  - `repo/API_tests/checkin_duplicate_blocked.sh:1-79`
  - `repo/API_tests/library_attachment_upload_and_preview.sh:1-121`

### What was not reviewed

- Runtime behavior under live Rocket/MySQL/browser execution.
- Actual scheduler timing behavior under real wall-clock delays.
- Filesystem-specific secure-erasure guarantees on deployment target media.

### What was intentionally not executed

- No project startup.
- No Docker.
- No unit/API/integration test execution.

### Claims requiring manual verification

- One-second check-in success latency guarantee.
- Cron/scheduler execution timing correctness (e.g., Monday 07:00).
- Effective secure deletion behavior on the actual storage/filesystem used in deployment.
- Frontend visual polish/interaction fidelity in browser rendering.

---

## 3. Repository / Requirement Mapping Summary

### Prompt core goal / flows / constraints (condensed)

- Offline-first scholarly operations with strict role separation.
- Library + teaching resource lifecycle with versioning and effective version review.
- Attachments with preview and checksum confirmation.
- Course/section management with import/export dry-run and commit.
- Check-in with duplicate prevention, retry reason control, and optional local-network policy.
- Dashboards, local scheduled reports, audit immutability with tamper detection, retention and secure deletion, sensitive-data masking/encryption.

### Main implementation areas mapped

- API surfaces via Rocket mount tree: `repo/backend/src/lib.rs:68-100`.
- RBAC/auth guards: `repo/backend/src/api/guards.rs:71-173`.
- Domain service enforcement:
  - reports (`report_service`),
  - resource/attachment object scope,
  - check-in policy,
  - audit chain,
  - retention execution.
- Test surface from shell suite + DB-backed Rust integration tests.

---

## 4. Section-by-section Review

### 4.1 Hard Gates

#### 4.1.1 Documentation and static verifiability

- **Conclusion: Pass (with caveat)**
- **Rationale:** Documentation and test/run entry points are clear and internally consistent; strict integration mode is documented for CI. Caveat: CI wiring for strict mode is not present in repo and cannot be statically confirmed.
- **Evidence:** `repo/README.md:739-780`, `repo/API_tests/README.md:1-41`, `repo/run_tests.sh:9-20`, `:30-37`, `:95-106`
- **Manual verification note:** CI pipeline config must be inspected outside this repository snapshot.

#### 4.1.2 Material deviation from prompt

- **Conclusion: Partial Pass**
- **Rationale:** Core domains are aligned, but report department-scope behavior for journal/resource types is implemented as a hard deny for non-global scope rather than scoped data delivery, which is a material functional gap vs prompt expectation.
- **Evidence:** `repo/backend/src/application/report_service.rs:417-428`

### 4.2 Delivery Completeness

#### 4.2.1 Core explicit requirements coverage

- **Conclusion: Partial Pass**
- **Rationale:** Most explicit requirements are implemented (offline auth, lockout, versioning, checksums, retry policy, audit chain, format normalization), but two material gaps remain: scoped report usability for certain report types and secure deletion guarantee limitations.
- **Evidence:**
  - Lockout/password policy: `repo/backend/src/application/password.rs:19-35`, `repo/backend/src/application/lockout.rs:5-13`
  - Effective report formats normalized: `repo/backend/migrations/018_normalize_report_formats.sql:33-75`
  - Check-in retry/network unification: `repo/backend/src/application/checkin_service.rs:23-31`, `:400-446`
  - Report-scope hard deny for journal/resource: `repo/backend/src/application/report_service.rs:425-428`
  - Secure-delete caveat in code comments: `repo/backend/src/application/retention_service.rs:721-738`

#### 4.2.2 End-to-end deliverable vs partial/demo

- **Conclusion: Pass**
- **Rationale:** Repository is full-stack with backend/frontend/docs/migrations/tests and robust API script inventory.
- **Evidence:** `repo/backend/src/lib.rs:11-16`, `repo/API_tests/README.md:72-164`, `repo/README.md:739-802`

### 4.3 Engineering and Architecture Quality

#### 4.3.1 Module decomposition and structure

- **Conclusion: Pass**
- **Rationale:** Clear layered decomposition (api/application/domain/infrastructure) with route namespaces and service-level domain boundaries.
- **Evidence:** `repo/backend/src/lib.rs:11-16`, `:68-100`

#### 4.3.2 Maintainability and extensibility

- **Conclusion: Partial Pass**
- **Rationale:** Significant improvement from prior drift issues; however, report-scope behavior is constrained by missing department linkage on some report sources, causing hard-deny fallback rather than scalable scoped-query design.
- **Evidence:** `repo/backend/src/application/report_service.rs:417-428`, `:1022-1030`

### 4.4 Engineering Details and Professionalism

#### 4.4.1 Error handling, logging, validation, API design

- **Conclusion: Pass (with targeted caveat)**
- **Rationale:** Error envelope is coherent and non-leaky; meaningful validation and audit logging exist. Caveat remains around secure deletion guarantee semantics on certain filesystems.
- **Evidence:**
  - Error non-leak envelope: `repo/backend/src/errors/mod.rs:121-131`, `:156-171`
  - Input validation examples: `repo/backend/src/application/report_service.rs:294-296`, `repo/backend/src/application/checkin_service.rs:342-348`
  - Audit categories + chain: `repo/backend/src/application/audit_service.rs:38-85`, `:278-338`
  - Secure-delete limitation note: `repo/backend/src/application/retention_service.rs:721-738`

#### 4.4.2 Product-like organization vs demo

- **Conclusion: Pass**
- **Rationale:** The codebase is product-structured with mature route/service/test/docs organization, not a demo fragment.
- **Evidence:** `repo/backend/src/lib.rs:68-100`, `repo/API_tests/README.md:1-164`, `repo/README.md:739-802`

### 4.5 Prompt Understanding and Requirement Fit

#### 4.5.1 Business goal + constraints fit

- **Conclusion: Partial Pass**
- **Rationale:** Core security and workflow semantics are now largely aligned, including offline auth and retry behavior. Remaining mismatch: department-scoped report restriction for journal/resource types is implemented as deny rather than scoped export access.
- **Evidence:**
  - Scoped object checks for reports/resources: `repo/backend/src/application/report_service.rs:266-276`, `repo/backend/src/application/resource_service.rs:607-626`
  - Hard deny for dept-scoped journal/resource reports: `repo/backend/src/application/report_service.rs:425-428`

### 4.6 Aesthetics (frontend-only / full-stack)

#### 4.6.1 Visual and interaction quality

- **Conclusion: Cannot Confirm Statistically**
- **Rationale:** Static code indicates role-aware nav and effective-version UI sections, but visual quality/spacing/interaction state can only be validated in runtime rendering.
- **Evidence:**
  - Role-aware nav filtering: `repo/frontend/src/layouts/main_layout.rs:95-146`, `:216-224`
  - Effective-version section in course detail: `repo/frontend/src/pages/course_detail.rs:509-515`
- **Manual verification note:** browser-based UI review required.

---

## 5. Issues / Suggestions (Severity-Rated)

### 1) **High** — Department-scoped report usability gap for journal/resource report types

- **Conclusion:** Fail (functional requirement gap)
- **Evidence:** `repo/backend/src/application/report_service.rs:417-428`
- **Impact:** Department-scoped roles cannot receive scoped exports for these report types; they are blocked entirely (`403`). This is secure from leakage but does not fully satisfy business expectation of department-restricted exports.
- **Minimum actionable fix:** Add/derive department linkage for journal/resource data used by report queries (e.g., owner/department join path), then apply scoped filters instead of hard deny; keep deny as fallback for unresolved rows.

### 2) **High** — Secure deletion guarantee is explicitly best-effort only on some filesystems

- **Conclusion:** Partial Fail (compliance/security semantics gap)
- **Evidence:** `repo/backend/src/application/retention_service.rs:721-738`
- **Impact:** Requirement states secure deletion on expiry, but implementation acknowledges non-guaranteed cryptographic erasure on CoW/overlay filesystems; policy guarantee depends on storage backend characteristics.
- **Minimum actionable fix:** Enforce deployment storage class with guaranteed wipe semantics or integrate cryptographic erasure strategy (envelope keys per artifact with key destruction); document enforceable environment constraints in acceptance criteria.

### 3) **Medium** — Critical DB-backed security tests are still skippable unless CI explicitly enforces strict mode

- **Conclusion:** Partial Fail
- **Evidence:**
  - Skip behavior by design: `repo/backend/tests/api_routes_test.rs:14-25`, `:61`
  - Strict mode exists but optional: `repo/run_tests.sh:30-37`, `:95-106`
  - CI wiring not present in repo snapshot (`repo/.github/workflows/*` absent)
- **Impact:** Local/CI runs can still appear green without executing high-risk integration checks if strict mode is not wired.
- **Minimum actionable fix:** Add repository CI workflow invoking `./run_tests.sh --strict-integration` with `SCHOLARLY_TEST_DB_URL` and fail-fast policy.

---

## 6. Security Review Summary

### Authentication entry points

- **Conclusion: Pass**
- **Reasoning:** Bearer-token guard resolves active session and principal; password hashing + lockout policy implemented.
- **Evidence:** `repo/backend/src/api/guards.rs:100-137`, `repo/backend/src/application/password.rs:37-49`, `repo/backend/src/application/lockout.rs:25-53`

### Route-level authorization

- **Conclusion: Pass**
- **Reasoning:** Guard/capability checks are explicit on sensitive endpoints; admin-only guard enforced for chain verification.
- **Evidence:** `repo/backend/src/api/guards.rs:145-173`, `repo/backend/src/api/audit_logs.rs:54`, `:127`, `:193-199`

### Object-level authorization

- **Conclusion: Pass (improved from prior audit)**
- **Reasoning:** Resource reads now enforce owner/publish visibility; attachment read paths delegate to parent object visibility.
- **Evidence:** `repo/backend/src/application/resource_service.rs:607-626`, `:674-707`; `repo/backend/src/application/attachment_service.rs:239-267`, `:400-456`

### Function-level authorization

- **Conclusion: Pass**
- **Reasoning:** Sensitive report types now require additional capabilities (e.g., `AuditRead` for audit summary).
- **Evidence:** `repo/backend/src/application/report_service.rs:290-291`, `:413-414`, `:425-428`, `:758-759`

### Tenant / user data isolation

- **Conclusion: Partial Pass**
- **Reasoning:** Leakage risk is reduced by strict scoping/deny logic, but scoped usability for certain report types is unresolved (hard deny).
- **Evidence:** `repo/backend/src/application/report_service.rs:266-276`, `:417-428`; `repo/backend/src/application/resource_service.rs:528`, `:625`

### Admin / internal / debug endpoint protection

- **Conclusion: Pass**
- **Reasoning:** Admin-only route checks and capability gates are in place and covered by shell tests.
- **Evidence:** `repo/backend/src/api/audit_logs.rs:193-199`, `repo/API_tests/admin_endpoint_protection.sh:10-30`

---

## 7. Tests and Logging Review

### Unit tests

- **Conclusion: Pass (static presence)**
- **Rationale:** Unit tests exist for critical policies (password, lockout, navigation, etc.).
- **Evidence:** `repo/backend/src/application/password.rs:73-117`, `repo/backend/src/application/lockout.rs:99-140`, `repo/frontend/src/layouts/main_layout.rs` test module start near `:253`

### API / integration tests

- **Conclusion: Partial Pass**
- **Rationale:** Coverage breadth is strong and includes newly fixed risk areas; however DB-backed integration tests can still be skipped unless strict mode is enforced by CI.
- **Evidence:**
  - API suite inventory: `repo/API_tests/README.md:72-164`
  - New network-retry regression test: `repo/API_tests/checkin_network_blocked_retry.sh:1-173`
  - DB tests include prior gaps: `repo/backend/tests/api_routes_test.rs:759`, `:1006`, `:1131`, `:1239`, `:1390`, `:1640`
  - Skip gate: `repo/backend/tests/api_routes_test.rs:14-25`, `:61`

### Logging categories / observability

- **Conclusion: Pass**
- **Rationale:** Structured action taxonomy and audit chain verification provide meaningful traceability.
- **Evidence:** `repo/backend/src/application/audit_service.rs:38-85`, `:278-338`

### Sensitive-data leakage risk in logs / responses

- **Conclusion: Partial Pass**
- **Rationale:** Error responses avoid leaking internals and audit/search paths apply masking; residual risk is mostly operational (e.g., secure deletion guarantees).
- **Evidence:** `repo/backend/src/errors/mod.rs:121-131`, `repo/backend/src/api/audit_logs.rs:65-72`, `:145-152`

---

## 8. Test Coverage Assessment (Static Audit)

### 8.1 Test Overview

- **Unit tests exist:** Yes.
- **API/integration tests exist:** Yes (shell + Rust DB-backed).
- **Frameworks:** Rust `cargo test`, Rocket local async client, shell `curl` tests.
- **Test entry points:** `repo/run_tests.sh:46-132`, `repo/API_tests/README.md:24-41`.
- **DB test gating:** `SCHOLARLY_TEST_DB_URL` opt-in, with strict mode option.
  - Evidence: `repo/backend/tests/api_routes_test.rs:14-25`, `:61`; `repo/run_tests.sh:30-37`, `:95-106`

### 8.2 Coverage Mapping Table

| Requirement / Risk Point                                | Mapped Test Case(s)                                                                                                                                         | Key Assertion / Fixture / Mock                                                    | Coverage Assessment                  | Gap                                        | Minimum Test Addition                                               |
| ------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- | ------------------------------------ | ------------------------------------------ | ------------------------------------------------------------------- |
| Offline auth lockout (5 failures, 15-min policy intent) | `repo/API_tests/auth_lockout.sh:1-29`                                                                                                                       | First 5 failures => `401`, 6th => `429`                                           | basically covered                    | Window expiry not statically tested        | Add deterministic unlock-after-window test (time control)           |
| Unauthenticated / unauthorized route protection         | `repo/backend/tests/api_routes_test.rs:145`, `:194`; `repo/API_tests/admin_endpoint_protection.sh:10-30`                                                    | 401 on missing token; 403 for non-admin on admin routes                           | sufficient                           | DB suite optional unless strict mode       | Enforce strict mode in CI workflow                                  |
| Session revocation on deactivation                      | `repo/backend/tests/api_routes_test.rs:507`                                                                                                                 | Deactivation path test exists for session revocation                              | basically covered                    | Execution depends on DB env var            | Ensure CI always runs DB suite                                      |
| Report format CSV/XLSX lifecycle                        | `repo/backend/tests/api_routes_test.rs:759`                                                                                                                 | create/schedule/run/download xlsx path test exists                                | basically covered                    | Optional DB execution gate                 | CI enforcement of strict integration                                |
| Audit-summary privilege boundary                        | `repo/backend/tests/api_routes_test.rs:1006`                                                                                                                | Non-audit role denied for audit-summary report                                    | sufficient                           | Optional DB execution gate                 | CI strict mode                                                      |
| Dept-scoped roles blocked on unscoped report types      | `repo/backend/tests/api_routes_test.rs:1131`; `repo/API_tests/report_scope_isolation.sh:1-220`                                                              | 403 for out-of-scope single-object and schedule reads                             | sufficient (for current deny policy) | Does not validate true scoped availability | Add tests for scoped results once department linkage is implemented |
| Resource object-level draft isolation                   | `repo/backend/tests/api_routes_test.rs:1239`                                                                                                                | Instructor cannot read non-owned draft resource                                   | sufficient                           | DB-gated                                   | CI strict mode                                                      |
| Attachment visibility inherits parent object scope      | `repo/backend/tests/api_routes_test.rs:1390`                                                                                                                | Instructor cannot list attachments for non-visible resource                       | sufficient                           | DB-gated                                   | CI strict mode                                                      |
| Check-in duplicate + retry/network consistency          | `repo/API_tests/checkin_duplicate_blocked.sh:45-79`; `repo/API_tests/checkin_network_blocked_retry.sh:71-173`; `repo/backend/tests/api_routes_test.rs:1640` | duplicate => 409; retry blocked path => 403 not 200; retry-slot behavior asserted | sufficient                           | Runtime timing (≤1s) untested statically   | Add latency/perf assertion in controlled integration env            |
| Attachment checksum round-trip                          | `repo/API_tests/library_attachment_upload_and_preview.sh:31-102`                                                                                            | Validates stored sha256 + preview checksum header                                 | sufficient                           | Tamper simulation path still indirect      | Add explicit tamper-on-disk negative test in controlled fixture     |

### 8.3 Security Coverage Audit

- **Authentication:** basically covered (lockout + auth guard tests exist).
- **Route authorization:** sufficient coverage (admin-only and capability checks tested).
- **Object-level authorization:** significantly improved; now sufficiently targeted by DB tests (`resource` + `attachment`).
- **Tenant/data isolation:** partially covered; strong deny-path coverage exists, but true scoped export behavior for journal/resource remains unimplemented.
- **Admin/internal protection:** sufficiently covered by shell + DB integration tests.

### 8.4 Final Coverage Judgment

**Final coverage judgment: Partial Pass**

Coverage now includes most formerly high-risk regressions (session revocation, xlsx persistence path, audit-summary capability gate, resource/attachment object scope, retry network rule). However, severe defects could still pass in environments where DB tests are skipped, and one material functional gap (scoped report usability for journal/resource types) is not a test omission but an implementation gap.

---

## 9. Final Notes

- This assessment is strictly static and evidence-based; no runtime success claims are made.
- Prior Blocker/High security defects from earlier audits show clear static remediation evidence.
- Remaining material risks are concentrated in:
  1. report scoped-usability design for journal/resource report types, and
  2. secure deletion guarantee semantics + operational enforcement of DB-backed security tests.
