# Business Logic Questions Log

This document records the key business-logic questions identified while interpreting the task prompt, plus the implementation-aligned hypothesis and resolution.

---

## 1) Role taxonomy mismatch (prompt vs implementation)

**Question:** The prompt defines five roles (System Administrator, Library Staff, Academic Scheduler, Instructor, Auditor), but the current implementation has `Admin`, `Librarian`, `DepartmentHead`, `Instructor`, `Auditor`, and `Viewer`. How should role mapping be handled?

**My Understanding / Hypothesis:**

- `System Administrator` maps to `Admin`
- `Library Staff` maps to `Librarian`
- `Academic Scheduler` maps to `DepartmentHead`
- `Instructor` maps directly to `Instructor`
- `Auditor` maps directly to `Auditor`
- `Viewer` is an additional read-mostly role not explicitly requested but useful and currently supported.

**Solution:** Keep the current 6-role RBAC model and document role mapping in product docs/API docs; continue enforcing capability-based checks and object-level scope filters.

---

## 2) Navigation vs data visibility boundaries

**Question:** “Users see only navigation and data permitted to them” does not define whether hiding nav alone is sufficient, or if API-level hard enforcement is required.

**My Understanding / Hypothesis:** Security must be enforced server-side regardless of UI hiding.

**Solution:** Current approach is correct: enforce at route guards + capability checks + object scope checks, and treat frontend navigation filtering as UX only.

---

## 3) Effective version selector behavior

**Question:** The prompt requires a visible “effective version” selector to review prior versions without overwriting current records. Is this read-only selection or does it change operational baseline?

**My Understanding / Hypothesis:** Selector should be read-only for review by default; operational change must be explicit through approve/publish actions.

**Solution:** Keep lifecycle model (`draft -> approved -> published -> archived`) and publish action as explicit baseline switch; do not overload selector into implicit state mutation.

---

## 4) Version retention duration

**Question:** Prompt says journal/course versions should retain at least 24 months. Does retention policy execution ever prune these versions?

**My Understanding / Hypothesis:** Version history for governed master data should not be pruned below compliance minimum; retention rules target operational/audit artifacts unless explicitly configured for version tables.

**Solution:** Preserve version tables by default and avoid auto-pruning them in current retention jobs; if pruning is later required, add dedicated policy with minimum floor checks.

---

## 5) Attachment checksum semantics

**Question:** Prompt requests checksum confirmation after upload to detect accidental changes. Should checksum be cryptographic and persisted?

**My Understanding / Hypothesis:** Yes—compute and persist file hash in DB metadata and return/show it in upload response/UI.

**Solution:** Keep current metadata strategy (path + hash in MySQL) and present checksum confirmation in attachment flow.

---

## 6) Attachment preview policy

**Question:** “Preview common formats” is ambiguous on which MIME types are acceptable and how unsupported types should behave.

**My Understanding / Hypothesis:** Use explicit MIME allowlist for preview and reject unsupported preview requests deterministically.

**Solution:** Continue whitelist-based preview behavior with unsupported preview returning `415`.

---

## 7) Bulk import commit semantics

**Question:** Prompt requires dry-run with row-level errors before commit, but does not specify partial success for commit mode.

**My Understanding / Hypothesis:** Commit must be all-or-nothing for operational consistency.

**Solution:** Keep current transactional model: dry-run writes nothing; commit aborts fully if any row fails validation.

---

## 8) CSV/XLSX template strictness

**Question:** Prompt mentions CSV/Excel templates but not whether headers are strict/case-sensitive.

**My Understanding / Hypothesis:** Header matching should be case-insensitive for usability; missing required columns must fail early.

**Solution:** Keep case-insensitive header detection and `422 validation` on missing required headers.

---

## 9) Check-in “one second” confirmation requirement

**Question:** Prompt says one-tap check-in confirms success within one second; is this a hard API SLA or UI responsiveness target?

**My Understanding / Hypothesis:** Treat as UX target in an offline/local deployment, not as strict contractual backend SLA.

**Solution:** Keep optimistic UI and fast local API path; document it as performance objective and validate through periodic latency checks in local environment.

---

## 10) Duplicate detection window precedence

**Question:** Prompt gives default duplicate window of 10 minutes. Is this fixed or admin-configurable?

**My Understanding / Hypothesis:** Default should be 10, but configurable by admin for institution policy variance.

**Solution:** Keep configurable admin setting for duplicate window and use it in duplicate suppression logic.

---

## 11) Retry behavior after duplicate warning

**Question:** Prompt allows a single retry with explicit reason selection; unclear whether retry reasons are free-text or controlled vocabulary.

**My Understanding / Hypothesis:** Use controlled reason codes to support analytics/audit consistency.

**Solution:** Keep single-retry cap and reason-code table; reject unknown codes with validation error.

---

## 12) Local network rule interpretation (SSID vs feasible web signals)

**Question:** Prompt example references approved SSID names, but browsers cannot reliably expose SSID in standard web contexts.

**My Understanding / Hypothesis:** SSID-based enforcement is not technically reliable in a Dioxus web app; network policy should be server-verifiable.

**Solution:** Use server-side CIDR allowlist enforcement and record browser-provided network hint only as non-authoritative audit metadata.

---

## 13) Metric lineage validation depth

**Question:** Prompt requires derived metrics to reference base metrics consistently; unclear whether validation is shallow or full dependency check.

**My Understanding / Hypothesis:** Validate lineage references at write time and flag dependent dashboards on publish-impacting changes.

**Solution:** Keep lineage reference validation on metric definition writes and auto-flag dependent widgets/charts for verification after publish.

---

## 14) Dashboard masking scope

**Question:** Prompt says sensitive instructor notes and student identifiers must not be shown to unauthorized viewers; unclear which roles are authorized.

**My Understanding / Hypothesis:** Authorization should be capability-based (`DashboardViewSensitive`) rather than hardcoded role names.

**Solution:** Keep API-layer masking for non-sensitive viewers and only return clear identifiers to roles with explicit sensitive-view capability.

---

## 15) Report schedule time interpretation

**Question:** Prompt example uses “every Monday at 7:00 AM” but does not define timezone source.

**My Understanding / Hypothesis:** In fully offline/local deployment, scheduler should use server local timezone unless tenant-level timezone config is added.

**Solution:** Keep current local scheduler with cron-based next-run computation; document timezone semantics as server-local.

---

## 16) Report scope for both lists and single-object downloads

**Question:** Prompt requires department-restricted exports, but does not explicitly state whether direct object access by ID must also be scoped.

**My Understanding / Hypothesis:** Scope must apply to list endpoints and single-object endpoints equally.

**Solution:** Keep hardening fix that enforces department visibility on `get_report`, `list_runs`, `get_run`, and artifact download paths.

---

## 17) Authentication token model ambiguity

**Question:** Prompt asks for offline auth with salted password hashing but does not require JWT specifically.

**My Understanding / Hypothesis:** Opaque bearer sessions are preferable offline when immediate revocation is required.

**Solution:** Keep opaque token model (random token on wire, hashed at rest in `sessions`) with logout revocation.

---

## 18) Lockout policy reset behavior

**Question:** Prompt defines lockout threshold/duration but not whether successful login resets failure counters.

**My Understanding / Hypothesis:** Successful authentication should clear failed-attempt counters.

**Solution:** Keep current lockout implementation where success resets the failed-attempt window.

---

## 19) “Immutable” audit logs vs retention/anonymization

**Question:** Prompt requires immutable audit logging with chained hashes, but also requires retention and secure deletion. Is anonymization/deletion allowed?

**My Understanding / Hypothesis:** Immutability applies during active retention window; post-retention actions are policy-governed lifecycle operations.

**Solution:** Keep append-only/tamper-detectable behavior in normal operation; retention executor applies configured anonymize/delete behavior on expiry with auditable runs.

---

## 20) Encryption-at-rest coverage boundaries

**Question:** Prompt says sensitive fields should be encrypted at rest “where appropriate” but does not enumerate required fields.

**My Understanding / Hypothesis:** Start with highest-risk narrative fields and expand iteratively with migration-safe rollout.

**Solution:** Keep current AES-256-GCM field encryption for sensitive section notes and response masking for sensitive identifiers; maintain extensible pattern for adding new encrypted columns.

---

## 21) Offline/air-gapped deployment interpretation

**Question:** Prompt requires fully offline operation; unclear whether optional external integrations are permitted.

**My Understanding / Hypothesis:** Core workflows must have zero dependency on internet services.

**Solution:** Keep architecture fully local (Rocket + MySQL + local disk storage + in-process scheduler), with no required external auth/queue/file services.

---

## 22) Academic scheduler permission granularity

**Question:** Prompt names “Academic Scheduler” role but does not define whether scope is institution-wide or department-scoped.

**My Understanding / Hypothesis:** Scheduler authority should be department-scoped by default in multi-department institutions.

**Solution:** Keep `DepartmentHead` behavior as scheduler-equivalent with department-bound scope checks, while `Admin` remains global override.
