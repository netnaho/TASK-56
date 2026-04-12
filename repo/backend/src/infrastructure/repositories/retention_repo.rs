//! Retention policy repository — MySQL-backed persistence.
//!
//! Raw DB queries for `retention_policies`, plus the targeted mutation
//! helpers that the execution engine needs for each entity type.
//!
//! # Safety note on dynamic SQL
//!
//! `count_expired_rows` and `delete_expired_rows` build their FROM / WHERE
//! clause with `format!()` because sqlx does not support binding table names
//! as parameters. Both functions validate the `(table, date_column)` pair
//! against [`ALLOWED_TABLES`] before constructing the query, so user-supplied
//! data can never reach those format strings — only internal service code
//! chooses the pair.

use chrono::NaiveDateTime;
use sqlx::{MySqlPool, Row};
use uuid::Uuid;

use crate::errors::{AppError, AppResult};

// ---------------------------------------------------------------------------
// Whitelist — the only (table, date_column) pairs the execution engine may
// target. Adding a new entity type requires an explicit entry here AND in the
// service layer's match statement.
// ---------------------------------------------------------------------------

const ALLOWED_TABLES: &[(&str, &str)] = &[
    ("audit_logs", "created_at"),
    ("sessions", "expires_at"),
    ("checkin_events", "checked_in_at"),
    ("report_runs", "created_at"),
];

fn validate_table_column(table: &str, date_column: &str) -> AppResult<()> {
    if ALLOWED_TABLES.iter().any(|(t, c)| *t == table && *c == date_column) {
        Ok(())
    } else {
        Err(AppError::Validation(format!(
            "table/column pair ({}, {}) is not on the retention whitelist",
            table, date_column
        )))
    }
}

// ---------------------------------------------------------------------------
// Raw row struct
// ---------------------------------------------------------------------------

/// Raw database row for `retention_policies`.
///
/// Callers should convert this to the domain `RetentionPolicy` or the
/// service-layer `RetentionPolicyView` using the mapping helpers in
/// `retention_service`.
pub struct RetentionPolicyRow {
    pub id: String,
    pub target_entity_type: String,
    pub retention_days: i32,
    pub action: String,
    pub rationale: Option<String>,
    pub is_active: i8,
    pub created_by: Option<String>,
    pub last_executed_at: Option<NaiveDateTime>,
    /// JSON string (or None) — serialised `RetentionExecutionResult`.
    pub last_execution_result: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

// ---------------------------------------------------------------------------
// Mapping helper — turns a sqlx `Row` into `RetentionPolicyRow`.
// ---------------------------------------------------------------------------

fn map_row(row: &sqlx::mysql::MySqlRow) -> AppResult<RetentionPolicyRow> {
    let id: String = row
        .try_get("id")
        .map_err(|e| AppError::Database(format!("retention_policies.id: {}", e)))?;
    let target_entity_type: String = row
        .try_get("target_entity_type")
        .map_err(|e| AppError::Database(format!("retention_policies.target_entity_type: {}", e)))?;
    let retention_days: i32 = row
        .try_get("retention_days")
        .map_err(|e| AppError::Database(format!("retention_policies.retention_days: {}", e)))?;
    let action: String = row
        .try_get("action")
        .map_err(|e| AppError::Database(format!("retention_policies.action: {}", e)))?;
    let rationale: Option<String> = row
        .try_get("rationale")
        .map_err(|e| AppError::Database(format!("retention_policies.rationale: {}", e)))?;
    let is_active: i8 = row
        .try_get::<i8, _>("is_active")
        .map_err(|e| AppError::Database(format!("retention_policies.is_active: {}", e)))?;
    let created_by: Option<String> = row
        .try_get("created_by")
        .map_err(|e| AppError::Database(format!("retention_policies.created_by: {}", e)))?;
    let last_executed_at: Option<NaiveDateTime> = row
        .try_get("last_executed_at")
        .map_err(|e| AppError::Database(format!("retention_policies.last_executed_at: {}", e)))?;
    let last_execution_result: Option<String> = row
        .try_get("last_execution_result")
        .map_err(|e| AppError::Database(format!("retention_policies.last_execution_result: {}", e)))?;
    let created_at: NaiveDateTime = row
        .try_get("created_at")
        .map_err(|e| AppError::Database(format!("retention_policies.created_at: {}", e)))?;
    let updated_at: NaiveDateTime = row
        .try_get("updated_at")
        .map_err(|e| AppError::Database(format!("retention_policies.updated_at: {}", e)))?;

    Ok(RetentionPolicyRow {
        id,
        target_entity_type,
        retention_days,
        action,
        rationale,
        is_active,
        created_by,
        last_executed_at,
        last_execution_result,
        created_at,
        updated_at,
    })
}

// ---------------------------------------------------------------------------
// CRUD
// ---------------------------------------------------------------------------

/// Return all retention policies ordered by target entity type.
pub async fn find_all(pool: &MySqlPool) -> AppResult<Vec<RetentionPolicyRow>> {
    let rows = sqlx::query(
        r#"
        SELECT id, target_entity_type, retention_days, action, rationale,
               is_active, created_by, last_executed_at,
               CAST(last_execution_result AS CHAR) AS last_execution_result,
               created_at, updated_at
          FROM retention_policies
         ORDER BY target_entity_type ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Database(format!("retention_policies find_all: {}", e)))?;

    rows.iter().map(map_row).collect()
}

/// Return a single retention policy by its UUID primary key.
pub async fn find_by_id(pool: &MySqlPool, id: Uuid) -> AppResult<RetentionPolicyRow> {
    let row = sqlx::query(
        r#"
        SELECT id, target_entity_type, retention_days, action, rationale,
               is_active, created_by, last_executed_at,
               CAST(last_execution_result AS CHAR) AS last_execution_result,
               created_at, updated_at
          FROM retention_policies
         WHERE id = ?
        "#,
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("retention_policies find_by_id: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("retention policy {}", id)))?;

    map_row(&row)
}

/// Return the policy governing a specific entity type, if one exists.
pub async fn find_by_entity_type(
    pool: &MySqlPool,
    entity_type: &str,
) -> AppResult<Option<RetentionPolicyRow>> {
    let row = sqlx::query(
        r#"
        SELECT id, target_entity_type, retention_days, action, rationale,
               is_active, created_by, last_executed_at,
               CAST(last_execution_result AS CHAR) AS last_execution_result,
               created_at, updated_at
          FROM retention_policies
         WHERE target_entity_type = ?
         LIMIT 1
        "#,
    )
    .bind(entity_type)
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("retention_policies find_by_entity_type: {}", e)))?;

    row.as_ref().map(map_row).transpose()
}

/// Insert a new retention policy row.
pub async fn insert(pool: &MySqlPool, row: &RetentionPolicyRow) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO retention_policies
            (id, target_entity_type, retention_days, action, rationale,
             is_active, created_by)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&row.id)
    .bind(&row.target_entity_type)
    .bind(row.retention_days)
    .bind(&row.action)
    .bind(&row.rationale)
    .bind(row.is_active)
    .bind(&row.created_by)
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("retention_policies insert: {}", e)))?;

    Ok(())
}

/// Update mutable fields of an existing retention policy.
pub async fn update(
    pool: &MySqlPool,
    id: Uuid,
    retention_days: i32,
    action: &str,
    rationale: Option<&str>,
    is_active: bool,
) -> AppResult<()> {
    let affected = sqlx::query(
        r#"
        UPDATE retention_policies
           SET retention_days = ?,
               action         = ?,
               rationale      = ?,
               is_active      = ?
         WHERE id = ?
        "#,
    )
    .bind(retention_days)
    .bind(action)
    .bind(rationale)
    .bind(if is_active { 1i8 } else { 0i8 })
    .bind(id.to_string())
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("retention_policies update: {}", e)))?
    .rows_affected();

    if affected == 0 {
        return Err(AppError::NotFound(format!("retention policy {}", id)));
    }

    Ok(())
}

/// Stamp the policy with the execution timestamp and serialised result JSON.
pub async fn mark_executed(
    pool: &MySqlPool,
    id: Uuid,
    executed_at: NaiveDateTime,
    result_json: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE retention_policies
           SET last_executed_at     = ?,
               last_execution_result = CAST(? AS JSON)
         WHERE id = ?
        "#,
    )
    .bind(executed_at)
    .bind(result_json)
    .bind(id.to_string())
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("retention_policies mark_executed: {}", e)))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Dry-run count
// ---------------------------------------------------------------------------

/// Count rows eligible for a retention action without mutating anything.
///
/// `table` and `date_column` must be a pair present in [`ALLOWED_TABLES`].
pub async fn count_expired_rows(
    pool: &MySqlPool,
    table: &str,
    date_column: &str,
    cutoff: NaiveDateTime,
) -> AppResult<u64> {
    validate_table_column(table, date_column)?;

    let sql = format!(
        "SELECT COUNT(*) AS cnt FROM `{}` WHERE `{}` <= ?",
        table, date_column
    );

    let row = sqlx::query(&sql)
        .bind(cutoff)
        .fetch_one(pool)
        .await
        .map_err(|e| AppError::Database(format!("count_expired_rows {}: {}", table, e)))?;

    let cnt: i64 = row
        .try_get("cnt")
        .map_err(|e| AppError::Database(format!("count_expired_rows read cnt: {}", e)))?;

    Ok(cnt as u64)
}

// ---------------------------------------------------------------------------
// Destructive helpers
// ---------------------------------------------------------------------------

/// Delete all rows in `table` where `date_column < cutoff`.
///
/// Returns the number of rows deleted. `table` and `date_column` must be a
/// pair present in [`ALLOWED_TABLES`].
pub async fn delete_expired_rows(
    pool: &MySqlPool,
    table: &str,
    date_column: &str,
    cutoff: NaiveDateTime,
) -> AppResult<u64> {
    validate_table_column(table, date_column)?;

    let sql = format!(
        "DELETE FROM `{}` WHERE `{}` <= ?",
        table, date_column
    );

    let result = sqlx::query(&sql)
        .bind(cutoff)
        .execute(pool)
        .await
        .map_err(|e| AppError::Database(format!("delete_expired_rows {}: {}", table, e)))?;

    Ok(result.rows_affected())
}

// ---------------------------------------------------------------------------
// Report-run artifact helpers
// ---------------------------------------------------------------------------

/// Return the `artifact_path` values for all `report_runs` rows whose
/// `created_at` predates `before`.  Paths may be NULL in the schema; those
/// are filtered out here.
pub async fn find_report_run_artifacts(
    pool: &MySqlPool,
    before: NaiveDateTime,
) -> AppResult<Vec<String>> {
    let rows = sqlx::query(
        r#"
        SELECT artifact_path
          FROM report_runs
         WHERE created_at <= ?
           AND artifact_path IS NOT NULL
        "#,
    )
    .bind(before)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Database(format!("find_report_run_artifacts: {}", e)))?;

    let mut paths = Vec::with_capacity(rows.len());
    for row in &rows {
        let path: String = row
            .try_get("artifact_path")
            .map_err(|e| AppError::Database(format!("find_report_run_artifacts path: {}", e)))?;
        paths.push(path);
    }

    Ok(paths)
}

/// Return `(run_id, artifact_path, artifact_dek)` tuples for all `report_runs`
/// rows whose `created_at` predates `before` and that have a non-NULL
/// `artifact_path`.
///
/// `artifact_dek` is `Some` for artifacts encrypted with a per-artifact DEK
/// (Phase 6 hardened) and `None` for legacy artifacts (pre-019 migration or
/// failed writes).  The caller uses this to select the appropriate deletion
/// strategy:
///
/// - `artifact_dek = Some(_)` → cryptographic erasure (NULL the DEK, then physical delete)
/// - `artifact_dek = None, backfill_status = Some("missing_file")` → no file to delete
/// - `artifact_dek = None, backfill_status = Some("encrypt_failed")` → best-effort delete
/// - `artifact_dek = None, backfill_status = None` → legacy best-effort (warn: not backfilled)
///
/// Returns `(run_id, artifact_path, artifact_dek, backfill_status)`.
pub async fn find_report_run_artifacts_with_keys(
    pool: &MySqlPool,
    before: NaiveDateTime,
) -> AppResult<Vec<(String, String, Option<String>, Option<String>)>> {
    let rows = sqlx::query(
        r#"
        SELECT id, artifact_path, artifact_dek, backfill_status
          FROM report_runs
         WHERE created_at <= ?
           AND artifact_path IS NOT NULL
        "#,
    )
    .bind(before)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Database(format!("find_report_run_artifacts_with_keys: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in &rows {
        let run_id: String = row
            .try_get("id")
            .map_err(|e| AppError::Database(format!("find_report_run_artifacts_with_keys id: {}", e)))?;
        let path: String = row
            .try_get("artifact_path")
            .map_err(|e| AppError::Database(format!("find_report_run_artifacts_with_keys path: {}", e)))?;
        // artifact_dek / backfill_status may be absent if the migration has not yet been applied.
        let dek: Option<String> = row.try_get("artifact_dek").unwrap_or(None);
        let backfill_status: Option<String> = row.try_get("backfill_status").unwrap_or(None);
        out.push((run_id, path, dek, backfill_status));
    }

    Ok(out)
}

// ---------------------------------------------------------------------------
// Audit-log anonymisation
// ---------------------------------------------------------------------------

/// Replace `actor_email` and `ip_address` with `[ANONYMIZED]` for every
/// `audit_logs` row whose `created_at < cutoff` and that has not already
/// been anonymised.
///
/// The chain hashes are intentionally left untouched — the anonymisation
/// does not invalidate them because the hashes are over the immutable
/// `action` + `payload`, not over PII fields.
///
/// Returns the number of rows updated.
pub async fn anonymize_audit_actors(
    pool: &MySqlPool,
    cutoff: NaiveDateTime,
) -> AppResult<u64> {
    let result = sqlx::query(
        r#"
        UPDATE audit_logs
           SET actor_email  = '[ANONYMIZED]',
               ip_address   = '[ANONYMIZED]'
         WHERE created_at < ?
           AND (actor_email != '[ANONYMIZED]' OR actor_email IS NULL)
        "#,
    )
    .bind(cutoff)
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("anonymize_audit_actors: {}", e)))?;

    Ok(result.rows_affected())
}
