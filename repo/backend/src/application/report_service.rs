//! Report service — CRUD, on-demand execution, scheduling, and artifact download.
//!
//! All public functions enforce capability checks as their first step.
//! Department-level scope is applied when listing reports so non-admin
//! callers only see reports relevant to their department.

use chrono::{NaiveDateTime, Utc};
use cron::Schedule;
use serde::{Deserialize, Serialize};
use sqlx::{MySqlPool, Row};
use std::str::FromStr;
use tokio::fs;
use uuid::Uuid;

use crate::application::artifact_crypto;
use crate::application::audit_service::{self, actions, AuditEvent};
use crate::application::authorization::{require, Capability};
use crate::application::encryption::FieldEncryption;
use crate::application::principal::{Principal, Role};
use crate::application::scope::{content_scope, require_object_visible, ScopeFilter};
use crate::domain::report::{
    ReportFilters, ReportFormat, ReportQueryDefinition, ReportType, RunStatus, TriggeredSource,
};
use crate::errors::{AppError, AppResult};
use crate::infrastructure::repositories::report_repo::{
    self, ReportRow, ReportRunRow, ReportScheduleRow,
};

// ─── View models ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ReportView {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub query_definition: ReportQueryDefinition,
    pub default_format: ReportFormat,
    pub created_by: Option<Uuid>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportRunView {
    pub id: Uuid,
    pub report_id: Uuid,
    pub report_title: String,
    pub triggered_by: Option<Uuid>,
    pub triggered_source: TriggeredSource,
    pub format: ReportFormat,
    pub status: RunStatus,
    /// True when artifact_path is Some and the file exists on disk.
    pub artifact_available: bool,
    pub artifact_size_bytes: Option<i64>,
    pub error_message: Option<String>,
    pub started_at: Option<NaiveDateTime>,
    pub completed_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReportScheduleView {
    pub id: Uuid,
    pub report_id: Uuid,
    pub report_title: String,
    pub cron_expression: String,
    pub department_scope_id: Option<Uuid>,
    pub is_active: bool,
    pub format: ReportFormat,
    pub last_run_at: Option<NaiveDateTime>,
    pub next_run_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

// ─── Input types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct CreateReportInput {
    pub title: String,
    pub description: Option<String>,
    pub query_definition: ReportQueryDefinition,
    pub default_format: Option<ReportFormat>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateReportInput {
    pub title: Option<String>,
    pub description: Option<String>,
    pub default_format: Option<ReportFormat>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TriggerRunInput {
    pub format: Option<ReportFormat>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateScheduleInput {
    pub cron_expression: String,
    pub department_scope_id: Option<Uuid>,
    pub format: Option<ReportFormat>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateScheduleInput {
    pub cron_expression: Option<String>,
    pub department_scope_id: Option<Uuid>,
    pub format: Option<ReportFormat>,
    pub is_active: Option<bool>,
}

// ─── Cron helper ──────────────────────────────────────────────────────────────

pub fn compute_next_run(cron_expr: &str) -> AppResult<Option<NaiveDateTime>> {
    let schedule = Schedule::from_str(cron_expr)
        .map_err(|e| AppError::Validation(format!("invalid cron expression: {}", e)))?;
    Ok(schedule.upcoming(Utc).next().map(|dt| dt.naive_utc()))
}

// ─── Conversion helpers ───────────────────────────────────────────────────────

fn row_to_report_view(row: &ReportRow) -> AppResult<ReportView> {
    let id_str: String = row.id.clone();
    let id = Uuid::parse_str(&id_str)
        .map_err(|e| AppError::Internal(format!("bad uuid: {}", e)))?;

    let query_definition: ReportQueryDefinition =
        serde_json::from_str(&row.query_definition).map_err(|e| {
            AppError::Internal(format!("failed to parse query_definition JSON: {}", e))
        })?;

    let default_format = ReportFormat::from_db(&row.default_format)
        .unwrap_or(ReportFormat::Csv);

    let created_by = row
        .created_by
        .as_deref()
        .map(|s| Uuid::parse_str(s).map_err(|e| AppError::Internal(format!("bad uuid: {}", e))))
        .transpose()?;

    Ok(ReportView {
        id,
        title: row.title.clone(),
        description: row.description.clone(),
        query_definition,
        default_format,
        created_by,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn row_to_run_view(
    row: &ReportRunRow,
    report_title: String,
    reports_storage_path: &str,
) -> AppResult<ReportRunView> {
    let id_str: String = row.id.clone();
    let id = Uuid::parse_str(&id_str)
        .map_err(|e| AppError::Internal(format!("bad uuid: {}", e)))?;

    let report_id_str: String = row.report_id.clone();
    let report_id = Uuid::parse_str(&report_id_str)
        .map_err(|e| AppError::Internal(format!("bad uuid: {}", e)))?;

    let triggered_by = row
        .triggered_by
        .as_deref()
        .map(|s| Uuid::parse_str(s).map_err(|e| AppError::Internal(format!("bad uuid: {}", e))))
        .transpose()?;

    let triggered_source =
        TriggeredSource::from_db(&row.triggered_source).unwrap_or(TriggeredSource::Manual);

    let format = ReportFormat::from_db(&row.format).unwrap_or(ReportFormat::Csv);
    let status = RunStatus::from_db(&row.status).unwrap_or(RunStatus::Queued);

    // Check if the artifact file actually exists on disk.
    let artifact_available = if let Some(ref path) = row.artifact_path {
        let full_path = format!("{}/{}", reports_storage_path, path);
        std::path::Path::new(&full_path).exists()
    } else {
        false
    };

    Ok(ReportRunView {
        id,
        report_id,
        report_title,
        triggered_by,
        triggered_source,
        format,
        status,
        artifact_available,
        artifact_size_bytes: row.artifact_size_bytes,
        error_message: row.error_message.clone(),
        started_at: row.started_at,
        completed_at: row.completed_at,
        created_at: row.created_at,
    })
}

fn row_to_schedule_view(row: &ReportScheduleRow, report_title: String) -> AppResult<ReportScheduleView> {
    let id_str: String = row.id.clone();
    let id = Uuid::parse_str(&id_str)
        .map_err(|e| AppError::Internal(format!("bad uuid: {}", e)))?;

    let report_id_str: String = row.report_id.clone();
    let report_id = Uuid::parse_str(&report_id_str)
        .map_err(|e| AppError::Internal(format!("bad uuid: {}", e)))?;

    let department_scope_id = row
        .department_scope_id
        .as_deref()
        .map(|s| Uuid::parse_str(s).map_err(|e| AppError::Internal(format!("bad uuid: {}", e))))
        .transpose()?;

    let is_active = row.is_active != 0;
    let format = ReportFormat::from_db(&row.format).unwrap_or(ReportFormat::Csv);

    Ok(ReportScheduleView {
        id,
        report_id,
        report_title,
        cron_expression: row.cron_expression.clone(),
        department_scope_id,
        is_active,
        format,
        last_run_at: row.last_run_at,
        next_run_at: row.next_run_at,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

// ─── Private helpers ──────────────────────────────────────────────────────────

/// Fetch the department_id of a report's creator so that department-scoped
/// principals cannot access reports that belong to a different department.
async fn creator_department(pool: &MySqlPool, created_by: Option<&str>) -> AppResult<Option<Uuid>> {
    let uid_str = match created_by {
        Some(s) => s,
        None => return Ok(None),
    };
    let row = sqlx::query("SELECT department_id FROM users WHERE id = ? LIMIT 1")
        .bind(uid_str)
        .fetch_optional(pool)
        .await
        .map_err(|e| AppError::Database(format!("creator_department: {}", e)))?;
    let dept = row
        .and_then(|r| r.try_get::<Option<String>, _>("department_id").ok().flatten())
        .and_then(|s| Uuid::parse_str(&s).ok());
    Ok(dept)
}

// ─── Public service API ────────────────────────────────────────────────────────

pub async fn list_reports(pool: &MySqlPool, principal: &Principal) -> AppResult<Vec<ReportView>> {
    require(principal, Capability::ReportRead)?;
    let scope = content_scope(principal);
    let rows = report_repo::find_all(pool, &scope).await?;
    rows.iter().map(row_to_report_view).collect()
}

pub async fn get_report(pool: &MySqlPool, principal: &Principal, id: Uuid) -> AppResult<ReportView> {
    require(principal, Capability::ReportRead)?;
    let row = report_repo::find_by_id(pool, id).await?;
    // Enforce department scope: callers who are not Admin/Librarian may only
    // see reports created by users in their own department.
    let scope = content_scope(principal);
    let created_by_uuid = row.created_by.as_deref().and_then(|s| Uuid::parse_str(s).ok());
    let creator_dept = creator_department(pool, row.created_by.as_deref()).await?;
    require_object_visible(&scope, created_by_uuid, creator_dept)?;
    row_to_report_view(&row)
}

pub async fn create_report(
    pool: &MySqlPool,
    principal: &Principal,
    input: CreateReportInput,
) -> AppResult<ReportView> {
    require(principal, Capability::ReportManage)?;

    // SECURITY-GATE: AuditRead required for AuditSummary — do not remove.
    // Audit-summary reports read from the audit log. Require AuditRead here so
    // roles that lack it (Librarian, DepartmentHead) cannot persist a report
    // definition that would later let them extract audit data via trigger_run.
    // Only Admin has both ReportManage and AuditRead, so this check is
    // effectively Admin-only for this report type.
    // Regression covered by: db_non_audit_role_denied_for_audit_summary_report (api_routes_test)
    if input.query_definition.report_type == ReportType::AuditSummary {
        require(principal, Capability::AuditRead)?;
    }

    if input.title.trim().is_empty() {
        return Err(AppError::Validation("title must not be empty".to_string()));
    }

    let id = Uuid::new_v4();
    let now = Utc::now().naive_utc();
    let format = input.default_format.unwrap_or(ReportFormat::Csv);
    let qd_json = serde_json::to_string(&input.query_definition)
        .map_err(|e| AppError::Internal(format!("serialize query_definition: {}", e)))?;

    let row = ReportRow {
        id: id.to_string(),
        title: input.title.clone(),
        description: input.description.clone(),
        query_definition: qd_json,
        default_format: format.as_db().to_string(),
        created_by: Some(principal.user_id.to_string()),
        created_at: now,
        updated_at: now,
    };

    report_repo::insert(pool, &row).await?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: actions::REPORT_CREATE,
            target_entity_type: Some("report"),
            target_entity_id: Some(id),
            change_payload: Some(serde_json::json!({ "title": input.title })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    let created = report_repo::find_by_id(pool, id).await?;
    row_to_report_view(&created)
}

pub async fn update_report(
    pool: &MySqlPool,
    principal: &Principal,
    id: Uuid,
    input: UpdateReportInput,
) -> AppResult<ReportView> {
    require(principal, Capability::ReportManage)?;

    let existing = report_repo::find_by_id(pool, id).await?;

    let new_title = input.title.as_deref().unwrap_or(&existing.title);
    if new_title.trim().is_empty() {
        return Err(AppError::Validation("title must not be empty".to_string()));
    }

    let new_description = match &input.description {
        Some(d) => Some(d.as_str()),
        None => existing.description.as_deref(),
    };

    let new_format = input
        .default_format
        .map(|f| f.as_db().to_string())
        .unwrap_or(existing.default_format.clone());

    report_repo::update_meta(pool, id, new_title, new_description, &new_format).await?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: actions::REPORT_UPDATE,
            target_entity_type: Some("report"),
            target_entity_id: Some(id),
            change_payload: Some(serde_json::json!({
                "title": new_title,
                "default_format": new_format,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    let updated = report_repo::find_by_id(pool, id).await?;
    row_to_report_view(&updated)
}

pub async fn trigger_run(
    pool: &MySqlPool,
    principal: &Principal,
    report_id: Uuid,
    format: Option<ReportFormat>,
    reports_storage_path: &str,
    enc: &FieldEncryption,
) -> AppResult<ReportRunView> {
    require(principal, Capability::ReportExecute)?;

    let report_row = report_repo::find_by_id(pool, report_id).await?;
    let effective_format = format
        .or_else(|| ReportFormat::from_db(&report_row.default_format))
        .unwrap_or(ReportFormat::Csv);

    // ── Type-specific capability checks (before any run state is written) ──────

    // Parse query_definition early so we can gate on report type without
    // persisting a run row that will immediately fail authorisation.
    let query_definition: ReportQueryDefinition =
        serde_json::from_str(&report_row.query_definition).map_err(|e| {
            AppError::Internal(format!("failed to parse query_definition JSON: {}", e))
        })?;

    // SECURITY-GATE: AuditRead required for AuditSummary — do not remove.
    // Audit-summary reports expose audit log rows.  Require AuditRead so this
    // execution path cannot be used to bypass the dedicated audit endpoint gate.
    // In the current RBAC matrix only Admin satisfies both ReportExecute and
    // AuditRead; Librarian and DepartmentHead are denied here.
    // Regression covered by: db_non_audit_role_denied_for_audit_summary_report (api_routes_test)
    if query_definition.report_type == ReportType::AuditSummary {
        require(principal, Capability::AuditRead)?;
    }

    // JournalCatalog and ResourceCatalog do not have a direct `department_id`
    // column on their primary tables.  Instead they scope by the
    // creator/owner's department via an INNER JOIN on `users`.  Three cases:
    //
    //   ScopeFilter::All               → Admin/Librarian; unrestricted.
    //   ScopeFilter::Department(_) or
    //   ScopeFilter::DepartmentOrOwned → scoped INNER JOIN at generation time;
    //                                    rows with NULL creator/owner or whose
    //                                    owner has no department are excluded
    //                                    (fail-closed).
    //   ScopeFilter::None / OwnedBy   → no department can be derived safely;
    //                                    deny upfront.
    let scope = content_scope(principal);
    if matches!(
        query_definition.report_type,
        ReportType::JournalCatalog | ReportType::ResourceCatalog
    ) && !matches!(
        scope,
        ScopeFilter::All | ScopeFilter::Department(_) | ScopeFilter::DepartmentOrOwned { .. }
    ) {
        return Err(AppError::Forbidden);
    }

    let run_id = Uuid::new_v4();
    let now = Utc::now().naive_utc();

    let run_row = ReportRunRow {
        id: run_id.to_string(),
        report_id: report_id.to_string(),
        triggered_by: Some(principal.user_id.to_string()),
        triggered_source: TriggeredSource::Manual.as_db().to_string(),
        format: effective_format.as_db().to_string(),
        status: RunStatus::Queued.as_db().to_string(),
        artifact_path: None,
        artifact_size_bytes: None,
        artifact_dek: None,
        error_message: None,
        started_at: None,
        completed_at: None,
        created_at: now,
    };

    report_repo::insert_run(pool, &run_row).await?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: actions::REPORT_RUN_TRIGGER,
            target_entity_type: Some("report_run"),
            target_entity_id: Some(run_id),
            change_payload: Some(serde_json::json!({
                "report_id": report_id,
                "format": effective_format.as_db(),
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    // Execute the report synchronously.
    // `scope` and `query_definition` were both resolved above before the run
    // row was persisted so that type-specific auth failures produce a clean 403.
    report_repo::update_run_started(pool, run_id).await?;

    match generate_report_data(
        pool,
        &query_definition,
        &scope,
        principal,
        reports_storage_path,
        run_id,
        effective_format,
        enc,
    )
    .await
    {
        Ok((artifact_filename, artifact_size)) => {
            report_repo::update_run_completed(pool, run_id, &artifact_filename, artifact_size)
                .await?;

            audit_service::record(
                pool,
                AuditEvent {
                    actor_id: Some(principal.user_id),
                    actor_email: Some(&principal.email),
                    action: actions::REPORT_RUN_COMPLETE,
                    target_entity_type: Some("report_run"),
                    target_entity_id: Some(run_id),
                    change_payload: Some(serde_json::json!({
                        "artifact": artifact_filename,
                        "size_bytes": artifact_size,
                    })),
                    ip_address: None,
                    user_agent: None,
                },
            )
            .await?;
        }
        Err(e) => {
            let msg = e.to_string();
            tracing::error!(run_id = %run_id, "report generation failed: {}", msg);
            report_repo::update_run_failed(pool, run_id, &msg).await?;

            audit_service::record(
                pool,
                AuditEvent {
                    actor_id: Some(principal.user_id),
                    actor_email: Some(&principal.email),
                    action: actions::REPORT_RUN_FAIL,
                    target_entity_type: Some("report_run"),
                    target_entity_id: Some(run_id),
                    change_payload: Some(serde_json::json!({ "error": msg })),
                    ip_address: None,
                    user_agent: None,
                },
            )
            .await?;
        }
    }

    let final_run = report_repo::find_run_by_id(pool, run_id).await?;
    row_to_run_view(&final_run, report_row.title, reports_storage_path)
}

/// Package-internal version used by the scheduler (no Principal, runs as system).
pub(crate) async fn trigger_run_internal(
    pool: &MySqlPool,
    schedule_row: &ReportScheduleRow,
    reports_storage_path: &str,
    enc: &FieldEncryption,
) -> AppResult<(Uuid, bool)> {
    let report_row = report_repo::find_by_id(
        pool,
        Uuid::parse_str(&schedule_row.report_id)
            .map_err(|e| AppError::Internal(format!("bad uuid: {}", e)))?,
    )
    .await?;

    let effective_format = ReportFormat::from_db(&schedule_row.format)
        .or_else(|| ReportFormat::from_db(&report_row.default_format))
        .unwrap_or(ReportFormat::Csv);

    let run_id = Uuid::new_v4();
    let now = Utc::now().naive_utc();

    let run_row = ReportRunRow {
        id: run_id.to_string(),
        report_id: schedule_row.report_id.clone(),
        triggered_by: None,
        triggered_source: TriggeredSource::Scheduled.as_db().to_string(),
        format: effective_format.as_db().to_string(),
        status: RunStatus::Queued.as_db().to_string(),
        artifact_path: None,
        artifact_size_bytes: None,
        artifact_dek: None,
        error_message: None,
        started_at: None,
        completed_at: None,
        created_at: now,
    };

    report_repo::insert_run(pool, &run_row).await?;
    report_repo::update_run_started(pool, run_id).await?;

    let query_definition: ReportQueryDefinition =
        serde_json::from_str(&report_row.query_definition).map_err(|e| {
            AppError::Internal(format!("failed to parse query_definition JSON: {}", e))
        })?;

    // For scheduled runs use All scope if no department_scope_id is set,
    // otherwise restrict to the configured department.
    let scope = if let Some(ref dept_str) = schedule_row.department_scope_id {
        if let Ok(dept_id) = Uuid::parse_str(dept_str) {
            ScopeFilter::Department(dept_id)
        } else {
            ScopeFilter::All
        }
    } else {
        ScopeFilter::All
    };

    // No principal for scheduled runs — use a sentinel that enables all output.
    let is_admin_context = true;

    match generate_report_data_with_admin_flag(
        pool,
        &query_definition,
        &scope,
        is_admin_context,
        reports_storage_path,
        run_id,
        effective_format,
        enc,
    )
    .await
    {
        Ok((artifact_filename, artifact_size)) => {
            report_repo::update_run_completed(pool, run_id, &artifact_filename, artifact_size)
                .await?;
            Ok((run_id, true))
        }
        Err(e) => {
            let msg = e.to_string();
            report_repo::update_run_failed(pool, run_id, &msg).await?;
            Ok((run_id, false))
        }
    }
}

pub async fn list_runs(
    pool: &MySqlPool,
    principal: &Principal,
    report_id: Uuid,
    reports_storage_path: &str,
) -> AppResult<Vec<ReportRunView>> {
    require(principal, Capability::ReportRead)?;
    let report_row = report_repo::find_by_id(pool, report_id).await?;
    // Enforce department scope on the parent report.
    let scope = content_scope(principal);
    let created_by_uuid = report_row.created_by.as_deref().and_then(|s| Uuid::parse_str(s).ok());
    let creator_dept = creator_department(pool, report_row.created_by.as_deref()).await?;
    require_object_visible(&scope, created_by_uuid, creator_dept)?;
    let rows = report_repo::find_runs(pool, report_id, 100).await?;
    rows.iter()
        .map(|r| row_to_run_view(r, report_row.title.clone(), reports_storage_path))
        .collect()
}

pub async fn get_run(
    pool: &MySqlPool,
    principal: &Principal,
    run_id: Uuid,
    reports_storage_path: &str,
) -> AppResult<ReportRunView> {
    require(principal, Capability::ReportRead)?;
    let run_row = report_repo::find_run_by_id(pool, run_id).await?;
    let report_id = Uuid::parse_str(&run_row.report_id)
        .map_err(|e| AppError::Internal(format!("bad uuid: {}", e)))?;
    let report_row = report_repo::find_by_id(pool, report_id).await?;
    // Enforce department scope on the parent report.
    let scope = content_scope(principal);
    let created_by_uuid = report_row.created_by.as_deref().and_then(|s| Uuid::parse_str(s).ok());
    let creator_dept = creator_department(pool, report_row.created_by.as_deref()).await?;
    require_object_visible(&scope, created_by_uuid, creator_dept)?;
    row_to_run_view(&run_row, report_row.title, reports_storage_path)
}

pub async fn download_artifact(
    pool: &MySqlPool,
    principal: &Principal,
    run_id: Uuid,
    reports_storage_path: &str,
    enc: &FieldEncryption,
) -> AppResult<(Vec<u8>, &'static str, String)> {
    require(principal, Capability::ReportRead)?;

    let run_row = report_repo::find_run_by_id(pool, run_id).await?;
    let report_id = Uuid::parse_str(&run_row.report_id)
        .map_err(|e| AppError::Internal(format!("bad uuid: {}", e)))?;
    let report_row = report_repo::find_by_id(pool, report_id).await?;

    // Enforce department scope: a DepartmentHead or Instructor may only
    // download artifacts from reports created within their department.
    let scope = content_scope(principal);
    let created_by_uuid = report_row.created_by.as_deref().and_then(|s| Uuid::parse_str(s).ok());
    let creator_dept = creator_department(pool, report_row.created_by.as_deref()).await?;
    require_object_visible(&scope, created_by_uuid, creator_dept)?;

    let artifact_path = run_row
        .artifact_path
        .as_ref()
        .ok_or_else(|| AppError::NotFound("artifact not yet available".to_string()))?;

    let full_path = format!("{}/{}", reports_storage_path, artifact_path);
    let raw_bytes = fs::read(&full_path)
        .await
        .map_err(|e| AppError::NotFound(format!("artifact file not found: {}", e)))?;

    // If a DEK is stored this artifact is encrypted; unwrap the DEK and decrypt.
    // Legacy artifacts (artifact_dek IS NULL) are returned as-is.
    let bytes = if let Some(ref wrapped_dek) = run_row.artifact_dek {
        let dek = artifact_crypto::unwrap_dek(enc, wrapped_dek)?;
        artifact_crypto::decrypt_artifact(&dek, &raw_bytes)?
    } else {
        raw_bytes
    };

    let format = ReportFormat::from_db(&run_row.format).unwrap_or(ReportFormat::Csv);
    let mime = format.mime_type();
    let safe_title: String = report_row
        .title
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let filename = format!(
        "report_{}_{}.{}",
        safe_title,
        run_row.id,
        format.extension()
    );

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: actions::REPORT_ARTIFACT_DOWNLOAD,
            target_entity_type: Some("report_run"),
            target_entity_id: Some(run_id),
            change_payload: Some(serde_json::json!({
                "report_id": report_id,
                "artifact": artifact_path,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    Ok((bytes, mime, filename))
}

pub async fn list_schedules(
    pool: &MySqlPool,
    principal: &Principal,
    report_id: Uuid,
) -> AppResult<Vec<ReportScheduleView>> {
    require(principal, Capability::ReportRead)?;
    let report_row = report_repo::find_by_id(pool, report_id).await?;
    // Enforce department scope on the parent report, consistent with
    // get_report, list_runs, get_run, and download_artifact.  Without this
    // check a department-scoped principal could enumerate schedule metadata
    // (cron expressions, is_active, format) for reports that belong to a
    // different department simply by knowing the report UUID.
    let scope = content_scope(principal);
    let created_by_uuid = report_row.created_by.as_deref().and_then(|s| Uuid::parse_str(s).ok());
    let creator_dept = creator_department(pool, report_row.created_by.as_deref()).await?;
    require_object_visible(&scope, created_by_uuid, creator_dept)?;
    let rows = report_repo::find_schedules(pool, report_id).await?;
    rows.iter()
        .map(|r| row_to_schedule_view(r, report_row.title.clone()))
        .collect()
}

pub async fn create_schedule(
    pool: &MySqlPool,
    principal: &Principal,
    report_id: Uuid,
    input: CreateScheduleInput,
) -> AppResult<ReportScheduleView> {
    require(principal, Capability::ReportManage)?;

    // Validate report exists.
    let report_row = report_repo::find_by_id(pool, report_id).await?;

    // SECURITY-GATE: AuditRead required for AuditSummary schedules — do not remove.
    // Propagate the AuditRead gate to schedule creation: a principal without
    // AuditRead cannot schedule an audit-summary report because the resulting
    // scheduled runs would expose audit data they are not permitted to read.
    // Regression covered by: db_non_audit_role_denied_for_audit_summary_report (api_routes_test)
    {
        let sched_qd: ReportQueryDefinition =
            serde_json::from_str(&report_row.query_definition)
                .map_err(|e| AppError::Internal(format!("bad query_definition: {}", e)))?;
        if sched_qd.report_type == ReportType::AuditSummary {
            require(principal, Capability::AuditRead)?;
        }
    }

    let next_run_at = compute_next_run(&input.cron_expression)?;
    let now = Utc::now().naive_utc();
    let schedule_id = Uuid::new_v4();
    let is_active = input.is_active.unwrap_or(true);
    let format = input
        .format
        .or_else(|| ReportFormat::from_db(&report_row.default_format))
        .unwrap_or(ReportFormat::Csv);

    let row = ReportScheduleRow {
        id: schedule_id.to_string(),
        report_id: report_id.to_string(),
        cron_expression: input.cron_expression.clone(),
        department_scope_id: input.department_scope_id.map(|u| u.to_string()),
        is_active: if is_active { 1 } else { 0 },
        format: format.as_db().to_string(),
        last_run_at: None,
        next_run_at,
        created_by: Some(principal.user_id.to_string()),
        created_at: now,
        updated_at: now,
    };

    report_repo::insert_schedule(pool, &row).await?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: actions::REPORT_SCHEDULE_CREATE,
            target_entity_type: Some("report_schedule"),
            target_entity_id: Some(schedule_id),
            change_payload: Some(serde_json::json!({
                "report_id": report_id,
                "cron_expression": input.cron_expression,
                "is_active": is_active,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    let created = report_repo::find_schedule_by_id(pool, schedule_id).await?;
    row_to_schedule_view(&created, report_row.title)
}

pub async fn update_schedule(
    pool: &MySqlPool,
    principal: &Principal,
    schedule_id: Uuid,
    input: UpdateScheduleInput,
) -> AppResult<ReportScheduleView> {
    require(principal, Capability::ReportManage)?;

    let existing = report_repo::find_schedule_by_id(pool, schedule_id).await?;
    let report_id = Uuid::parse_str(&existing.report_id)
        .map_err(|e| AppError::Internal(format!("bad uuid: {}", e)))?;
    let report_row = report_repo::find_by_id(pool, report_id).await?;

    let new_cron = input
        .cron_expression
        .as_deref()
        .unwrap_or(&existing.cron_expression);
    let new_is_active = input.is_active.unwrap_or(existing.is_active != 0);
    let new_format = input
        .format
        .map(|f| f.as_db().to_string())
        .unwrap_or(existing.format.clone());
    let new_dept_scope = match input.department_scope_id {
        Some(id) => Some(id),
        None => existing
            .department_scope_id
            .as_deref()
            .map(|s| Uuid::parse_str(s))
            .transpose()
            .map_err(|e| AppError::Internal(format!("bad uuid: {}", e)))?,
    };

    // Recompute next_run_at if cron changed or schedule toggled active.
    let new_next_run = if input.cron_expression.is_some() || input.is_active.is_some() {
        if new_is_active {
            compute_next_run(new_cron)?
        } else {
            None
        }
    } else {
        existing.next_run_at
    };

    report_repo::update_schedule(
        pool,
        schedule_id,
        new_cron,
        new_is_active,
        &new_format,
        new_dept_scope,
        new_next_run,
    )
    .await?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: actions::REPORT_SCHEDULE_UPDATE,
            target_entity_type: Some("report_schedule"),
            target_entity_id: Some(schedule_id),
            change_payload: Some(serde_json::json!({
                "cron_expression": new_cron,
                "is_active": new_is_active,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    let updated = report_repo::find_schedule_by_id(pool, schedule_id).await?;
    row_to_schedule_view(&updated, report_row.title)
}

pub async fn delete_schedule(
    pool: &MySqlPool,
    principal: &Principal,
    schedule_id: Uuid,
) -> AppResult<()> {
    require(principal, Capability::ReportManage)?;

    let existing = report_repo::find_schedule_by_id(pool, schedule_id).await?;

    report_repo::delete_schedule(pool, schedule_id).await?;

    audit_service::record(
        pool,
        AuditEvent {
            actor_id: Some(principal.user_id),
            actor_email: Some(&principal.email),
            action: actions::REPORT_SCHEDULE_DELETE,
            target_entity_type: Some("report_schedule"),
            target_entity_id: Some(schedule_id),
            change_payload: Some(serde_json::json!({
                "report_id": existing.report_id,
            })),
            ip_address: None,
            user_agent: None,
        },
    )
    .await?;

    Ok(())
}

// ─── Report generation ────────────────────────────────────────────────────────

/// Entry point from `trigger_run` — extracts admin flag from Principal.
async fn generate_report_data(
    pool: &MySqlPool,
    query_def: &ReportQueryDefinition,
    scope: &ScopeFilter,
    principal: &Principal,
    reports_storage_path: &str,
    run_id: Uuid,
    format: ReportFormat,
    enc: &FieldEncryption,
) -> AppResult<(String, i64)> {
    let is_privileged = principal.is_admin()
        || principal.has_role(Role::Librarian)
        || principal.has_role(Role::DepartmentHead);
    generate_report_data_with_admin_flag(
        pool,
        query_def,
        scope,
        is_privileged,
        reports_storage_path,
        run_id,
        format,
        enc,
    )
    .await
}

/// Core report generation — executes the appropriate SQL, assembles the
/// output, writes the artifact file, encrypts it in-place, and stores the
/// wrapped DEK.
///
/// Returns `(relative_filename, size_bytes)`.
async fn generate_report_data_with_admin_flag(
    pool: &MySqlPool,
    query_def: &ReportQueryDefinition,
    scope: &ScopeFilter,
    is_privileged: bool,
    reports_storage_path: &str,
    run_id: Uuid,
    format: ReportFormat,
    enc: &FieldEncryption,
) -> AppResult<(String, i64)> {
    fs::create_dir_all(reports_storage_path)
        .await
        .map_err(|e| AppError::Internal(format!("create reports dir: {}", e)))?;

    // Determine the effective department filter.
    // Non-privileged callers can only filter within their own scoped department.
    let effective_dept_filter: Option<Uuid> = {
        let requested = query_def.filters.department_id;
        match scope {
            ScopeFilter::All => requested,
            ScopeFilter::Department(d) | ScopeFilter::DepartmentOrOwned { department_id: d, .. } => {
                // If a specific dept was requested, only honour it if it matches the caller's dept.
                if let Some(req_dept) = requested {
                    if req_dept == *d {
                        Some(req_dept)
                    } else {
                        // Restrict to the caller's own dept regardless of what was requested.
                        Some(*d)
                    }
                } else {
                    Some(*d)
                }
            }
            _ => requested,
        }
    };

    let filters = &query_def.filters;

    let rows_data: Vec<Vec<String>> = match &query_def.report_type {
        ReportType::JournalCatalog => {
            generate_journal_catalog(pool, effective_dept_filter, filters).await?
        }
        ReportType::ResourceCatalog => {
            generate_resource_catalog(pool, effective_dept_filter, filters).await?
        }
        ReportType::CourseCatalog => {
            generate_course_catalog(pool, effective_dept_filter, filters).await?
        }
        ReportType::CheckinActivity => {
            generate_checkin_activity(pool, effective_dept_filter, filters).await?
        }
        ReportType::AuditSummary => {
            generate_audit_summary(pool, filters).await?
        }
        ReportType::SectionRoster => {
            generate_section_roster(pool, effective_dept_filter, filters, is_privileged).await?
        }
    };

    let headers = report_headers(&query_def.report_type);
    let filename = format!("{}.{}", run_id, format.extension());
    let full_path = format!("{}/{}", reports_storage_path, filename);

    let bytes_written = match format {
        ReportFormat::Csv => write_csv(&full_path, &headers, &rows_data).await?,
        ReportFormat::Xlsx => write_xlsx(&full_path, &headers, &rows_data).await?,
    };

    // ── Envelope encryption ───────────────────────────────────────────────────
    // Generate a per-artifact DEK, encrypt the file in-place, and store the
    // wrapped DEK in `report_runs.artifact_dek`.  On retention expiry, NULLing
    // that column is sufficient for cryptographic erasure even on OverlayFS.
    let dek = artifact_crypto::generate_dek();
    let plaintext = fs::read(&full_path)
        .await
        .map_err(|e| AppError::Internal(format!("artifact encrypt: read plaintext: {}", e)))?;
    let ciphertext = artifact_crypto::encrypt_artifact(&dek, &plaintext)?;
    fs::write(&full_path, &ciphertext)
        .await
        .map_err(|e| AppError::Internal(format!("artifact encrypt: write ciphertext: {}", e)))?;
    let wrapped_dek = artifact_crypto::wrap_dek(enc, &dek)?;
    report_repo::update_run_artifact_dek(pool, run_id, &wrapped_dek).await?;

    Ok((filename, bytes_written))
}

fn report_headers(report_type: &ReportType) -> Vec<&'static str> {
    match report_type {
        ReportType::JournalCatalog => {
            vec!["journal_id", "title", "status", "created_at"]
        }
        ReportType::ResourceCatalog => {
            vec!["resource_id", "title", "resource_type", "status", "created_at"]
        }
        ReportType::CourseCatalog => {
            vec!["course_code", "title", "department_name", "credit_hours", "status", "section_count", "created_at"]
        }
        ReportType::CheckinActivity => {
            vec!["section_code", "course_code", "term", "year", "department_name", "total_checkins", "unique_users"]
        }
        ReportType::AuditSummary => {
            vec!["action", "count", "first_seen", "last_seen"]
        }
        ReportType::SectionRoster => {
            vec!["section_code", "course_code", "term", "year", "instructor_email", "instructor_name", "capacity", "department_name", "status"]
        }
    }
}

// ─── Individual report queries ────────────────────────────────────────────────

async fn generate_journal_catalog(
    pool: &MySqlPool,
    dept: Option<Uuid>,
    filters: &ReportFilters,
) -> AppResult<Vec<Vec<String>>> {
    // Department scoping is achieved via the journal creator's user record.
    // When `dept` is Some we use an INNER JOIN so that:
    //   • journals with NULL created_by are excluded (fail-closed)
    //   • journals whose creator has no department_id are excluded (fail-closed)
    //   • only rows belonging to the requested department are included
    // When `dept` is None (Admin/Librarian running an unrestricted report)
    // no join is added and the full catalog is returned.
    let (dept_join, dept_where) = if dept.is_some() {
        (
            "JOIN users u ON j.author_id = u.id",
            " AND u.department_id = ?",
        )
    } else {
        ("", "")
    };

    let mut sql = format!(
        r#"
        SELECT j.id, j.title,
               COALESCE(jv.state, 'draft') AS status,
               j.created_at
          FROM journals j
          {dept_join}
          LEFT JOIN journal_versions jv ON j.latest_version_id = jv.id
         WHERE 1=1{dept_where}
        "#,
    );

    if filters.status_filter.is_some() {
        sql.push_str(" AND COALESCE(jv.state, 'draft') = ?");
    }
    sql.push_str(" ORDER BY j.created_at DESC");

    let mut q = sqlx::query(&sql);
    if let Some(d) = dept {
        q = q.bind(d.to_string());
    }
    if let Some(ref s) = filters.status_filter {
        q = q.bind(s.clone());
    }

    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("journal_catalog query: {}", e)))?;

    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row.try_get("id").map_err(|e| AppError::Database(e.to_string()))?;
        let title: String = row.try_get("title").map_err(|e| AppError::Database(e.to_string()))?;
        let status: String = row.try_get("status").map_err(|e| AppError::Database(e.to_string()))?;
        let created_at: NaiveDateTime = row.try_get("created_at").map_err(|e| AppError::Database(e.to_string()))?;

        result.push(vec![id, title, status, created_at.to_string()]);
    }
    Ok(result)
}

async fn generate_resource_catalog(
    pool: &MySqlPool,
    dept: Option<Uuid>,
    filters: &ReportFilters,
) -> AppResult<Vec<Vec<String>>> {
    // Department scoping is achieved via the resource owner's user record.
    // When `dept` is Some we use an INNER JOIN so that:
    //   • resources with NULL owner_id are excluded (fail-closed)
    //   • resources whose owner has no department_id are excluded (fail-closed)
    //   • only rows belonging to the requested department are included
    // When `dept` is None (Admin/Librarian running an unrestricted report)
    // no join is added and the full catalog is returned.
    let (dept_join, dept_where) = if dept.is_some() {
        (
            "JOIN users u ON r.owner_id = u.id",
            " AND u.department_id = ?",
        )
    } else {
        ("", "")
    };

    let mut sql = format!(
        r#"
        SELECT r.id, r.title, r.resource_type,
               COALESCE(rv.state, 'draft') AS status, r.created_at
          FROM teaching_resources r
          {dept_join}
          LEFT JOIN resource_versions rv ON r.latest_version_id = rv.id
         WHERE 1=1{dept_where}
        "#,
    );

    if filters.status_filter.is_some() {
        sql.push_str(" AND COALESCE(rv.state, 'draft') = ?");
    }
    sql.push_str(" ORDER BY r.created_at DESC");

    let mut q = sqlx::query(&sql);
    if let Some(d) = dept {
        q = q.bind(d.to_string());
    }
    if let Some(ref s) = filters.status_filter {
        q = q.bind(s.clone());
    }

    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("resource_catalog query: {}", e)))?;

    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let id: String = row.try_get("id").map_err(|e| AppError::Database(e.to_string()))?;
        let title: Option<String> = row.try_get("title").map_err(|e| AppError::Database(e.to_string()))?;
        let resource_type: String = row.try_get("resource_type").map_err(|e| AppError::Database(e.to_string()))?;
        let status: String = row.try_get("status").map_err(|e| AppError::Database(e.to_string()))?;
        let created_at: NaiveDateTime = row.try_get("created_at").map_err(|e| AppError::Database(e.to_string()))?;

        result.push(vec![
            id,
            title.unwrap_or_default(),
            resource_type,
            status,
            created_at.to_string(),
        ]);
    }
    Ok(result)
}

async fn generate_course_catalog(
    pool: &MySqlPool,
    dept: Option<Uuid>,
    filters: &ReportFilters,
) -> AppResult<Vec<Vec<String>>> {
    let mut sql = String::from(
        r#"
        SELECT c.code AS course_code, c.title, d.name AS department_name,
               cv.credit_hours, COALESCE(cv.state, 'draft') AS status,
               COUNT(s.id) AS section_count, c.created_at
          FROM courses c
          LEFT JOIN departments d ON c.department_id = d.id
          LEFT JOIN course_versions cv ON c.latest_version_id = cv.id
          LEFT JOIN sections s ON s.course_id = c.id AND s.is_active = 1
         WHERE 1=1
        "#,
    );

    if dept.is_some() {
        sql.push_str(" AND c.department_id = ?");
    }
    if filters.status_filter.is_some() {
        sql.push_str(" AND COALESCE(cv.state, 'draft') = ?");
    }
    sql.push_str(
        " GROUP BY c.id, c.code, c.title, d.name, cv.credit_hours, cv.state, c.created_at ORDER BY c.code",
    );

    let mut q = sqlx::query(&sql);
    if let Some(d) = dept {
        q = q.bind(d.to_string());
    }
    if let Some(ref s) = filters.status_filter {
        q = q.bind(s.clone());
    }

    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("course_catalog query: {}", e)))?;

    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let course_code: String = row.try_get("course_code").map_err(|e| AppError::Database(e.to_string()))?;
        let title: String = row.try_get("title").map_err(|e| AppError::Database(e.to_string()))?;
        let dept_name: Option<String> = row.try_get("department_name").map_err(|e| AppError::Database(e.to_string()))?;
        let credit_hours: Option<f32> = row.try_get("credit_hours").map_err(|e| AppError::Database(e.to_string()))?;
        let status: String = row.try_get("status").map_err(|e| AppError::Database(e.to_string()))?;
        let section_count: i64 = row.try_get("section_count").map_err(|e| AppError::Database(e.to_string()))?;
        let created_at: NaiveDateTime = row.try_get("created_at").map_err(|e| AppError::Database(e.to_string()))?;

        result.push(vec![
            course_code,
            title,
            dept_name.unwrap_or_default(),
            credit_hours.map(|v| v.to_string()).unwrap_or_default(),
            status,
            section_count.to_string(),
            created_at.to_string(),
        ]);
    }
    Ok(result)
}

async fn generate_checkin_activity(
    pool: &MySqlPool,
    dept: Option<Uuid>,
    filters: &ReportFilters,
) -> AppResult<Vec<Vec<String>>> {
    let mut sql = String::from(
        r#"
        SELECT s.section_code, c.course_code, s.term, s.year,
               d.name AS department_name,
               COUNT(ce.id) AS total_checkins,
               COUNT(DISTINCT ce.user_id) AS unique_users
          FROM checkin_events ce
          JOIN sections s ON ce.section_id = s.id
          JOIN courses c ON s.course_id = c.id
          LEFT JOIN departments d ON c.department_id = d.id
         WHERE ce.is_duplicate_attempt = 0
        "#,
    );

    if dept.is_some() {
        sql.push_str(" AND c.department_id = ?");
    }
    if filters.date_from.is_some() {
        sql.push_str(" AND ce.created_at >= ?");
    }
    if filters.date_to.is_some() {
        sql.push_str(" AND ce.created_at <= ?");
    }
    sql.push_str(
        " GROUP BY s.id, s.section_code, c.course_code, s.term, s.year, d.name ORDER BY total_checkins DESC",
    );

    let mut q = sqlx::query(&sql);
    if let Some(d) = dept {
        q = q.bind(d.to_string());
    }
    if let Some(ref df) = filters.date_from {
        q = q.bind(df.clone());
    }
    if let Some(ref dt) = filters.date_to {
        q = q.bind(dt.clone());
    }

    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("checkin_activity query: {}", e)))?;

    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let section_code: String = row.try_get("section_code").map_err(|e| AppError::Database(e.to_string()))?;
        let course_code: String = row.try_get("course_code").map_err(|e| AppError::Database(e.to_string()))?;
        let term: String = row.try_get("term").map_err(|e| AppError::Database(e.to_string()))?;
        let year: i32 = row.try_get("year").map_err(|e| AppError::Database(e.to_string()))?;
        let dept_name: Option<String> = row.try_get("department_name").map_err(|e| AppError::Database(e.to_string()))?;
        let total_checkins: i64 = row.try_get("total_checkins").map_err(|e| AppError::Database(e.to_string()))?;
        let unique_users: i64 = row.try_get("unique_users").map_err(|e| AppError::Database(e.to_string()))?;

        result.push(vec![
            section_code,
            course_code,
            term,
            year.to_string(),
            dept_name.unwrap_or_default(),
            total_checkins.to_string(),
            unique_users.to_string(),
        ]);
    }
    Ok(result)
}

async fn generate_audit_summary(
    pool: &MySqlPool,
    filters: &ReportFilters,
) -> AppResult<Vec<Vec<String>>> {
    let mut sql = String::from(
        r#"
        SELECT action, COUNT(*) AS count,
               MIN(created_at) AS first_seen, MAX(created_at) AS last_seen
          FROM audit_logs
         WHERE 1=1
        "#,
    );

    if filters.date_from.is_some() {
        sql.push_str(" AND created_at >= ?");
    }
    if filters.date_to.is_some() {
        sql.push_str(" AND created_at <= ?");
    }
    sql.push_str(" GROUP BY action ORDER BY count DESC");

    let mut q = sqlx::query(&sql);
    if let Some(ref df) = filters.date_from {
        q = q.bind(df.clone());
    }
    if let Some(ref dt) = filters.date_to {
        q = q.bind(dt.clone());
    }

    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("audit_summary query: {}", e)))?;

    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let action: String = row.try_get("action").map_err(|e| AppError::Database(e.to_string()))?;
        let count: i64 = row.try_get("count").map_err(|e| AppError::Database(e.to_string()))?;
        let first_seen: NaiveDateTime = row.try_get("first_seen").map_err(|e| AppError::Database(e.to_string()))?;
        let last_seen: NaiveDateTime = row.try_get("last_seen").map_err(|e| AppError::Database(e.to_string()))?;

        result.push(vec![
            action,
            count.to_string(),
            first_seen.to_string(),
            last_seen.to_string(),
        ]);
    }
    Ok(result)
}

async fn generate_section_roster(
    pool: &MySqlPool,
    dept: Option<Uuid>,
    filters: &ReportFilters,
    is_privileged: bool,
) -> AppResult<Vec<Vec<String>>> {
    let mut sql = String::from(
        r#"
        SELECT s.section_code, c.code AS course_code, s.term, s.year,
               u.email AS instructor_email, u.display_name AS instructor_name,
               s.capacity, d.name AS department_name,
               COALESCE(sv.state, 'draft') AS status
          FROM sections s
          JOIN courses c ON s.course_id = c.id
          LEFT JOIN users u ON s.instructor_id = u.id
          LEFT JOIN departments d ON c.department_id = d.id
          LEFT JOIN section_versions sv ON s.latest_version_id = sv.id
         WHERE s.is_active = 1
        "#,
    );

    if dept.is_some() {
        sql.push_str(" AND c.department_id = ?");
    }
    if filters.status_filter.is_some() {
        sql.push_str(" AND COALESCE(sv.state, 'draft') = ?");
    }
    sql.push_str(" ORDER BY c.course_code, s.section_code");

    let mut q = sqlx::query(&sql);
    if let Some(d) = dept {
        q = q.bind(d.to_string());
    }
    if let Some(ref s) = filters.status_filter {
        q = q.bind(s.clone());
    }

    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| AppError::Database(format!("section_roster query: {}", e)))?;

    let mut result = Vec::with_capacity(rows.len());
    for row in rows {
        let section_code: String = row.try_get("section_code").map_err(|e| AppError::Database(e.to_string()))?;
        let course_code: String = row.try_get("course_code").map_err(|e| AppError::Database(e.to_string()))?;
        let term: String = row.try_get("term").map_err(|e| AppError::Database(e.to_string()))?;
        let year: i32 = row.try_get("year").map_err(|e| AppError::Database(e.to_string()))?;
        let instructor_email: Option<String> = row.try_get("instructor_email").map_err(|e| AppError::Database(e.to_string()))?;
        let instructor_name: Option<String> = row.try_get("instructor_name").map_err(|e| AppError::Database(e.to_string()))?;
        let capacity: Option<i32> = row.try_get("capacity").map_err(|e| AppError::Database(e.to_string()))?;
        let dept_name: Option<String> = row.try_get("department_name").map_err(|e| AppError::Database(e.to_string()))?;
        let status: String = row.try_get("status").map_err(|e| AppError::Database(e.to_string()))?;

        // Mask sensitive fields for non-privileged callers.
        let (email_out, name_out) = if is_privileged {
            (
                instructor_email.unwrap_or_default(),
                instructor_name.unwrap_or_default(),
            )
        } else {
            (
                "[REDACTED]".to_string(),
                "[REDACTED]".to_string(),
            )
        };

        result.push(vec![
            section_code,
            course_code,
            term,
            year.to_string(),
            email_out,
            name_out,
            capacity.map(|v| v.to_string()).unwrap_or_default(),
            dept_name.unwrap_or_default(),
            status,
        ]);
    }
    Ok(result)
}

// ─── File writers ─────────────────────────────────────────────────────────────

async fn write_csv(
    path: &str,
    headers: &[&str],
    rows: &[Vec<String>],
) -> AppResult<i64> {
    let mut wtr = csv::WriterBuilder::new()
        .has_headers(true)
        .from_path(path)
        .map_err(|e| AppError::Internal(format!("csv writer create: {}", e)))?;

    wtr.write_record(headers)
        .map_err(|e| AppError::Internal(format!("csv write headers: {}", e)))?;

    for row in rows {
        wtr.write_record(row)
            .map_err(|e| AppError::Internal(format!("csv write row: {}", e)))?;
    }

    wtr.flush()
        .map_err(|e| AppError::Internal(format!("csv flush: {}", e)))?;

    let meta = std::fs::metadata(path)
        .map_err(|e| AppError::Internal(format!("csv stat: {}", e)))?;
    Ok(meta.len() as i64)
}

async fn write_xlsx(
    path: &str,
    headers: &[&str],
    rows: &[Vec<String>],
) -> AppResult<i64> {
    use rust_xlsxwriter::Workbook;

    let mut workbook = Workbook::new();
    let worksheet = workbook.add_worksheet();

    // Write header row
    for (col, header) in headers.iter().enumerate() {
        worksheet
            .write_string(0, col as u16, *header)
            .map_err(|e| AppError::Internal(format!("xlsx write header: {}", e)))?;
    }

    // Write data rows
    for (row_idx, row) in rows.iter().enumerate() {
        for (col, cell) in row.iter().enumerate() {
            worksheet
                .write_string((row_idx + 1) as u32, col as u16, cell.as_str())
                .map_err(|e| AppError::Internal(format!("xlsx write cell: {}", e)))?;
        }
    }

    workbook
        .save(path)
        .map_err(|e| AppError::Internal(format!("xlsx save: {}", e)))?;

    let meta = std::fs::metadata(path)
        .map_err(|e| AppError::Internal(format!("xlsx stat: {}", e)))?;
    Ok(meta.len() as i64)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::scope::require_object_visible;

    // ── Helpers ──────────────────────────────────────────────────────────────

    fn principal_with_scope(roles: Vec<Role>, department_id: Option<Uuid>) -> Principal {
        Principal {
            user_id: Uuid::new_v4(),
            session_id: Uuid::new_v4(),
            email: "test@scholarly.local".into(),
            display_name: "Tester".into(),
            roles,
            department_id,
        }
    }

    // ── list_schedules scope enforcement (pure logic) ─────────────────────────
    //
    // list_schedules applies `content_scope` + `require_object_visible` on the
    // parent report row before returning schedule rows.  The tests below
    // exercise the exact call sequence the fixed function uses, so a regression
    // that removes the check would cause these assertions to flip.

    /// Admin principal → ScopeFilter::All → always visible.
    #[test]
    fn list_schedules_scope_admin_sees_any_report() {
        let admin = principal_with_scope(vec![Role::Admin], None);
        let scope = content_scope(&admin);

        // Report created by someone in a completely different department.
        let other_dept = Some(Uuid::new_v4());
        let other_owner = Some(Uuid::new_v4());

        assert!(
            require_object_visible(&scope, other_owner, other_dept).is_ok(),
            "admin scope (All) must pass require_object_visible for any report"
        );
    }

    /// DepartmentHead whose department MATCHES the report creator's department
    /// must be granted access (same-department in-scope read).
    #[test]
    fn list_schedules_scope_dept_head_same_dept_is_visible() {
        let dept_id = Uuid::new_v4();
        let depthead = principal_with_scope(vec![Role::DepartmentHead], Some(dept_id));
        let scope = content_scope(&depthead);

        // Report was created by a user from the same department.
        let creator_dept = Some(dept_id);
        let creator_id = Some(Uuid::new_v4());

        assert!(
            require_object_visible(&scope, creator_id, creator_dept).is_ok(),
            "dept-head must see reports from their own department"
        );
    }

    /// DepartmentHead whose department does NOT match the report creator's
    /// department must be denied — this is the cross-department leakage case
    /// that the fix closes for list_schedules.
    #[test]
    fn list_schedules_scope_dept_head_other_dept_is_forbidden() {
        let own_dept = Uuid::new_v4();
        let depthead = principal_with_scope(vec![Role::DepartmentHead], Some(own_dept));
        let scope = content_scope(&depthead);

        // Report was created by a user in a *different* department.
        let other_dept = Some(Uuid::new_v4());
        let creator_id = Some(Uuid::new_v4());

        assert!(
            require_object_visible(&scope, creator_id, other_dept).is_err(),
            "dept-head must NOT see reports from a different department (schedule metadata leak)"
        );
    }

    /// Report created by an admin (NULL department) — a DepartmentHead must be
    /// denied because their scope is Department(X) and the creator has no dept.
    #[test]
    fn list_schedules_scope_dept_head_cannot_see_admin_owned_report() {
        let own_dept = Uuid::new_v4();
        let depthead = principal_with_scope(vec![Role::DepartmentHead], Some(own_dept));
        let scope = content_scope(&depthead);

        // Admin-created report: creator dept = None.
        let admin_owner = Some(Uuid::new_v4());
        let no_dept: Option<Uuid> = None;

        assert!(
            require_object_visible(&scope, admin_owner, no_dept).is_err(),
            "dept-head must not access admin-owned report schedules (null dept mismatch)"
        );
    }

    #[test]
    fn compute_next_run_valid_cron_returns_future_date() {
        // "every Monday at 07:00 UTC" — 7-field cron (sec min hour dom month dow year)
        let result = compute_next_run("0 0 7 * * Mon *");
        assert!(result.is_ok(), "valid cron should parse without error");
        let next = result.unwrap();
        assert!(next.is_some(), "a repeating schedule should always have a next run");
        let next_dt = next.unwrap();
        // The computed next-run must be in the future (strictly after now).
        let now = chrono::Utc::now().naive_utc();
        assert!(
            next_dt > now,
            "next_run_at ({}) should be after now ({})",
            next_dt,
            now
        );
    }

    #[test]
    fn compute_next_run_invalid_cron_returns_error() {
        let result = compute_next_run("not a cron");
        assert!(result.is_err(), "invalid cron expression should return Err");

        let result2 = compute_next_run("* * *");
        assert!(result2.is_err(), "too few cron fields should return Err");
    }

    #[test]
    fn compute_next_run_midnight_daily() {
        // "every day at midnight UTC"
        let result = compute_next_run("0 0 0 * * * *");
        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[test]
    fn report_format_db_round_trip() {
        use crate::domain::report::ReportFormat;
        for fmt in [ReportFormat::Csv, ReportFormat::Xlsx] {
            let db_str = fmt.as_db();
            let recovered = ReportFormat::from_db(db_str);
            assert_eq!(recovered, Some(fmt), "round-trip failed for {:?}", fmt);
        }
    }

    #[test]
    fn report_format_legacy_db_values_degrade_gracefully() {
        use crate::domain::report::ReportFormat;
        // Legacy values that existed in the pre-018 DB enum must not return None;
        // they degrade to the nearest supported format so reads never fail.
        assert_eq!(ReportFormat::from_db("pdf"),   Some(ReportFormat::Csv));
        assert_eq!(ReportFormat::from_db("html"),  Some(ReportFormat::Csv));
        assert_eq!(ReportFormat::from_db("json"),  Some(ReportFormat::Csv));
        assert_eq!(ReportFormat::from_db("excel"), Some(ReportFormat::Xlsx));
        // Truly unknown values must still return None.
        assert!(ReportFormat::from_db("").is_none());
        assert!(ReportFormat::from_db("docx").is_none());
    }
}
