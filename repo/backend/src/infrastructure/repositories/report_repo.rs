//! Report repository — MySQL-backed persistence for report definitions,
//! runs, and schedules.
//!
//! All structs here are raw DB-facing types (string UUIDs, i8 booleans).
//! The service layer converts them to domain types.

use chrono::NaiveDateTime;
use sqlx::{MySqlPool, Row};
use uuid::Uuid;

use crate::application::scope::ScopeFilter;
use crate::errors::{AppError, AppResult};

// ─── Raw DB structs ────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ReportRow {
    pub id: String,
    pub title: String,
    pub description: Option<String>,
    /// JSON-serialised `ReportQueryDefinition`.
    pub query_definition: String,
    pub default_format: String,
    pub created_by: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone)]
pub struct ReportRunRow {
    pub id: String,
    pub report_id: String,
    pub triggered_by: Option<String>,
    pub triggered_source: String,
    pub format: String,
    pub status: String,
    pub artifact_path: Option<String>,
    pub artifact_size_bytes: Option<i64>,
    /// `enc:<base64url>` wrapped per-artifact DEK; `None` for legacy artifacts.
    pub artifact_dek: Option<String>,
    pub error_message: Option<String>,
    pub started_at: Option<NaiveDateTime>,
    pub completed_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Clone)]
pub struct ReportScheduleRow {
    pub id: String,
    pub report_id: String,
    pub cron_expression: String,
    pub department_scope_id: Option<String>,
    pub is_active: i8,
    pub format: String,
    pub last_run_at: Option<NaiveDateTime>,
    pub next_run_at: Option<NaiveDateTime>,
    pub created_by: Option<String>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

// ─── Report CRUD ───────────────────────────────────────────────────────────────

/// List all reports visible under the given scope.
/// Scope is based on the report's `created_by` user's department (joined via users table).
/// For ScopeFilter::All, every report is returned.
/// For department scopes, only reports created by users in the same department are returned.
pub async fn find_all(pool: &MySqlPool, scope: &ScopeFilter) -> AppResult<Vec<ReportRow>> {
    let mut sql = String::from(
        r#"
        SELECT r.id, r.title, r.description,
               CAST(r.query_definition AS CHAR) AS query_definition,
               r.default_format, r.created_by,
               r.created_at, r.updated_at
          FROM reports r
          LEFT JOIN users u ON r.created_by = u.id
         WHERE 1=1
        "#,
    );

    let dept_id: Option<Uuid> = match scope {
        ScopeFilter::All => None,
        ScopeFilter::Department(d) => Some(*d),
        ScopeFilter::DepartmentOrOwned { department_id, .. } => Some(*department_id),
        ScopeFilter::OwnedBy(_) => None,
        ScopeFilter::None => {
            // Return empty set immediately — no rows visible.
            return Ok(Vec::new());
        }
    };

    let owner_id: Option<Uuid> = match scope {
        ScopeFilter::OwnedBy(uid) => Some(*uid),
        ScopeFilter::DepartmentOrOwned { owner_id, .. } => Some(*owner_id),
        _ => None,
    };

    // Build WHERE clause
    match scope {
        ScopeFilter::All => {}
        ScopeFilter::Department(_) => {
            sql.push_str(" AND u.department_id = ?");
        }
        ScopeFilter::OwnedBy(_) => {
            sql.push_str(" AND r.created_by = ?");
        }
        ScopeFilter::DepartmentOrOwned { .. } => {
            sql.push_str(" AND (u.department_id = ? OR r.created_by = ?)");
        }
        ScopeFilter::None => unreachable!(),
    }

    sql.push_str(" ORDER BY r.created_at DESC");

    let mut q = sqlx::query(&sql);

    match scope {
        ScopeFilter::All => {}
        ScopeFilter::Department(_) => {
            q = q.bind(dept_id.unwrap().to_string());
        }
        ScopeFilter::OwnedBy(_) => {
            q = q.bind(owner_id.unwrap().to_string());
        }
        ScopeFilter::DepartmentOrOwned { .. } => {
            q = q.bind(dept_id.unwrap().to_string());
            q = q.bind(owner_id.unwrap().to_string());
        }
        ScopeFilter::None => unreachable!(),
    }

    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("find_all reports: {}", e)))?;

    rows.into_iter().map(parse_report_row).collect()
}

pub async fn find_by_id(pool: &MySqlPool, id: Uuid) -> AppResult<ReportRow> {
    let row = sqlx::query(
        r#"
        SELECT id, title, description,
               CAST(query_definition AS CHAR) AS query_definition,
               default_format, created_by, created_at, updated_at
          FROM reports
         WHERE id = ?
        "#,
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("find_by_id report: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("report {}", id)))?;

    parse_report_row(row)
}

pub async fn insert(pool: &MySqlPool, row: &ReportRow) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO reports (id, title, description, query_definition, default_format, created_by, created_at, updated_at)
        VALUES (?, ?, ?, CAST(? AS JSON), ?, ?, ?, ?)
        "#,
    )
    .bind(&row.id)
    .bind(&row.title)
    .bind(&row.description)
    .bind(&row.query_definition)
    .bind(&row.default_format)
    .bind(&row.created_by)
    .bind(row.created_at)
    .bind(row.updated_at)
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("insert report: {}", e)))?;
    Ok(())
}

pub async fn update_meta(
    pool: &MySqlPool,
    id: Uuid,
    title: &str,
    description: Option<&str>,
    default_format: &str,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE reports
           SET title = ?, description = ?, default_format = ?, updated_at = NOW()
         WHERE id = ?
        "#,
    )
    .bind(title)
    .bind(description)
    .bind(default_format)
    .bind(id.to_string())
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("update_meta report: {}", e)))?;
    Ok(())
}

// ─── Report Runs ───────────────────────────────────────────────────────────────

pub async fn find_runs(
    pool: &MySqlPool,
    report_id: Uuid,
    limit: i64,
) -> AppResult<Vec<ReportRunRow>> {
    let rows = sqlx::query(
        r#"
        SELECT id, report_id, triggered_by, triggered_source, format, status,
               artifact_path, artifact_size_bytes, artifact_dek, error_message,
               started_at, completed_at, created_at
          FROM report_runs
         WHERE report_id = ?
         ORDER BY created_at DESC
         LIMIT ?
        "#,
    )
    .bind(report_id.to_string())
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Database(format!("find_runs: {}", e)))?;

    rows.into_iter().map(parse_run_row).collect()
}

pub async fn find_run_by_id(pool: &MySqlPool, id: Uuid) -> AppResult<ReportRunRow> {
    let row = sqlx::query(
        r#"
        SELECT id, report_id, triggered_by, triggered_source, format, status,
               artifact_path, artifact_size_bytes, artifact_dek, error_message,
               started_at, completed_at, created_at
          FROM report_runs
         WHERE id = ?
        "#,
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("find_run_by_id: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("report run {}", id)))?;

    parse_run_row(row)
}

pub async fn insert_run(pool: &MySqlPool, row: &ReportRunRow) -> AppResult<()> {
    // created_at is intentionally omitted — MySQL's DEFAULT CURRENT_TIMESTAMP
    // stores the exact second floor (no rounding). If we bind Rust's
    // NaiveDateTime, MySQL DATETIME rounds sub-seconds (e.g. 14:37:40.875 →
    // 14:37:41), which can cause the retention cutoff (Rust's Utc::now(), also
    // sub-second) to fall *before* the stored value and miss the row.
    sqlx::query(
        r#"
        INSERT INTO report_runs
            (id, report_id, triggered_by, triggered_source, format, status,
             artifact_path, artifact_size_bytes, artifact_dek, error_message,
             started_at, completed_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&row.id)
    .bind(&row.report_id)
    .bind(&row.triggered_by)
    .bind(&row.triggered_source)
    .bind(&row.format)
    .bind(&row.status)
    .bind(&row.artifact_path)
    .bind(row.artifact_size_bytes)
    .bind(&row.artifact_dek)
    .bind(&row.error_message)
    .bind(row.started_at)
    .bind(row.completed_at)
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("insert_run: {}", e)))?;
    Ok(())
}

pub async fn update_run_started(pool: &MySqlPool, id: Uuid) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE report_runs
           SET status = 'running', started_at = NOW()
         WHERE id = ?
        "#,
    )
    .bind(id.to_string())
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("update_run_started: {}", e)))?;
    Ok(())
}

pub async fn update_run_completed(
    pool: &MySqlPool,
    id: Uuid,
    artifact_path: &str,
    artifact_size_bytes: i64,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE report_runs
           SET status = 'completed',
               artifact_path = ?,
               artifact_size_bytes = ?,
               completed_at = NOW()
         WHERE id = ?
        "#,
    )
    .bind(artifact_path)
    .bind(artifact_size_bytes)
    .bind(id.to_string())
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("update_run_completed: {}", e)))?;
    Ok(())
}

pub async fn update_run_failed(pool: &MySqlPool, id: Uuid, error_message: &str) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE report_runs
           SET status = 'failed',
               error_message = ?,
               completed_at = NOW()
         WHERE id = ?
        "#,
    )
    .bind(error_message)
    .bind(id.to_string())
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("update_run_failed: {}", e)))?;
    Ok(())
}

/// Store the wrapped per-artifact DEK after the encrypted artifact is written.
///
/// Called by the report generation path once the on-disk file has been
/// encrypted in-place and the DEK is known.
pub async fn update_run_artifact_dek(pool: &MySqlPool, id: Uuid, wrapped_dek: &str) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE report_runs
           SET artifact_dek = ?
         WHERE id = ?
        "#,
    )
    .bind(wrapped_dek)
    .bind(id.to_string())
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("update_run_artifact_dek: {}", e)))?;
    Ok(())
}

/// Perform cryptographic erasure by NULLing the artifact DEK.
///
/// Once the DEK is gone the on-disk ciphertext is permanently irrecoverable,
/// even on copy-on-write filesystems where physical block zeroing is
/// not guaranteed.  The physical file should then be removed best-effort.
///
/// This operation is idempotent: if `artifact_dek` is already NULL, the
/// UPDATE simply matches zero rows.
pub async fn erase_run_artifact_dek(pool: &MySqlPool, id: Uuid) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE report_runs
           SET artifact_dek = NULL
         WHERE id = ?
        "#,
    )
    .bind(id.to_string())
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("erase_run_artifact_dek: {}", e)))?;
    Ok(())
}

// ─── Artifact backfill helpers ────────────────────────────────────────────────

/// A legacy `report_runs` row that has a file on disk but no DEK yet.
#[derive(Debug, Clone)]
pub struct LegacyArtifactRun {
    pub run_id: String,
    pub artifact_path: String,
}

/// Return up to `limit` legacy artifact rows eligible for backfill, starting
/// at `offset`.
///
/// A row is eligible when:
///   - `artifact_path IS NOT NULL` (a file path is recorded)
///   - `artifact_dek IS NULL` (not yet encrypted)
///   - `backfill_status IS NULL OR backfill_status = 'encrypt_failed'`
///     (`'missing_file'` rows are skipped — the file is gone and there's
///     nothing to encrypt; `'encrypt_failed'` rows are retried)
///
/// Ordered by `created_at ASC` so the oldest artifacts are processed first.
pub async fn find_legacy_artifact_runs(
    pool: &MySqlPool,
    limit: u32,
    offset: u32,
) -> AppResult<Vec<LegacyArtifactRun>> {
    let rows = sqlx::query(
        r#"
        SELECT id, artifact_path
          FROM report_runs
         WHERE artifact_path IS NOT NULL
           AND artifact_dek  IS NULL
           AND (backfill_status IS NULL OR backfill_status = 'encrypt_failed')
         ORDER BY created_at ASC
         LIMIT ? OFFSET ?
        "#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Database(format!("find_legacy_artifact_runs: {}", e)))?;

    let mut out = Vec::with_capacity(rows.len());
    for row in &rows {
        let run_id: String = row
            .try_get("id")
            .map_err(|e| AppError::Database(format!("legacy_artifact_runs id: {}", e)))?;
        let artifact_path: String = row
            .try_get("artifact_path")
            .map_err(|e| AppError::Database(format!("legacy_artifact_runs path: {}", e)))?;
        out.push(LegacyArtifactRun { run_id, artifact_path });
    }
    Ok(out)
}

/// Count all eligible legacy artifact rows (same WHERE clause as
/// [`find_legacy_artifact_runs`] without pagination).
pub async fn count_legacy_artifact_runs(pool: &MySqlPool) -> AppResult<u64> {
    let row = sqlx::query(
        r#"
        SELECT COUNT(*) AS cnt
          FROM report_runs
         WHERE artifact_path IS NOT NULL
           AND artifact_dek  IS NULL
           AND (backfill_status IS NULL OR backfill_status = 'encrypt_failed')
        "#,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::Database(format!("count_legacy_artifact_runs: {}", e)))?;

    let cnt: i64 = row
        .try_get("cnt")
        .map_err(|e| AppError::Database(format!("count_legacy_artifact_runs cnt: {}", e)))?;
    Ok(cnt as u64)
}

/// Count rows permanently marked as `backfill_status = 'missing_file'`.
pub async fn count_missing_file_artifact_runs(pool: &MySqlPool) -> AppResult<u64> {
    let row = sqlx::query(
        r#"
        SELECT COUNT(*) AS cnt
          FROM report_runs
         WHERE artifact_path IS NOT NULL
           AND artifact_dek  IS NULL
           AND backfill_status = 'missing_file'
        "#,
    )
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::Database(format!("count_missing_file_artifact_runs: {}", e)))?;

    let cnt: i64 = row
        .try_get("cnt")
        .map_err(|e| AppError::Database(format!("count_missing_file_artifact_runs cnt: {}", e)))?;
    Ok(cnt as u64)
}

/// Set `backfill_status` for a run row.
///
/// `status` should be one of `"missing_file"` or `"encrypt_failed"`.
/// Passing `None` clears the field (used if a previously-failed row is
/// retried and succeeds — though normally success is indicated by the DEK
/// being written, not by this field being cleared).
pub async fn set_artifact_backfill_status(
    pool: &MySqlPool,
    id: Uuid,
    status: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE report_runs
           SET backfill_status = ?
         WHERE id = ?
        "#,
    )
    .bind(status)
    .bind(id.to_string())
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("set_artifact_backfill_status: {}", e)))?;
    Ok(())
}

/// Count actionable legacy artifact rows whose `created_at` predates `before`.
///
/// "Actionable legacy" means the artifact has no DEK yet AND the file may
/// still be on disk (i.e., `backfill_status` is NOT `'missing_file'`).
/// These are the rows that strict-mode retention would need to delete but
/// cannot safely do so without a DEK.
///
/// Returns `(unbackfilled_count, encrypt_failed_count)`.
pub async fn count_actionable_legacy_expired_artifacts(
    pool: &MySqlPool,
    before: NaiveDateTime,
) -> AppResult<(u64, u64)> {
    let unbackfilled_row = sqlx::query(
        r#"
        SELECT COUNT(*) AS cnt
          FROM report_runs
         WHERE created_at     < ?
           AND artifact_path IS NOT NULL
           AND artifact_dek  IS NULL
           AND backfill_status IS NULL
        "#,
    )
    .bind(before)
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::Database(format!("count_actionable_legacy_expired unbackfilled: {}", e)))?;

    let unbackfilled: i64 = unbackfilled_row
        .try_get("cnt")
        .map_err(|e| AppError::Database(format!("count_actionable_legacy_expired unbackfilled cnt: {}", e)))?;

    let failed_row = sqlx::query(
        r#"
        SELECT COUNT(*) AS cnt
          FROM report_runs
         WHERE created_at     < ?
           AND artifact_path IS NOT NULL
           AND artifact_dek  IS NULL
           AND backfill_status = 'encrypt_failed'
        "#,
    )
    .bind(before)
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::Database(format!("count_actionable_legacy_expired failed: {}", e)))?;

    let failed: i64 = failed_row
        .try_get("cnt")
        .map_err(|e| AppError::Database(format!("count_actionable_legacy_expired failed cnt: {}", e)))?;

    Ok((unbackfilled as u64, failed as u64))
}

// ─── Report Schedules ──────────────────────────────────────────────────────────

pub async fn find_schedules(
    pool: &MySqlPool,
    report_id: Uuid,
) -> AppResult<Vec<ReportScheduleRow>> {
    let rows = sqlx::query(
        r#"
        SELECT id, report_id, cron_expression, department_scope_id,
               is_active, format, last_run_at, next_run_at,
               created_by, created_at, updated_at
          FROM report_schedules
         WHERE report_id = ?
         ORDER BY created_at DESC
        "#,
    )
    .bind(report_id.to_string())
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Database(format!("find_schedules: {}", e)))?;

    rows.into_iter().map(parse_schedule_row).collect()
}

pub async fn find_schedule_by_id(pool: &MySqlPool, id: Uuid) -> AppResult<ReportScheduleRow> {
    let row = sqlx::query(
        r#"
        SELECT id, report_id, cron_expression, department_scope_id,
               is_active, format, last_run_at, next_run_at,
               created_by, created_at, updated_at
          FROM report_schedules
         WHERE id = ?
        "#,
    )
    .bind(id.to_string())
    .fetch_optional(pool)
    .await
    .map_err(|e| AppError::Database(format!("find_schedule_by_id: {}", e)))?
    .ok_or_else(|| AppError::NotFound(format!("report schedule {}", id)))?;

    parse_schedule_row(row)
}

pub async fn insert_schedule(pool: &MySqlPool, row: &ReportScheduleRow) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO report_schedules
            (id, report_id, cron_expression, department_scope_id,
             is_active, format, last_run_at, next_run_at,
             created_by, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&row.id)
    .bind(&row.report_id)
    .bind(&row.cron_expression)
    .bind(&row.department_scope_id)
    .bind(row.is_active)
    .bind(&row.format)
    .bind(row.last_run_at)
    .bind(row.next_run_at)
    .bind(&row.created_by)
    .bind(row.created_at)
    .bind(row.updated_at)
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("insert_schedule: {}", e)))?;
    Ok(())
}

pub async fn update_schedule(
    pool: &MySqlPool,
    id: Uuid,
    cron_expression: &str,
    is_active: bool,
    format: &str,
    department_scope_id: Option<Uuid>,
    next_run_at: Option<NaiveDateTime>,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE report_schedules
           SET cron_expression = ?,
               is_active = ?,
               format = ?,
               department_scope_id = ?,
               next_run_at = ?,
               updated_at = NOW()
         WHERE id = ?
        "#,
    )
    .bind(cron_expression)
    .bind(if is_active { 1i8 } else { 0i8 })
    .bind(format)
    .bind(department_scope_id.map(|u| u.to_string()))
    .bind(next_run_at)
    .bind(id.to_string())
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("update_schedule: {}", e)))?;
    Ok(())
}

pub async fn delete_schedule(pool: &MySqlPool, id: Uuid) -> AppResult<()> {
    sqlx::query("DELETE FROM report_schedules WHERE id = ?")
        .bind(id.to_string())
        .execute(pool)
        .await
        .map_err(|e| AppError::Database(format!("delete_schedule: {}", e)))?;
    Ok(())
}

/// Find all active schedules whose `next_run_at` is at or before NOW().
pub async fn find_due_schedules(pool: &MySqlPool) -> AppResult<Vec<ReportScheduleRow>> {
    let rows = sqlx::query(
        r#"
        SELECT id, report_id, cron_expression, department_scope_id,
               is_active, format, last_run_at, next_run_at,
               created_by, created_at, updated_at
          FROM report_schedules
         WHERE is_active = 1
           AND next_run_at <= NOW()
         ORDER BY next_run_at ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Database(format!("find_due_schedules: {}", e)))?;

    rows.into_iter().map(parse_schedule_row).collect()
}

/// Find all active schedules where `next_run_at` is NULL (need recomputation).
pub async fn find_active_schedules_without_next_run(
    pool: &MySqlPool,
) -> AppResult<Vec<ReportScheduleRow>> {
    let rows = sqlx::query(
        r#"
        SELECT id, report_id, cron_expression, department_scope_id,
               is_active, format, last_run_at, next_run_at,
               created_by, created_at, updated_at
          FROM report_schedules
         WHERE is_active = 1
           AND next_run_at IS NULL
        "#,
    )
    .fetch_all(pool)
    .await
    .map_err(|e| AppError::Database(format!("find_active_schedules_without_next_run: {}", e)))?;

    rows.into_iter().map(parse_schedule_row).collect()
}

pub async fn update_schedule_ran(
    pool: &MySqlPool,
    id: Uuid,
    last_run_at: NaiveDateTime,
    next_run_at: Option<NaiveDateTime>,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE report_schedules
           SET last_run_at = ?,
               next_run_at = ?,
               updated_at = NOW()
         WHERE id = ?
        "#,
    )
    .bind(last_run_at)
    .bind(next_run_at)
    .bind(id.to_string())
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("update_schedule_ran: {}", e)))?;
    Ok(())
}

/// Update only the `next_run_at` of a schedule (used at startup recomputation).
pub async fn update_schedule_next_run(
    pool: &MySqlPool,
    id: Uuid,
    next_run_at: NaiveDateTime,
) -> AppResult<()> {
    sqlx::query(
        r#"
        UPDATE report_schedules
           SET next_run_at = ?, updated_at = NOW()
         WHERE id = ?
        "#,
    )
    .bind(next_run_at)
    .bind(id.to_string())
    .execute(pool)
    .await
    .map_err(|e| AppError::Database(format!("update_schedule_next_run: {}", e)))?;
    Ok(())
}

/// Delete runs older than `before` for a specific report (or all reports if `None`).
/// Returns the number of rows deleted.
pub async fn delete_runs_before(
    pool: &MySqlPool,
    report_id: Option<Uuid>,
    before: NaiveDateTime,
) -> AppResult<u64> {
    let result = if let Some(rid) = report_id {
        sqlx::query(
            "DELETE FROM report_runs WHERE report_id = ? AND created_at < ?",
        )
        .bind(rid.to_string())
        .bind(before)
        .execute(pool)
        .await
        .map_err(|e| AppError::Database(format!("delete_runs_before: {}", e)))?
    } else {
        sqlx::query("DELETE FROM report_runs WHERE created_at < ?")
            .bind(before)
            .execute(pool)
            .await
            .map_err(|e| AppError::Database(format!("delete_runs_before (all): {}", e)))?
    };
    Ok(result.rows_affected())
}

// ─── Row parsers ───────────────────────────────────────────────────────────────

fn parse_report_row(row: sqlx::mysql::MySqlRow) -> AppResult<ReportRow> {
    let id: String = row
        .try_get("id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let title: String = row
        .try_get("title")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let description: Option<String> = row
        .try_get("description")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let query_definition: String = row
        .try_get("query_definition")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let default_format: String = row
        .try_get("default_format")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let created_by: Option<String> = row
        .try_get("created_by")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let created_at: NaiveDateTime = row
        .try_get("created_at")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let updated_at: NaiveDateTime = row
        .try_get("updated_at")
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(ReportRow {
        id,
        title,
        description,
        query_definition,
        default_format,
        created_by,
        created_at,
        updated_at,
    })
}

fn parse_run_row(row: sqlx::mysql::MySqlRow) -> AppResult<ReportRunRow> {
    let id: String = row
        .try_get("id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let report_id: String = row
        .try_get("report_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let triggered_by: Option<String> = row
        .try_get("triggered_by")
        .map_err(|e| AppError::Database(e.to_string()))?;
    // triggered_source may not exist on old schemas — default to "manual"
    let triggered_source: String = row
        .try_get("triggered_source")
        .unwrap_or_else(|_| "manual".to_string());
    let format: String = row
        .try_get("format")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let status: String = row
        .try_get("status")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let artifact_path: Option<String> = row
        .try_get("artifact_path")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let artifact_size_bytes: Option<i64> = row
        .try_get("artifact_size_bytes")
        .map_err(|e| AppError::Database(e.to_string()))?;
    // artifact_dek may be absent on old schemas (migration not yet applied) —
    // default to None so the legacy deletion path is used.
    let artifact_dek: Option<String> = row.try_get("artifact_dek").unwrap_or(None);
    let error_message: Option<String> = row
        .try_get("error_message")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let started_at: Option<NaiveDateTime> = row
        .try_get("started_at")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let completed_at: Option<NaiveDateTime> = row
        .try_get("completed_at")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let created_at: NaiveDateTime = row
        .try_get("created_at")
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(ReportRunRow {
        id,
        report_id,
        triggered_by,
        triggered_source,
        format,
        status,
        artifact_path,
        artifact_size_bytes,
        artifact_dek,
        error_message,
        started_at,
        completed_at,
        created_at,
    })
}

fn parse_schedule_row(row: sqlx::mysql::MySqlRow) -> AppResult<ReportScheduleRow> {
    let id: String = row
        .try_get("id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let report_id: String = row
        .try_get("report_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let cron_expression: String = row
        .try_get("cron_expression")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let department_scope_id: Option<String> = row
        .try_get("department_scope_id")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let is_active: i8 = row
        .try_get::<i8, _>("is_active")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let format: String = row
        .try_get("format")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let last_run_at: Option<NaiveDateTime> = row
        .try_get("last_run_at")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let next_run_at: Option<NaiveDateTime> = row
        .try_get("next_run_at")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let created_by: Option<String> = row
        .try_get("created_by")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let created_at: NaiveDateTime = row
        .try_get("created_at")
        .map_err(|e| AppError::Database(e.to_string()))?;
    let updated_at: NaiveDateTime = row
        .try_get("updated_at")
        .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(ReportScheduleRow {
        id,
        report_id,
        cron_expression,
        department_scope_id,
        is_active,
        format,
        last_run_at,
        next_run_at,
        created_by,
        created_at,
        updated_at,
    })
}
