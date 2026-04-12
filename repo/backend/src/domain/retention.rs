//! Retention policy domain models.
//!
//! Each `RetentionPolicy` targets a named entity type and specifies how
//! long records are kept and what action is taken when the window expires.
//!
//! # Entity types supported by Phase 6
//!
//! | target_entity_type    | Table(s)           | Notes                              |
//! |-----------------------|--------------------|------------------------------------|
//! | `audit_logs`          | audit_logs         | Anonymise only; chain preserved    |
//! | `sessions`            | sessions           | Hard delete                        |
//! | `operational_events`  | checkin_events     | Hard delete                        |
//! | `report_runs`         | report_runs        | Hard delete + artifact file        |
//!
//! See docs/phase_6_summary.md for the secure-deletion definitions and limitations.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// The action to take when a retention policy's time window expires.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionAction {
    /// Permanently delete the row and any associated files.
    Delete,
    /// Replace identifying fields with anonymised placeholders while keeping
    /// the row for statistical purposes.
    Anonymize,
    /// Flag the row for manual review before any destructive action.
    FlagForReview,
}

impl RetentionAction {
    pub fn as_db(self) -> &'static str {
        match self {
            RetentionAction::Delete => "delete",
            RetentionAction::Anonymize => "anonymize",
            RetentionAction::FlagForReview => "flag_for_review",
        }
    }

    pub fn from_db(s: &str) -> Option<Self> {
        match s {
            "delete" => Some(RetentionAction::Delete),
            "anonymize" => Some(RetentionAction::Anonymize),
            // Legacy values
            "archive" => Some(RetentionAction::FlagForReview),
            "flag_for_review" => Some(RetentionAction::FlagForReview),
            _ => None,
        }
    }
}

/// A data-retention policy governing how long a category of records is kept.
#[derive(Debug, Clone, Serialize)]
pub struct RetentionPolicy {
    pub id: Uuid,
    /// Entity type identifier, e.g. "audit_logs", "operational_events".
    pub target_entity_type: String,
    /// Number of days after `created_at` before the action fires.
    pub retention_days: i32,
    /// What to do when the retention window expires.
    pub action: RetentionAction,
    /// Legal or business rationale for this policy.
    pub rationale: Option<String>,
    /// Whether this policy is actively enforced.
    pub is_active: bool,
    pub created_by: Option<Uuid>,
    pub last_executed_at: Option<NaiveDateTime>,
    pub last_execution_result: Option<serde_json::Value>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// Summary returned after a retention execution run.
#[derive(Debug, Clone, Serialize)]
pub struct RetentionExecutionResult {
    pub policy_id: Uuid,
    pub target_entity_type: String,
    pub action: RetentionAction,
    pub cutoff_date: NaiveDateTime,
    /// Number of rows affected (deleted or anonymised).
    pub rows_affected: u64,
    /// Number of artifact files deleted (for report_runs policy).
    pub files_deleted: u64,
    /// Whether this was a dry run (counts only, no mutations).
    pub dry_run: bool,
    pub executed_at: NaiveDateTime,
    // ── report_runs artifact deletion breakdown (Phase 6 hardened) ────────
    /// Artifacts deleted via cryptographic erasure (DEK nulled; guaranteed
    /// irrecoverable).  Zero for non-report-runs policies.
    pub crypto_erased: u64,
    /// Artifacts deleted via legacy best-effort overwrite + unlink (not
    /// guaranteed on OverlayFS).  Should be zero after a full backfill run.
    pub legacy_fallback: u64,
    /// Artifact rows skipped because the file was absent from disk
    /// (`backfill_status = 'missing_file'`).  DB row is still deleted.
    pub missing_file: u64,
    // ── strict-mode counters (Phase 6 final closure) ──────────────────────
    /// Legacy artifacts with `artifact_dek IS NULL` and `backfill_status IS NULL`
    /// (never attempted).  In strict mode these block execution.
    pub legacy_unbackfilled: u64,
    /// Legacy artifacts with `artifact_dek IS NULL` and `backfill_status =
    /// 'encrypt_failed'`.  Retryable via backfill.  Block in strict mode.
    pub legacy_encrypt_failed: u64,
    /// Artifacts that would have been processed via legacy fallback but were
    /// blocked by strict mode.  Equals `legacy_unbackfilled + legacy_encrypt_failed`
    /// when the gate fires; otherwise 0.
    pub blocked_due_to_strict_mode: u64,
    /// Whether strict mode was requested for this execution.
    pub strict_mode: bool,
}

/// Summary across all policies in a single execution pass.
#[derive(Debug, Clone, Serialize)]
pub struct RetentionExecutionSummary {
    pub policies_run: u32,
    pub policies_skipped: u32,
    pub total_rows_affected: u64,
    pub total_files_deleted: u64,
    pub dry_run: bool,
    pub results: Vec<RetentionExecutionResult>,
    pub executed_at: NaiveDateTime,
    /// Across all report_runs policies: artifacts crypto-erased.
    pub total_crypto_erased: u64,
    /// Across all report_runs policies: artifacts using legacy best-effort path.
    pub total_legacy_fallback: u64,
    /// Across all report_runs policies: artifacts with missing files.
    pub total_missing_file: u64,
    // ── strict-mode summary (Phase 6 final closure) ───────────────────────
    /// Across all policies: never-attempted legacy artifacts.
    pub total_legacy_unbackfilled: u64,
    /// Across all policies: encrypt_failed legacy artifacts.
    pub total_legacy_encrypt_failed: u64,
    /// Across all policies: artifacts blocked by strict mode gate.
    pub total_blocked_due_to_strict_mode: u64,
    /// Whether strict mode was requested.
    pub strict_mode: bool,
    /// `true` when `total_blocked_due_to_strict_mode == 0` and strict mode was
    /// enabled — confirms all expired artifacts were handled via crypto-erase.
    pub strict_retention_ready: bool,
}
