# Delivery Acceptance & Project Architecture Audit (Static-Only, Round 2)

## 1. Verdict

- **Overall conclusion: Partial Pass**

Static evidence shows broad end-to-end implementation aligned to the prompt (roles, offline auth, versioning, attachments/checksum, import/export dry-run, check-in controls, dashboards/masking, reports/schedules, audit-chain, retention, encryption).

However, one **material documentation inconsistency** remains (README sections still claim major features are not implemented), which weakens hard-gate static verifiability.

---

## 2. Scope and Static Verification Boundary

### What was reviewed

- Docs / setup / test entrypoints:
  - `repo/README.md:1`
  - `repo/run_tests.sh:1`
  - `repo/API_tests/README.md:1`
  - `repo/docs/api_surface.md:196`
- Backend architecture / route wiring:
  - `repo/backend/src/lib.rs:75-95`
- Security/authn/authz/scope/audit:
  - `repo/backend/src/api/guards.rs:76-166`
  - `repo/backend/src/application/authorization.rs:23-136`
  - `repo/backend/src/application/scope.rs:31-91`
  - `repo/backend/src/application/auth_service.rs:53-123`
  - `repo/backend/src/application/password.rs:19-55`
  - `repo/backend/src/application/lockout.rs:22-52`
  - `repo/backend/src/application/audit_service.rs:1-20, 152-250, 278-320`
  - `repo/backend/src/api/audit_logs.rs:25-27, 47, 112-128, 185`
- Core business modules (samples):
  - check-ins: `repo/backend/src/application/checkin_service.rs:165-307, 523-579, 743-817`
  - reports/scope: `repo/backend/src/application/report_service.rs:576-699`
  - attachments: `repo/backend/src/application/attachment_service.rs:1-30, 46-83`
  - dashboards/masking: `repo/backend/src/application/dashboard_service.rs:377-459`
  - retention: `repo/backend/src/application/retention_service.rs:1-15, 74-107`
  - encryption-at-rest: `repo/backend/src/application/encryption.rs:1-20, 72-99, 118-145`
- Frontend workflow evidence:
  - role/nav allowlist + tests: `repo/frontend/src/layouts/main_layout.rs:114-156, 228, 275-382`
  - attachment checksum UX: `repo/frontend/src/components/attachment_panel.rs:203-210, 333-351`
  - version/effective-view UX: `repo/frontend/src/pages/course_detail.rs:508-518`, `repo/frontend/src/pages/journal_detail.rs:378-406`
- Static tests coverage assets:
  - Shell API tests in `repo/API_tests/*.sh`
  - Rust integration tests: `repo/backend/tests/api_routes_test.rs:14-18, 143-415`

### What was not reviewed

- Runtime behavior under live DB/service/browser execution.
- Container/network orchestration behavior.
- Performance/SLA timing under load.

### What was intentionally not executed

- Project startup, Docker, backend/frontend runtime, all tests.

### Which claims require manual verification

- 1-second check-in confirmation SLA.
- Actual scheduler wall-clock execution (e.g., Monday 7:00 AM).
- Browser-level visual/interaction quality across environments.
- Secure deletion guarantees on concrete deployment filesystem/runtime.

---

## 3. Repository / Requirement Mapping Summary

- **Prompt core goal:** offline scholarly + teaching operations platform with strict role segregation, versioned records, local attachments + checksum, import/export dry-run, check-in controls, analytics dashboards, scheduled reports, immutable audit logs with tamper detection, retention, and encryption-at-rest.
- **Mapped implementation areas:** Rocket route domains mounted in one backend app (`repo/backend/src/lib.rs:75-95`), Dioxus frontend with explicit route access policy (`repo/frontend/src/layouts/main_layout.rs:114-156`), MySQL-backed services for auth/audit/reporting/retention/check-ins, and shell + Rust static test assets.
- **Constraint fit:** offline auth/lockout/password policy and local-file attachment model are implemented in code paths and config defaults (`repo/backend/src/application/password.rs:19`, `repo/backend/src/config/mod.rs:51`).

---

## 4. Section-by-section Review

### 1. Hard Gates

#### 1.1 Documentation and static verifiability

- **Conclusion: Partial Pass**
- **Rationale:** There is enough static evidence to review architecture and flows, but README contains contradictory stale sections that now conflict with implemented features.
- **Evidence:**
  - Setup/tests docs exist: `repo/README.md:242-245`, `repo/run_tests.sh:1-18`, `repo/API_tests/README.md:9-23`
  - Contradictory stale README section: `repo/README.md:663-674` (claims key features still missing)
  - Conflicting current implementation evidence: `repo/backend/src/application/principal.rs:21`, `repo/backend/src/api/audit_logs.rs:25-27,115`
- **Manual verification note:** N/A

#### 1.2 Material deviation from Prompt

- **Conclusion: Pass**
- **Rationale:** Current code aligns to the stated business scenario and technical constraints (roles incl. Auditor, local reporting/export, audit CSV export, scope controls, retention, encryption).
- **Evidence:**
  - Role model includes Auditor: `repo/backend/src/application/principal.rs:15-23`, `repo/backend/seeds/001_seed_roles_and_permissions.sql:17`
  - Audit CSV export implemented: `repo/backend/src/api/audit_logs.rs:25-27,112-181`
  - Lockout policy 5/15: `repo/backend/src/config/mod.rs:48-51`, `repo/backend/src/application/lockout.rs:22-52`

### 2. Delivery Completeness

#### 2.1 Core requirement coverage

- **Conclusion: Partial Pass**
- **Rationale:** Most explicit core requirements are implemented statically; runtime-timing requirements remain manual-verification only.
- **Evidence:**
  - Versioned content flows (frontend effective/version history): `repo/frontend/src/pages/course_detail.rs:508-518,535-543`, `repo/frontend/src/pages/journal_detail.rs:378-406`
  - Attachments + checksum confirmation: `repo/backend/src/application/attachment_service.rs:1-30`, `repo/frontend/src/components/attachment_panel.rs:203-210,333-351`
  - Import/export + dry-run endpoints: `repo/backend/src/api/courses.rs:286-352`, `repo/backend/src/api/sections.rs:240-305`
  - Check-in duplicate/retry/network rule: `repo/backend/src/application/checkin_service.rs:165-307,523-579`
  - Report schedules + scope: `repo/backend/src/application/report_service.rs:682-699`
- **Manual verification note:** 1-second response and actual scheduler timing cannot be proven statically.

#### 2.2 End-to-end 0→1 deliverable

- **Conclusion: Pass**
- **Rationale:** Full project structure with backend/frontend/docs/migrations/seeds/tests exists; not a fragment/demo.
- **Evidence:**
  - Mounted route surface: `repo/backend/src/lib.rs:75-95`
  - Multi-area API tests: `repo/API_tests/*.sh` (e.g., `repo/API_tests/report_scope_isolation.sh:1`, `repo/API_tests/audit_log_export.sh:1`)

### 3. Engineering and Architecture Quality

#### 3.1 Structure and decomposition

- **Conclusion: Pass**
- **Rationale:** Clear module decomposition (api/application/infrastructure/domain), typed service boundaries, explicit authz/scope helpers.
- **Evidence:**
  - Backend modules: `repo/backend/src/lib.rs:11-16`
  - Authz + scope isolation helpers: `repo/backend/src/application/authorization.rs:23-136`, `repo/backend/src/application/scope.rs:13-91`

#### 3.2 Maintainability/extensibility

- **Conclusion: Partial Pass**
- **Rationale:** Architecture is maintainable, but documentation drift introduces maintainability risk for reviewers/operators.
- **Evidence:**
  - Maintainable explicit nav allowlist + tests: `repo/frontend/src/layouts/main_layout.rs:114-156,275-382`
  - Documentation drift symptom: `repo/README.md:663-674` vs current code capabilities above.

### 4. Engineering Details and Professionalism

#### 4.1 Error handling/logging/validation/API quality

- **Conclusion: Pass**
- **Rationale:** Unified error envelope with correlation IDs, internal/db detail masking, structured logging, and validation across critical flows.
- **Evidence:**
  - Error envelope + no internal leak: `repo/backend/src/errors/mod.rs:13-20,119-134,153-186`
  - Catchers normalize framework-level errors: `repo/backend/src/api/catchers.rs:1-17,21-38`
  - Health degraded response sanitized + warned server-side: `repo/backend/src/api/health.rs:40,67-74`

#### 4.2 Product-like vs demo-like

- **Conclusion: Pass**
- **Rationale:** Breadth of domains, compliance features (audit chain, retention, encryption), and non-trivial workflow coverage indicate product-style implementation.
- **Evidence:**
  - Audit chain implementation: `repo/backend/src/application/audit_service.rs:1-20,278-320`
  - Retention policy service: `repo/backend/src/application/retention_service.rs:1-15,74-107`

### 5. Prompt Understanding and Requirement Fit

#### 5.1 Business goal and implicit constraints fit

- **Conclusion: Partial Pass**
- **Rationale:** Code fit is strong; primary remaining mismatch is documentation narrative lag (not code behavior).
- **Evidence:**
  - Offline-oriented auth + lockout + password policy: `repo/backend/src/application/auth_service.rs:53-123`, `repo/backend/src/application/password.rs:19-33`, `repo/backend/src/application/lockout.rs:22-52`
  - Prompt-compatible local network rule implemented via CIDR (explicitly documented browser SSID limitation): `repo/README.md:117-120`, `repo/backend/src/application/checkin_service.rs:523-579`

### 6. Aesthetics (frontend)

#### 6.1 Visual and interaction quality

- **Conclusion: Cannot Confirm Statistically**
- **Rationale:** Static code shows layout structure and feedback banners/buttons, but real visual quality/consistency requires runtime rendering.
- **Evidence:**
  - Save/error banners and workflow feedback: `repo/frontend/src/pages/course_detail.rs:501-507`, `repo/frontend/src/components/attachment_panel.rs:218-231`
  - Structured sections/components are present: `repo/frontend/src/pages/journal_detail.rs:378-447`
- **Manual verification note:** UI review in browser needed for spacing, typography, hover/click states, and overall visual coherence.

---

## 5. Issues / Suggestions (Severity-Rated)

### High

1. **Severity:** High  
   **Title:** README contains materially stale/contradictory implementation status claims  
   **Conclusion:** Fail  
   **Evidence:**
   - Stale “not implemented” block: `repo/README.md:663-674`
   - Contradicted by current code for Auditor role: `repo/backend/src/application/principal.rs:21`
   - Contradicted by current code for audit CSV export: `repo/backend/src/api/audit_logs.rs:25-27,115`
     **Impact:** Hard-gate static verifiability is degraded; reviewers may conclude core requirements are missing when they are implemented.  
     **Minimum actionable fix:** Replace or remove stale “What Is NOT Implemented Yet” section and synchronize README claims with current route/capability reality.

### Medium

2. **Severity:** Medium  
   **Title:** Rust integration tests are opt-in and can be fully skipped by default environment  
   **Conclusion:** Partial Fail  
   **Evidence:**
   - Skip gate behavior documented inline: `repo/backend/tests/api_routes_test.rs:14-18,61`
   - Tests print skip and return when env missing: e.g., `repo/backend/tests/api_routes_test.rs:147-148,196-197,259-260`
     **Impact:** Local/CI runs can appear green while critical integration authz paths are not exercised.  
     **Minimum actionable fix:** Add CI profile/job that always sets `SCHOLARLY_TEST_DB_URL` and reports skip count as a quality signal.

### Low

3. **Severity:** Low  
   **Title:** Internal module comment in `audit_logs.rs` is stale after export route addition  
   **Conclusion:** Improvement Needed  
   **Evidence:**
   - File header says “Both routes require `AuditRead`”: `repo/backend/src/api/audit_logs.rs:3-6`
   - Actual routes now include export + `AuditExport`: `repo/backend/src/api/audit_logs.rs:25-27,112-128`
     **Impact:** Maintainer confusion; not a runtime defect.  
     **Minimum actionable fix:** Update module-level comments to reflect three-route authorization split.

4. **Severity:** Low  
   **Title:** API tests README describes only single-script execution path  
   **Conclusion:** Improvement Needed  
   **Evidence:** `repo/API_tests/README.md:17,23` (only `health_check.sh` examples) vs broad suite in folder.  
   **Impact:** Reduced discoverability of complete API test surface for reviewers.  
   **Minimum actionable fix:** Add “run all scripts” guidance and reference `run_tests.sh` integration.

---

## 6. Security Review Summary

### authentication entry points

- **Conclusion:** Pass
- **Evidence:**
  - Auth guard enforces bearer token + active session lookup: `repo/backend/src/api/guards.rs:89-133`
  - Login flow enforces lockout before password verification: `repo/backend/src/application/auth_service.rs:53-79`
  - Password policy + Argon2 + min length 12: `repo/backend/src/application/password.rs:19-47`

### route-level authorization

- **Conclusion:** Pass
- **Evidence:**
  - `AdminOnly` guard: `repo/backend/src/api/guards.rs:143-166`
  - Capability checks at route/service boundaries (`AuditRead`, `AuditExport`): `repo/backend/src/api/audit_logs.rs:47,127`

### object-level authorization

- **Conclusion:** Pass
- **Evidence:**
  - Canonical helper: `repo/backend/src/application/scope.rs:54-91`
  - Report object/schedule scope enforcement: `repo/backend/src/application/report_service.rs:591,613,635,697`

### function-level authorization

- **Conclusion:** Pass
- **Evidence:**
  - `require(...)` pattern in services: `repo/backend/src/application/authorization.rs:220-226`
  - Examples: `check_in` capability gate `repo/backend/src/application/checkin_service.rs:173`, report schedule gate `repo/backend/src/application/report_service.rs:686`

### tenant / user data isolation

- **Conclusion:** Partial Pass
- **Evidence:**
  - Scope model supports department/owner constraints: `repo/backend/src/application/scope.rs:31-49`
  - Check-in visibility gate by section/course department and role: `repo/backend/src/application/checkin_service.rs:587-621`
- **Reasoning:** Strong evidence exists for reviewed high-risk domains (reports/check-ins/dashboards), but full per-endpoint tenant isolation for every domain was not exhaustively proven in this pass.

### admin / internal / debug protection

- **Conclusion:** Pass
- **Evidence:**
  - Admin-only chain verify route: `repo/backend/src/api/audit_logs.rs:185`
  - Admin config routes mounted under admin path and guarded in corresponding handlers/service design (`AdminOnly`/capability model): `repo/backend/src/lib.rs:92-93`, `repo/backend/src/api/guards.rs:143-166`

---

## 7. Tests and Logging Review

### Unit tests

- **Conclusion:** Pass
- **Rationale:** Rich unit coverage in core security and utility modules.
- **Evidence:**
  - Password tests: `repo/backend/src/application/password.rs:74-120`
  - Lockout tests: `repo/backend/src/application/lockout.rs:101-138`
  - Scope tests: `repo/backend/src/application/scope.rs:93-174`
  - Health sanitization tests: `repo/backend/src/api/health.rs:95-183`

### API / integration tests

- **Conclusion:** Partial Pass
- **Rationale:** Coverage breadth is strong, but rust integration tests are env-gated and can skip.
- **Evidence:**
  - Shell suite breadth: `repo/API_tests/auth_lockout.sh:1`, `repo/API_tests/report_scope_isolation.sh:1`, `repo/API_tests/audit_log_export.sh:1`
  - Rust integration tests exist: `repo/backend/tests/api_routes_test.rs:143-415`
  - Skip gate caveat: `repo/backend/tests/api_routes_test.rs:14-18,61`

### Logging categories / observability

- **Conclusion:** Pass
- **Evidence:**
  - Startup logging: `repo/backend/src/main.rs:6`
  - Structured warn/error logging in error/health paths: `repo/backend/src/errors/mod.rs:167-170`, `repo/backend/src/api/health.rs:67-70`

### Sensitive-data leakage risk in logs / responses

- **Conclusion:** Pass
- **Evidence:**
  - Public error masking for Internal/Database: `repo/backend/src/errors/mod.rs:129-134`
  - Health endpoint returns generic message, not raw DB detail: `repo/backend/src/api/health.rs:40,74`
  - Dashboard/check-in masking paths present: `repo/backend/src/application/dashboard_service.rs:434-437`, `repo/backend/src/application/checkin_service.rs:787-817`

---

## 8. Test Coverage Assessment (Static Audit)

### 8.1 Test Overview

- Unit tests exist in backend modules (`#[cfg(test)]`) and are invoked by `run_tests.sh` (`repo/run_tests.sh:30-47`).
- API/integration shell tests exist in `API_tests/` and are invoked by `run_tests.sh` (`repo/run_tests.sh:59-90`).
- Rust integration tests exist in `backend/tests/api_routes_test.rs` (`repo/backend/tests/api_routes_test.rs:143-415`).
- Documentation provides test command entrypoint (`repo/README.md:694-700`, `repo/run_tests.sh:1-10`).

### 8.2 Coverage Mapping Table

| Requirement / Risk Point                     | Mapped Test Case(s)                                                          | Key Assertion / Fixture / Mock                                  | Coverage Assessment | Gap                                                                   | Minimum Test Addition                                                                      |
| -------------------------------------------- | ---------------------------------------------------------------------------- | --------------------------------------------------------------- | ------------------- | --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| Auth lockout threshold                       | `repo/API_tests/auth_lockout.sh:1-29`                                        | 6th attempt expects `429` (`line 28`)                           | basically covered   | Duration window expiry (15 min) not statically asserted in shell test | Add time-window expiry assertion (or deterministic DB fixture-based unit/integration test) |
| Admin-only endpoint protection               | `repo/API_tests/admin_endpoint_protection.sh:1-31`                           | Instructor `403`, admin `200`                                   | sufficient          | None obvious                                                          | Add one more admin route sample (`/admin/retention`)                                       |
| Report object-scope isolation incl schedules | `repo/API_tests/report_scope_isolation.sh:1-165`                             | out-of-scope depthead gets `403` on report/run/runs/schedules   | sufficient          | Runtime race around run completion timing remains non-deterministic   | Add deterministic fixture for completed run artifact                                       |
| Audit CSV export authz + format              | `repo/API_tests/audit_log_export.sh:1-127`                                   | admin 200 + CSV header, librarian 403, unauth 401, bad UUID 422 | sufficient          | None major                                                            | Add explicit masking assertion for non-admin if policy expands                             |
| Attachment checksum round-trip + preview     | `repo/API_tests/library_attachment_upload_and_preview.sh:1-118`              | sha256 matches local hash + preview checksum header             | sufficient          | No corrupted-binary negative case                                     | Add corrupted preview/content tamper test                                                  |
| Unauthenticated 401 on sensitive endpoints   | Rust integration test `db_unauthenticated_request_is_rejected_with_401`      | direct status assertions for missing/malformed auth             | sufficient          | Env-gated skip possible                                               | Ensure CI job sets `SCHOLARLY_TEST_DB_URL`                                                 |
| AdminOnly guard behavior                     | Rust integration test `db_non_admin_is_forbidden_on_admin_only_endpoint`     | librarian 403 vs admin 200 on verify-chain                      | sufficient          | Env-gated skip possible                                               | Same CI gating as above                                                                    |
| Schedules scope security regression          | Rust integration test `db_report_schedule_listing_enforces_department_scope` | depthead 403 on out-of-scope schedules; owner 200               | sufficient          | Env-gated skip possible                                               | Same CI gating as above                                                                    |
| Audit export capability enforcement          | Rust integration test `db_audit_export_denied_to_non_exporting_roles`        | non-export roles forbidden                                      | sufficient          | Env-gated skip possible                                               | Same CI gating as above                                                                    |

### 8.3 Security Coverage Audit

- **authentication:** basically covered (login/lockout scripts + auth integration tests), but lockout duration expiry is not deeply asserted.
- **route authorization:** sufficient (admin endpoint protection scripts + integration tests).
- **object-level authorization:** sufficient for report scope paths (including schedules) based on dedicated regression tests.
- **tenant / data isolation:** basically covered in reports/check-ins/dashboard paths; not exhaustively proven for all domains in this static pass.
- **admin / internal protection:** sufficient for audited endpoints (`/admin/config`, `/audit-logs/verify-chain`, export capability gate).

### 8.4 Final Coverage Judgment

- **Final Coverage Judgment: Partial Pass**

Boundary:

- Major auth/authz/scope and core workflow risks are covered by meaningful shell and Rust integration tests.
- Remaining confidence gap is primarily execution gating (`SCHOLARLY_TEST_DB_URL` skip path) plus some non-deterministic/time-window concerns (e.g., lockout expiry timing).

---

## 9. Final Notes

- This audit is static-only and does not claim runtime success.
- Core implementation appears materially aligned with the prompt.
- Primary acceptance drag is documentation reliability (README stale contradictory sections), not core backend/frontend feature absence.
