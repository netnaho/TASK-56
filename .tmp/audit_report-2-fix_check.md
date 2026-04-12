# Scholarly Static Re-Check: Prior Inspection Issues Fix Status (Round 5)

Scope: static code/configuration inspection only (no runtime execution).

## Overall

- **Fixed:** 3
- **Partially fixed:** 0
- **Not fixed:** 0

## Issue-by-issue status

### 1) HIGH — Department-scoped report usability gap for `JournalCatalog` / `ResourceCatalog`

**Previous finding:** department-scoped callers were hard-blocked (`403`) for these two report types.

**Status: ✅ Fixed (statically)**

**Evidence**

- Scope gate now allows scoped callers (`ScopeFilter::Department` / `DepartmentOrOwned`) and only denies unscopable contexts:
  - `repo/backend/src/application/report_service.rs:424-442`
- Journal catalog now scopes via creator department join (fail-closed):
  - `repo/backend/src/application/report_service.rs:1088-1147`
- Resource catalog now scopes via owner department join (fail-closed):
  - `repo/backend/src/application/report_service.rs:1149-1210`
- Dedicated regression script validates dept-scope and all-scope behavior:
  - `repo/API_tests/report_catalog_scope.sh:1-260`

**Conclusion**

- The prior functional hard-deny gap is resolved.

---

### 2) HIGH — Secure deletion guarantee previously best-effort on CoW/overlay filesystems

**Previous finding:** legacy null-DEK artifacts could still only be handled via best-effort overwrite/delete semantics.

**Status: ✅ Fixed (statically, with strict fail-closed enforcement path)**

**Evidence**

- Crypto-erase model already in place for keyed artifacts (`artifact_dek`):
  - `repo/backend/migrations/019_artifact_dek.sql:1-17`
  - `repo/backend/src/application/report_service.rs:1048-1066`
- Legacy classification/status model in DB:
  - `repo/backend/migrations/020_artifact_backfill_status.sql:1-48`
- Backfill endpoint + service now expose readiness and unresolved-actionable counts:
  - `repo/backend/src/api/artifact_backfill.rs:1-91`
  - `repo/backend/src/application/artifact_backfill.rs:76-112`
  - `repo/backend/src/application/artifact_backfill.rs:154-177`
  - `repo/backend/src/application/artifact_backfill.rs:327-352`
- Retention strict mode now blocks execution when actionable legacy artifacts exist (fail-closed):
  - `repo/backend/src/application/retention_service.rs:630-634`
  - `repo/backend/src/application/retention_service.rs:766-810`
  - `repo/backend/src/api/retention.rs:53-65`
- Error path is machine-readable and non-ambiguous (`strict_mode_blocked`, HTTP 409):
  - `repo/backend/src/errors/mod.rs:65-71`
  - `repo/backend/src/errors/mod.rs:104-106`
  - `repo/backend/src/errors/mod.rs:132`
  - `repo/backend/src/errors/mod.rs:145`
- Explicit strict-mode/backfill E2E script validates blocked→remediate→ready behavior:
  - `repo/API_tests/artifact_backfill_strict.sh:1-240`

**Conclusion**

- The prior compliance gap is now closed by a strict fail-closed execution mode plus measurable backfill readiness signals.
- Legacy best-effort path still exists only for **compat mode** (`strict_mode=false`) by design; strict mode provides enforceable secure-deletion semantics.

---

### 3) MEDIUM — DB-backed security tests skippable unless CI enforces strict mode

**Previous finding:** strict mode existed but CI enforcement was absent.

**Status: ✅ Fixed (statically)**

**Evidence**

- Main CI enforces strict integration run with DB URL + guard:
  - `repo/.github/workflows/ci.yml:1-206`
- Dedicated DB security gate workflow enforces DB-backed tests and fails on silent `[SKIP]` output:
  - `repo/.github/workflows/db-security.yml:1-179`

**Conclusion**

- CI silent-skip risk is addressed with defense-in-depth controls.

## Final re-check summary

- **All three previously reported issues are now statically fixed.**
- The remaining operational requirement for Issue #2 is procedural: use strict mode (`strict_mode=true`) once backfill indicates readiness.

## Notes

- This assessment is static-only and does not assert runtime execution outcomes.
- Security semantics for retention are now enforceable through explicit strict-mode gating and machine-readable blocked-state responses.
