//! Application services — orchestration layer between API and domain.
//!
//! New in Phase 2: the security backbone (`password`, `lockout`, `session`,
//! `principal`, `authorization`, `scope`, `masking`) and the real
//! implementations of `auth_service` and `audit_service`.
//!
//! New in Phase 6: `encryption` (AES-256-GCM field-level encryption),
//! `report_service` (scheduling, generation, artifacts),
//! and `retention_service` (policy enforcement, secure deletion).

// ── Phase 2: security backbone ─────────────────────────────────────────────
pub mod authorization;
pub mod lockout;
pub mod masking;
pub mod password;
pub mod principal;
pub mod scope;
pub mod session;

// ── Phase 6: field encryption and artifact crypto ────────────────────────
pub mod artifact_backfill;
pub mod artifact_crypto;
pub mod encryption;

// ── Real service implementations ──────────────────────────────────────────
pub mod attachment_service;
pub mod audit_service;
pub mod auth_service;
pub mod checkin_service;
pub mod course_service;
pub mod dashboard_service;
pub mod export_service;
pub mod import_service;
pub mod journal_service;
pub mod metric_service;
pub mod report_service;
pub mod resource_service;
pub mod retention_service;
pub mod section_service;

// ── Phase 1 stub (pending) ────────────────────────────────────────────────
pub mod user_service;
