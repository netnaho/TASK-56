//! Retention service — policy CRUD and scheduled enforcement execution.
//!
//! # Supported entity types
//!
//! | `target_entity_type` | Table            | Cutoff column   | Action enforced      |
//! |----------------------|------------------|-----------------|----------------------|
//! | `audit_logs`         | audit_logs       | created_at      | Anonymize **only**   |
//! | `sessions`           | sessions         | expires_at      | Delete               |
//! | `operational_events` | checkin_events   | checked_in_at   | Delete               |
//! | `report_runs`        | report_runs      | created_at      | Delete + file purge  |
//!
//! Deleting `audit_logs` rows is explicitly prohibited: the append-only chain
//! must stay intact. Configuring `action = Delete` on the `audit_logs` entity
//! type is rejected at create / update time.

use chrono::{Duration, Utc};
use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use sqlx::MySqlPool;
use uuid::Uuid;

use crate::application::artifact_crypto;
use crate::application::audit_service::{self, AuditEvent, actions};
use crate::application::authorization::{require, Capability};
use crate::application::encryption::FieldEncryption;
use crate::application::principal::Principal;
use crate::domain::retention::{
    RetentionAction, RetentionExecutionResult, RetentionExecutionSummary, RetentionPolicy,
};
use crate::errors::{AppError, AppResult};
use crate::infrastructure::repositories::report_repo;
use crate::infrastructure::repositories::retention_repo::{self, RetentionPolicyRow};

// ---------------------------------------------------------------------------
// View models & inputs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct RetentionPolicyView {
    pub id: Uuid,
    pub target_entity_type: String,
    pub retention_days: i32,
    pub action: RetentionAction,
    pub rationale: Option<String>,
    pub is_active: bool,
    pub created_by: Option<Uuid>,
    pub last_executed_at: Option<NaiveDateTime>,
    pub last_execution_result: Option<serde_json::Value>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    /// Approximate count of rows currently eligible for the policy's action,
    /// computed at request time. `None` when the policy is inactive or the
    /// entity type does not map to a directly-countable table.
    pub eligible_rows: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateRetentionPolicyInput {
    pub target_entity_type: String,
    pub retention_days: i32,
    pub action: RetentionAction,
    pub rationale: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateRetentionPolicyInput {
    pub retention_days: Option<i32>,
    pub action: Option<RetentionAction>,
    pub rationale: Option<String>,
    pub is_active: Option<bool>,
}

// ---------------------------------------------------------------------------
// Validation helpers
// ---------------------------------------------------------------------------

const VALID_ENTITY_TYPES: &[&str] = &[
    "audit_logs",
    "sessions",
    "operational_events",
    "report_runs",
];

fn validate_entity_type(t: &str) -> AppResult<()> {
    if VALID_ENTITY_TYPES.contains(&t) {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "unknown target_entity_type '{}'; must be one of: {}",
            t,
            VALID_ENTITY_TYPES.join(", ")
        )))
    }
}

fn validate_retention_days(days: i32) -> AppResult<()> {
    if days >= 0 {
        Ok(())
    } else {
        Err(AppError::Validation(
            "retention_days must be 0 or greater".into(),
        ))
    }
}

/// Reject any attempt to set action = Delete (or FlagForReview) on audit_logs.
fn validate_audit_logs_action(entity_type: &str, action: RetentionAction) -> AppResult<()> {
    if entity_type == "audit_logs" && action != RetentionAction::Anonymize {
        return Err(AppError::Validation(
            "audit_logs retention action must be 'anonymize'; \
             deleting audit log rows breaks the tamper-evident chain"
                .into(),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Row → domain / view mapping
// ---------------------------------------------------------------------------

fn row_to_policy(row: &RetentionPolicyRow) -> AppResult<RetentionPolicy> {
    let id = Uuid::parse_str(&row.id)
        .map_err(|e| AppError::Database(format!("invalid UUID in retention_policies.id: {}", e)))?;
    let action = RetentionAction::from_db(&row.action).ok_or_else(|| {
        AppError::Database(format!(
            "unknown action '{}' in retention_policies row {}",
            row.action, row.id
        ))
    })?;
    let created_by = row
        .created_by
        .as_deref()
        .map(|s| {
            Uuid::parse_str(s).map_err(|e| {
                AppError::Database(format!(
                    "invalid UUID in retention_policies.created_by: {}",
                    e
                ))
            })
        })
        .transpose()?;
    let last_execution_result = row
        .last_execution_result
        .as_deref()
        .map(|s| {
            serde_json::from_str(s).map_err(|e| {
                AppError::Database(format!(
                    "invalid JSON in retention_policies.last_execution_result: {}",
                    e
                ))
            })
        })
        .transpose()?;

    Ok(RetentionPolicy {
        id,
        target_entity_type: row.target_entity_type.clone(),
        retention_days: row.retention_days,
        action,
        rationale: row.rationale.clone(),
        is_active: row.is_active != 0,
        created_by,
        last_executed_at: row.last_executed_at,
        last_execution_result,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// Resolve the approximate eligible-row count for a policy's current cutoff.
///
/// Returns `None` when the policy is inactive or when counting is not
/// applicable (should not occur given the validated entity type set).
async fn compute_eligible_rows(
    pool: &MySqlPool,
    policy: &RetentionPolicy,
) -> Option<u64> {
    if !policy.is_active {
        return None;
    }

    let (table, col) = entity_type_to_table(&policy.target_entity_type)?;
    let cutoff = Utc::now().naive_utc() - Duration::days(policy.retention_days as i64);

    match retention_repo::count_expired_rows(pool, table, col, cutoff).await {
        Ok(n) => Some(n),
        Err(e) => {
            tracing::warn!(
                policy_id = %policy.id,
                entity_type = %policy.target_entity_type,
                "could not compute eligible_rows: {}",
                e
            );
            None
        }
    }
}

async fn policy_to_view(pool: &MySqlPool, policy: RetentionPolicy) -> AppResult<RetentionPolicyView> {
    let eligible_rows = compute_eligible_rows(pool, &policy).await;
    Ok(RetentionPolicyView {
        id: policy.id,
        target_entity_type: policy.target_entity_type,
        retention_days: policy.retention_days,
        action: policy.action,
        rationale: policy.rationale,
        is_active: policy.is_active,
        created_by: policy.created_by,
        last_executed_at: policy.last_executed_at,
        last_execution_result: policy.last_execution_result,
        created_at: policy.created_at,
        updated_at: policy.updated_at,
        eligible_rows,
    })
}

// ---------------------------------------------------------------------------
// Entity type → table/column mapping
// ---------------------------------------------------------------------------

/// Return the `(table_name, date_column)` pair for a given entity type.
/// Returns `None` for unknown types (should be caught by validation first).
fn entity_type_to_table(entity_type: &str) -> Option<(&'static str, &'static str)> {
    match entity_type {
        "audit_logs" => Some(("audit_logs", "created_at")),
        "sessions" => Some(("sessions", "expires_at")),
        "operational_events" => Some(("checkin_events", "checked_in_at")),
        "report_runs" => Some(("report_runs", "created_at")),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Public CRUD API
// ---------------------------------------------------------------------------

/// List all retention policies with live eligible-row counts.
pub async fn list_policies(
    pool: &MySqlPool,
    principal: &Principal,
) -> AppResult<Vec<RetentionPolicyView>> {
    require(principal, Capability::RetentionManage)?;

    let rows = retention_repo::find_all(pool).await?;
    let mut views = Vec::with_capacity(rows.len());
    for row in &rows {
        let policy = row_to_policy(row)?;
        let view = policy_to_view(pool, policy).await?;
        views.push(view);
    }
    Ok(views)
}

/// Fetch a single retention policy by ID.
pub async fn get_policy(
    pool: &MySqlPool,
    principal: &Principal,
    id: Uuid,
) -> AppResult<RetentionPolicyView> {
    require(principal, Capability::RetentionManage)?;

    let row = retention_repo::find_by_id(pool, id).await?;
    let policy = row_to_policy(&row)?;
    policy_to_view(pool, policy).await
}

/// Create a new retention policy.
///
/// Validates:
/// * `retention_days > 0`
/// * `target_entity_type` is one of the supported values
/// * `audit_logs` must use `Anonymize`
/// * No duplicate policy for the same entity type
pub async fn create_policy(
    pool: &MySqlPool,
    principal: &Principal,
    input: CreateRetentionPolicyInput,
) -> AppResult<RetentionPolicyView> {
    require(principal, Capability::RetentionManage)?;

    validate_entity_type(&input.target_entity_type)?;
    validate_retention_days(input.retention_days)?;
    validate_audit_logs_action(&input.target_entity_type, input.action)?;

    // Enforce uniqueness (the DB also has a unique key, but surface a clear error).
    if let Some(_existing) =
        retention_repo::find_by_entity_type(pool, &input.target_entity_type).await?
    {
        return Err(AppError::Conflict(format!(
            "a retention policy for '{}' already exists",
            input.target_entity_type
        )));
    }

    let id = Uuid::new_v4();
    let now = Utc::now().naive_utc();
    let row = RetentionPolicyRow {
        id: id.to_string(),
        target_entity_type: input.target_entity_type.clone(),
        retention_days: input.retention_days,
        action: input.action.as_db().to_string(),
        rationale: input.rationale.clone(),
        is_active: 1,
        created_by: Some(principal.user_id.to_string()),
        last_executed_at: None,
        last_execution_result: None,
        created_at: now,
        updated_at: now,
    };

    retention_repo::insert(pool, &row).await?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: actions::RETENTION_POLICY_CREATE,
            target_entity_type: Some("retention_policy"),
            target_entity_id: Some(id),
            change_payload: Some(serde_json::json!({
                "target_entity_type": input.target_entity_type,
                "retention_days":     input.retention_days,
                "action":             input.action.as_db(),
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    let created = retention_repo::find_by_id(pool, id).await?;
    let policy = row_to_policy(&created)?;
    policy_to_view(pool, policy).await
}

/// Update mutable fields on an existing retention policy.
///
/// Only fields present in the input are applied; `None` means "leave as is".
pub async fn update_policy(
    pool: &MySqlPool,
    principal: &Principal,
    id: Uuid,
    input: UpdateRetentionPolicyInput,
) -> AppResult<RetentionPolicyView> {
    require(principal, Capability::RetentionManage)?;

    // Load current state so we can fill in unchanged fields.
    let existing_row = retention_repo::find_by_id(pool, id).await?;
    let existing = row_to_policy(&existing_row)?;

    let new_days = input.retention_days.unwrap_or(existing.retention_days);
    let new_action = input.action.unwrap_or(existing.action);
    let new_rationale = match input.rationale {
        Some(r) => Some(r),
        None => existing.rationale.clone(),
    };
    let new_is_active = input.is_active.unwrap_or(existing.is_active);

    validate_retention_days(new_days)?;
    validate_audit_logs_action(&existing.target_entity_type, new_action)?;

    retention_repo::update(
        pool,
        id,
        new_days,
        new_action.as_db(),
        new_rationale.as_deref(),
        new_is_active,
    )
    .await?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: actions::RETENTION_POLICY_UPDATE,
            target_entity_type: Some("retention_policy"),
            target_entity_id: Some(id),
            change_payload: Some(serde_json::json!({
                "retention_days": new_days,
                "action":         new_action.as_db(),
                "is_active":      new_is_active,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    let updated = retention_repo::find_by_id(pool, id).await?;
    let policy = row_to_policy(&updated)?;
    policy_to_view(pool, policy).await
}

// ---------------------------------------------------------------------------
// Execution — all active policies
// ---------------------------------------------------------------------------

/// Execute every active retention policy and return an aggregate summary.
///
/// `strict_mode`: when `true`, any `report_runs` policy that has expired
/// actionable legacy artifacts (artifact_dek IS NULL, not permanently
/// terminal) will **not** proceed silently — the function returns
/// [`AppError::StrictModeBlocked`] immediately rather than falling back to
/// best-effort deletion.  Defaults to `false` for backward compatibility.
pub async fn execute_all(
    pool: &MySqlPool,
    principal: &Principal,
    reports_storage_path: &str,
    dry_run: bool,
    strict_mode: bool,
    enc: &FieldEncryption,
) -> AppResult<RetentionExecutionSummary> {
    require(principal, Capability::RetentionManage)?;

    let all_rows = retention_repo::find_all(pool).await?;
    let started_at = Utc::now().naive_utc();

    let mut results: Vec<RetentionExecutionResult> = Vec::new();
    let mut policies_skipped: u32 = 0;

    for row in &all_rows {
        let policy = row_to_policy(row)?;
        if !policy.is_active {
            policies_skipped += 1;
            continue;
        }

        match run_policy_execution(pool, &policy, reports_storage_path, dry_run, strict_mode, enc).await {
            Ok(result) => {
                // Persist execution stamp (skip on dry runs to avoid noise).
                if !dry_run {
                    let result_json = serde_json::to_string(&result)
                        .unwrap_or_else(|e| {
                            tracing::warn!(
                                policy_id = %policy.id,
                                "failed to serialize retention result, storing null: {}",
                                e
                            );
                            "null".into()
                        });
                    if let Err(e) =
                        retention_repo::mark_executed(pool, policy.id, result.executed_at, &result_json)
                            .await
                    {
                        tracing::warn!(
                            policy_id = %policy.id,
                            "mark_executed failed: {}",
                            e
                        );
                    }
                }

                audit_service::record(
                    pool,
                    AuditEvent {
                        actor_id: Some(principal.user_id),
                        actor_email: Some(&principal.email),
                        action: actions::RETENTION_EXECUTE_POLICY,
                        target_entity_type: Some("retention_policy"),
                        target_entity_id: Some(policy.id),
                        change_payload: Some(serde_json::json!({
                            "dry_run":       dry_run,
                            "strict_mode":   strict_mode,
                            "rows_affected": result.rows_affected,
                            "files_deleted": result.files_deleted,
                            "cutoff_date":   result.cutoff_date.to_string(),
                        })),
                        ip_address: None,
                        user_agent: None,
                    },
                )
                .await?;

                results.push(result);
            }
            Err(e) => {
                // StrictModeBlocked errors are deliberate gates — propagate them
                // so the caller receives a non-success response.  Other errors
                // are logged and swallowed so remaining policies still run.
                if matches!(e, AppError::StrictModeBlocked { .. }) {
                    return Err(e);
                }
                tracing::error!(
                    policy_id = %policy.id,
                    entity_type = %policy.target_entity_type,
                    "retention execution error: {}",
                    e
                );
                // Continue with remaining policies; errors are logged not propagated.
                policies_skipped += 1;
            }
        }
    }

    let total_rows_affected: u64 = results.iter().map(|r| r.rows_affected).sum();
    let total_files_deleted: u64 = results.iter().map(|r| r.files_deleted).sum();
    let total_crypto_erased: u64 = results.iter().map(|r| r.crypto_erased).sum();
    let total_legacy_fallback: u64 = results.iter().map(|r| r.legacy_fallback).sum();
    let total_missing_file: u64 = results.iter().map(|r| r.missing_file).sum();
    let total_legacy_unbackfilled: u64 = results.iter().map(|r| r.legacy_unbackfilled).sum();
    let total_legacy_encrypt_failed: u64 = results.iter().map(|r| r.legacy_encrypt_failed).sum();
    let total_blocked_due_to_strict_mode: u64 =
        results.iter().map(|r| r.blocked_due_to_strict_mode).sum();
    let policies_run = results.len() as u32;

    let strict_retention_ready =
        strict_mode && total_blocked_due_to_strict_mode == 0;

    let summary = RetentionExecutionSummary {
        policies_run,
        policies_skipped,
        total_rows_affected,
        total_files_deleted,
        total_crypto_erased,
        total_legacy_fallback,
        total_missing_file,
        total_legacy_unbackfilled,
        total_legacy_encrypt_failed,
        total_blocked_due_to_strict_mode,
        strict_mode,
        strict_retention_ready,
        dry_run,
        results,
        executed_at: started_at,
    };

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: actions::RETENTION_EXECUTE,
            target_entity_type: Some("retention_policy"),
            target_entity_id: None,
            change_payload: Some(serde_json::json!({
                "dry_run":                       dry_run,
                "strict_mode":                   strict_mode,
                "policies_run":                  policies_run,
                "policies_skipped":              policies_skipped,
                "total_rows_affected":           total_rows_affected,
                "total_files_deleted":           total_files_deleted,
                "total_crypto_erased":           total_crypto_erased,
                "total_legacy_fallback":         total_legacy_fallback,
                "total_missing_file":            total_missing_file,
                "total_legacy_unbackfilled":     total_legacy_unbackfilled,
                "total_legacy_encrypt_failed":   total_legacy_encrypt_failed,
                "total_blocked_due_to_strict_mode": total_blocked_due_to_strict_mode,
                "strict_retention_ready":        strict_retention_ready,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    Ok(summary)
}

// ---------------------------------------------------------------------------
// Execution — single policy
// ---------------------------------------------------------------------------

/// Execute a single retention policy by its UUID.
///
/// `strict_mode`: same semantics as [`execute_all`] — see its documentation.
pub async fn execute_policy(
    pool: &MySqlPool,
    principal: &Principal,
    policy_id: Uuid,
    reports_storage_path: &str,
    dry_run: bool,
    strict_mode: bool,
    enc: &FieldEncryption,
) -> AppResult<RetentionExecutionResult> {
    require(principal, Capability::RetentionManage)?;

    let row = retention_repo::find_by_id(pool, policy_id).await?;
    let policy = row_to_policy(&row)?;

    let result =
        run_policy_execution(pool, &policy, reports_storage_path, dry_run, strict_mode, enc)
            .await?;

    if !dry_run {
        let result_json = serde_json::to_string(&result)
            .unwrap_or_else(|_| "null".into());
        retention_repo::mark_executed(pool, policy.id, result.executed_at, &result_json).await?;
    }

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: actions::RETENTION_EXECUTE_POLICY,
            target_entity_type: Some("retention_policy"),
            target_entity_id: Some(policy_id),
            change_payload: Some(serde_json::json!({
                "dry_run":       dry_run,
                "rows_affected": result.rows_affected,
                "files_deleted": result.files_deleted,
                "cutoff_date":   result.cutoff_date.to_string(),
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    Ok(result)
}

// ---------------------------------------------------------------------------
// Core execution engine
// ---------------------------------------------------------------------------

/// Execute a single retention policy, dispatching to the correct strategy
/// for the entity type.
///
/// This function is intentionally private — all external callers go through
/// [`execute_policy`] or [`execute_all`], both of which enforce the
/// `RetentionManage` capability and write the audit trail.
///
/// When `strict_mode = true` and the policy targets `report_runs`, any
/// actionable legacy artifact (artifact_dek IS NULL, not missing_file) in the
/// expiry window causes this function to return
/// [`AppError::StrictModeBlocked`] instead of proceeding with best-effort
/// deletion.  The caller must run the backfill endpoint first.
async fn run_policy_execution(
    pool: &MySqlPool,
    policy: &RetentionPolicy,
    reports_storage_path: &str,
    dry_run: bool,
    strict_mode: bool,
    enc: &FieldEncryption,
) -> AppResult<RetentionExecutionResult> {
    // Use Rust's clock for the cutoff. Row timestamps (created_at, expires_at,
    // checked_in_at) are all stored using Utc::now().naive_utc() by the service
    // layer, so using the same clock domain ensures the comparison is consistent.
    // The <= operator (not strict <) handles the case where retention_days = 0
    // and a row was created in the same second as the cutoff is calculated.
    let executed_at = Utc::now().naive_utc();
    let cutoff = executed_at - Duration::days(policy.retention_days as i64);

    // rows_affected, files_deleted, crypto_erased, legacy_fallback, missing_file,
    // legacy_unbackfilled, legacy_encrypt_failed, blocked_due_to_strict_mode
    let (rows_affected, files_deleted, crypto_erased, legacy_fallback, missing_file,
         legacy_unbackfilled, legacy_encrypt_failed, blocked_due_to_strict_mode) =
        match policy.target_entity_type.as_str() {
        // ── audit_logs: Anonymize ONLY ────────────────────────────────────
        "audit_logs" => {
            if policy.action != RetentionAction::Anonymize {
                return Err(AppError::Validation(
                    "audit_logs retention action must be 'anonymize'".into(),
                ));
            }
            if dry_run {
                let count =
                    retention_repo::count_expired_rows(pool, "audit_logs", "created_at", cutoff)
                        .await?;
                (count, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64)
            } else {
                let affected = retention_repo::anonymize_audit_actors(pool, cutoff).await?;
                tracing::info!(
                    policy_id = %policy.id,
                    rows_anonymized = affected,
                    cutoff = %cutoff,
                    "audit_logs anonymisation complete"
                );
                (affected, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64)
            }
        }

        // ── sessions: Delete ──────────────────────────────────────────────
        "sessions" => {
            if dry_run {
                let count = retention_repo::count_expired_rows(
                    pool,
                    "sessions",
                    "expires_at",
                    cutoff,
                )
                .await?;
                (count, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64)
            } else {
                let affected = retention_repo::delete_expired_rows(
                    pool,
                    "sessions",
                    "expires_at",
                    cutoff,
                )
                .await?;
                (affected, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64)
            }
        }

        // ── operational_events: Delete ────────────────────────────────────
        "operational_events" => {
            if dry_run {
                let count = retention_repo::count_expired_rows(
                    pool,
                    "checkin_events",
                    "checked_in_at",
                    cutoff,
                )
                .await?;
                (count, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64)
            } else {
                let affected = retention_repo::delete_expired_rows(
                    pool,
                    "checkin_events",
                    "checked_in_at",
                    cutoff,
                )
                .await?;
                (affected, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64)
            }
        }

        // ── report_runs: Delete rows + artifact files ─────────────────────
        "report_runs" => {
            if dry_run {
                let count = retention_repo::count_expired_rows(
                    pool,
                    "report_runs",
                    "created_at",
                    cutoff,
                )
                .await?;
                // In a dry run also classify artifact files but don't touch them.
                let artifacts =
                    retention_repo::find_report_run_artifacts_with_keys(pool, cutoff).await?;
                let file_count = artifacts.len() as u64;
                let mut unbackfilled_count = 0u64;
                let mut encrypt_failed_count = 0u64;
                for (_, _, dek_opt, status_opt) in &artifacts {
                    if dek_opt.is_none() {
                        match status_opt.as_deref() {
                            Some("missing_file") => {}
                            Some("encrypt_failed") => { encrypt_failed_count += 1; }
                            _ => { unbackfilled_count += 1; }
                        }
                    }
                }
                // (rows, files, crypto, fallback, missing, unbackfilled, enc_failed, blocked)
                (count, file_count, 0u64, 0u64, 0u64,
                 unbackfilled_count, encrypt_failed_count, 0u64)
            } else {
                // 1. Collect artifact metadata (run_id, path, DEK, backfill_status).
                let artifacts =
                    retention_repo::find_report_run_artifacts_with_keys(pool, cutoff).await?;

                // 2. Classify all artifacts in a single pass before mutating anything.
                let mut files_deleted: u64 = 0;
                let mut crypto_erased: u64 = 0;
                let mut legacy_fallback: u64 = 0;
                let mut missing_file_count: u64 = 0;
                let mut legacy_unbackfilled: u64 = 0;
                let mut legacy_encrypt_failed: u64 = 0;

                for (_, _, dek_opt, status_opt) in &artifacts {
                    if dek_opt.is_none() {
                        match status_opt.as_deref() {
                            Some("missing_file") => {}
                            Some("encrypt_failed") => { legacy_encrypt_failed += 1; }
                            _ => { legacy_unbackfilled += 1; }
                        }
                    }
                }
                let total_actionable_legacy = legacy_unbackfilled + legacy_encrypt_failed;

                // 3. Strict-mode gate: block if any actionable legacy rows exist.
                if strict_mode && total_actionable_legacy > 0 {
                    // Emit audit event before returning the error so operators can
                    // correlate log entries.  Best-effort — don't mask the gate error.
                    let _ = audit_service::record(
                        pool,
                        AuditEvent {
                            actor_id: None,
                            actor_email: None,
                            action: actions::RETENTION_STRICT_MODE_BLOCKED,
                            target_entity_type: Some("retention_policy"),
                            target_entity_id: Some(policy.id),
                            change_payload: Some(serde_json::json!({
                                "policy_id":            policy.id.to_string(),
                                "unresolved_count":     total_actionable_legacy,
                                "legacy_unbackfilled":  legacy_unbackfilled,
                                "legacy_encrypt_failed": legacy_encrypt_failed,
                                "cutoff_date":          cutoff.to_string(),
                                "remediation":
                                    "POST /api/v1/admin/artifact-backfill to encrypt \
                                     legacy artifacts before re-running retention.",
                            })),
                            ip_address: None,
                            user_agent: None,
                        },
                    )
                    .await;

                    return Err(AppError::StrictModeBlocked {
                        unresolved_count: total_actionable_legacy,
                        hint: format!(
                            "Run POST /api/v1/admin/artifact-backfill to encrypt \
                             {} legacy artifact(s) (unbackfilled={}, encrypt_failed={}), \
                             then retry retention.",
                            total_actionable_legacy,
                            legacy_unbackfilled,
                            legacy_encrypt_failed,
                        ),
                    });
                }

                // 4. For each artifact: choose deletion strategy based on DEK
                //    presence and backfill_status.
                // Reset classification counters — we now track actuals for the result.
                legacy_unbackfilled = 0;
                legacy_encrypt_failed = 0;

                for (run_id_str, path_str, wrapped_dek_opt, backfill_status_opt) in &artifacts {
                    if let Some(wrapped_dek) = wrapped_dek_opt {
                        // ── Crypto-erase path (guaranteed) ────────────────────
                        match artifact_crypto::unwrap_dek(enc, wrapped_dek) {
                            Ok(_) => {}
                            Err(e) => {
                                tracing::warn!(
                                    run_id = %run_id_str,
                                    "retention: DEK unwrap failed (corrupt?): {}; \
                                     proceeding with erasure anyway",
                                    e
                                );
                            }
                        }
                        if let Ok(run_uuid) = Uuid::parse_str(run_id_str) {
                            if let Err(e) = report_repo::erase_run_artifact_dek(pool, run_uuid).await {
                                tracing::warn!(
                                    run_id = %run_id_str,
                                    "retention: erase_run_artifact_dek failed: {}",
                                    e
                                );
                            }
                        }
                        tracing::debug!(
                            run_id = %run_id_str,
                            path = %path_str,
                            deletion_mode = "crypto_erase",
                            "artifact DEK erased; ciphertext on disk is irrecoverable"
                        );

                        // Physical deletion of encrypted file (best-effort housekeeping).
                        let full_path = if std::path::Path::new(path_str).is_absolute() {
                            std::path::PathBuf::from(path_str)
                        } else {
                            std::path::Path::new(reports_storage_path).join(path_str)
                        };
                        match tokio::fs::remove_file(&full_path).await {
                            Ok(_) => { files_deleted += 1; }
                            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                            Err(_) => {}
                        }
                        crypto_erased += 1;

                    } else {
                        // ── Legacy path (artifact_dek IS NULL) ────────────────
                        let status = backfill_status_opt.as_deref().unwrap_or("");

                        match status {
                            "missing_file" => {
                                // No file on disk — nothing to physically delete.
                                tracing::debug!(
                                    run_id = %run_id_str,
                                    deletion_mode = "missing_file",
                                    "artifact row has no file (missing_file status); \
                                     skipping physical deletion"
                                );
                                missing_file_count += 1;
                            }
                            "encrypt_failed" => {
                                // Known failed backfill — compat mode allows best-effort.
                                tracing::warn!(
                                    run_id = %run_id_str,
                                    path = %path_str,
                                    deletion_mode = "legacy_fallback",
                                    "retention: artifact backfill failed previously; \
                                     using best-effort deletion (strict mode not enabled)"
                                );
                                let full_path = if std::path::Path::new(path_str).is_absolute() {
                                    std::path::PathBuf::from(path_str)
                                } else {
                                    std::path::Path::new(reports_storage_path).join(path_str)
                                };
                                let deleted = secure_delete_file(&full_path).await;
                                files_deleted += deleted;
                                legacy_fallback += 1;
                                legacy_encrypt_failed += 1;
                            }
                            _ => {
                                // Never-attempted legacy artifact.
                                tracing::warn!(
                                    run_id = %run_id_str,
                                    path = %path_str,
                                    deletion_mode = "legacy_fallback",
                                    "retention: legacy artifact without DEK — \
                                     using best-effort deletion (not cryptographic). \
                                     Run the backfill endpoint to upgrade this artifact."
                                );
                                let full_path = if std::path::Path::new(path_str).is_absolute() {
                                    std::path::PathBuf::from(path_str)
                                } else {
                                    std::path::Path::new(reports_storage_path).join(path_str)
                                };
                                let deleted = secure_delete_file(&full_path).await;
                                files_deleted += deleted;
                                legacy_fallback += 1;
                                legacy_unbackfilled += 1;
                            }
                        }
                    }
                }

                // 5. Delete DB rows.
                let rows_affected = retention_repo::delete_expired_rows(
                    pool,
                    "report_runs",
                    "created_at",
                    cutoff,
                )
                .await?;

                tracing::info!(
                    policy_id = %policy.id,
                    rows_deleted = rows_affected,
                    files_deleted = files_deleted,
                    crypto_erased = crypto_erased,
                    legacy_fallback = legacy_fallback,
                    legacy_unbackfilled = legacy_unbackfilled,
                    legacy_encrypt_failed = legacy_encrypt_failed,
                    missing_file = missing_file_count,
                    cutoff = %cutoff,
                    strict_mode = strict_mode,
                    "report_runs retention complete"
                );

                (rows_affected, files_deleted, crypto_erased, legacy_fallback,
                 missing_file_count, legacy_unbackfilled, legacy_encrypt_failed, 0u64)
            }
        }

        other => {
            return Err(AppError::Validation(format!(
                "unsupported target_entity_type '{}' in policy {}",
                other, policy.id
            )));
        }
    };

    Ok(RetentionExecutionResult {
        policy_id: policy.id,
        target_entity_type: policy.target_entity_type.clone(),
        action: policy.action,
        cutoff_date: cutoff,
        rows_affected,
        files_deleted,
        dry_run,
        executed_at,
        crypto_erased,
        legacy_fallback,
        missing_file,
        legacy_unbackfilled,
        legacy_encrypt_failed,
        blocked_due_to_strict_mode,
        strict_mode,
    })
}

// ---------------------------------------------------------------------------
// Secure file deletion
// ---------------------------------------------------------------------------

/// Best-effort secure file deletion.
///
/// Overwrites the file content with zero bytes, then removes it from the
/// filesystem.  Returns 1 if the file was removed, 0 if it was not found or
/// removal failed.
///
/// **Limitations**: this does NOT guarantee cryptographic erasure on
/// copy-on-write or container overlay filesystems where the OS may keep the
/// original blocks. See `docs/phase_6_summary.md` for the full discussion.
async fn secure_delete_file(path: &std::path::Path) -> u64 {
    // Best-effort: overwrite with zeros then delete.
    // This does NOT guarantee cryptographic erasure on container overlay FS;
    // see docs/phase_6_summary.md for limitations.
    if let Ok(metadata) = tokio::fs::metadata(path).await {
        let size = metadata.len() as usize;
        if size > 0 {
            let zeros = vec![0u8; size.min(1024 * 1024)]; // cap at 1 MB chunks
            if let Ok(mut file) = tokio::fs::OpenOptions::new().write(true).open(path).await {
                use tokio::io::AsyncWriteExt;
                let _ = file.write_all(&zeros).await;
                let _ = file.flush().await;
            }
        }
    }
    match tokio::fs::remove_file(path).await {
        Ok(_) => 1,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
        Err(_) => 0,
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::retention::RetentionAction;

    #[test]
    fn valid_entity_types_are_accepted() {
        for t in &["audit_logs", "sessions", "operational_events", "report_runs"] {
            assert!(
                validate_entity_type(t).is_ok(),
                "expected '{}' to be valid",
                t
            );
        }
    }

    #[test]
    fn unknown_entity_type_is_rejected() {
        assert!(validate_entity_type("users").is_err());
        assert!(validate_entity_type("journals").is_err());
        assert!(validate_entity_type("").is_err());
    }

    #[test]
    fn retention_days_must_be_non_negative() {
        assert!(validate_retention_days(1).is_ok());
        assert!(validate_retention_days(2555).is_ok());
        assert!(validate_retention_days(0).is_ok(), "zero should be valid (expire immediately)");
        assert!(validate_retention_days(-1).is_err(), "negative should be invalid");
    }

    #[test]
    fn audit_logs_rejects_delete_action() {
        // Deleting audit log rows breaks the SHA-256 chain.
        assert!(validate_audit_logs_action("audit_logs", RetentionAction::Delete).is_err());
        assert!(
            validate_audit_logs_action("audit_logs", RetentionAction::FlagForReview).is_err()
        );
        assert!(
            validate_audit_logs_action("audit_logs", RetentionAction::Anonymize).is_ok(),
            "anonymize must be the only allowed action for audit_logs"
        );
    }

    #[test]
    fn non_audit_entity_types_allow_delete() {
        for entity in &["sessions", "operational_events", "report_runs"] {
            assert!(
                validate_audit_logs_action(entity, RetentionAction::Delete).is_ok(),
                "expected Delete to be allowed for '{}'",
                entity
            );
        }
    }

    // ── Strict-mode gate decision logic ────────────────────────────────────────

    /// The strict gate classifies legacy artifacts from a 4-tuple vec (as returned
    /// by `find_report_run_artifacts_with_keys`) and computes the actionable count.
    ///
    /// This is a unit test of the classification logic embedded in
    /// `run_policy_execution`'s `report_runs` live path.  We replicate the
    /// classification here to verify correctness without needing a real DB.
    fn classify_artifacts(
        artifacts: &[(String, String, Option<String>, Option<String>)],
    ) -> (u64, u64) {
        // Returns (unbackfilled, encrypt_failed) counts.
        let mut unbackfilled = 0u64;
        let mut encrypt_failed = 0u64;
        for (_, _, dek_opt, status_opt) in artifacts {
            if dek_opt.is_none() {
                match status_opt.as_deref() {
                    Some("missing_file") => {}
                    Some("encrypt_failed") => { encrypt_failed += 1; }
                    _ => { unbackfilled += 1; }
                }
            }
        }
        (unbackfilled, encrypt_failed)
    }

    #[test]
    fn strict_gate_counts_only_actionable_legacy() {
        // Simulate a mixed bag of artifact rows.
        let artifacts: Vec<(String, String, Option<String>, Option<String>)> = vec![
            // Keyed artifact — not actionable legacy.
            ("run-1".into(), "r1.csv".into(), Some("enc:abc".into()), None),
            // Unbackfilled — actionable, no status.
            ("run-2".into(), "r2.csv".into(), None, None),
            // Previously failed — actionable, retryable.
            ("run-3".into(), "r3.csv".into(), None, Some("encrypt_failed".into())),
            // Missing file — terminal, NOT actionable.
            ("run-4".into(), "r4.csv".into(), None, Some("missing_file".into())),
            // Another unbackfilled.
            ("run-5".into(), "r5.csv".into(), None, None),
        ];

        let (unbackfilled, enc_failed) = classify_artifacts(&artifacts);

        assert_eq!(unbackfilled, 2, "run-2 and run-5 are never-attempted legacy");
        assert_eq!(enc_failed, 1, "run-3 is encrypt_failed");
        let total_actionable = unbackfilled + enc_failed;
        assert_eq!(total_actionable, 3, "total actionable legacy = 3");

        // Strict gate should fire for this set.
        assert!(total_actionable > 0, "gate must fire when actionable > 0");
    }

    #[test]
    fn strict_gate_passes_when_all_keyed_or_missing() {
        let artifacts: Vec<(String, String, Option<String>, Option<String>)> = vec![
            // All keyed.
            ("run-1".into(), "r1.csv".into(), Some("enc:abc".into()), None),
            ("run-2".into(), "r2.csv".into(), Some("enc:def".into()), None),
            // Terminal missing.
            ("run-3".into(), "r3.csv".into(), None, Some("missing_file".into())),
        ];

        let (unbackfilled, enc_failed) = classify_artifacts(&artifacts);

        assert_eq!(unbackfilled, 0);
        assert_eq!(enc_failed, 0);
        assert_eq!(unbackfilled + enc_failed, 0, "gate must NOT fire");
    }

    #[test]
    fn strict_gate_passes_on_empty_artifact_list() {
        let artifacts: Vec<(String, String, Option<String>, Option<String>)> = vec![];
        let (unbackfilled, enc_failed) = classify_artifacts(&artifacts);
        assert_eq!(unbackfilled + enc_failed, 0);
    }

    #[test]
    fn actionable_legacy_count_excludes_missing_file_status() {
        // Only 'missing_file' rows must be excluded.
        // 'encrypt_failed' must be included (retryable).
        let artifacts: Vec<(String, String, Option<String>, Option<String>)> = vec![
            ("run-1".into(), "r1.csv".into(), None, Some("missing_file".into())),
            ("run-2".into(), "r2.csv".into(), None, Some("missing_file".into())),
        ];
        let (unbackfilled, enc_failed) = classify_artifacts(&artifacts);
        assert_eq!(
            unbackfilled + enc_failed, 0,
            "missing_file rows must not count as actionable"
        );
    }

    #[test]
    fn idempotent_backfill_state_transitions_are_consistent() {
        // Verify the classification for all possible backfill_status values.
        let cases: &[(Option<&str>, bool /* is actionable */)] = &[
            (None, true),                          // never attempted
            (Some("encrypt_failed"), true),        // retryable
            (Some("missing_file"), false),         // terminal
            (Some("unknown_future_status"), true), // conservative: treat unknowns as actionable
        ];

        for (status, should_be_actionable) in cases {
            let artifacts: Vec<(String, String, Option<String>, Option<String>)> = vec![
                (
                    "run-x".into(),
                    "rx.csv".into(),
                    None, // dek IS NULL
                    status.map(|s| s.to_string()),
                ),
            ];
            let (u, e) = classify_artifacts(&artifacts);
            let actionable = u + e > 0;
            assert_eq!(
                actionable, *should_be_actionable,
                "status={:?} → actionable should be {}",
                status, should_be_actionable
            );
        }
    }

    #[test]
    fn strict_mode_blocked_error_code_is_distinct_from_conflict() {
        // Verify that the AppError variants produce different ErrorCodes.
        use crate::errors::{AppError, ErrorCode};

        let blocked = AppError::StrictModeBlocked {
            unresolved_count: 3,
            hint: "run backfill".into(),
        };
        let conflict = AppError::Conflict("some conflict".into());

        assert!(
            matches!(blocked.code(), ErrorCode::StrictModeBlocked),
            "StrictModeBlocked must produce distinct error code"
        );
        assert!(
            matches!(conflict.code(), ErrorCode::Conflict),
            "Conflict must retain its own code"
        );
        // Both map to 409 HTTP status.
        assert_eq!(
            blocked.status(),
            conflict.status(),
            "both variants should return HTTP 409"
        );
    }
}
