//! Typed wrappers for the Phase 6 reporting endpoints.
//!
//! Covers report definitions, run history, artifact download, and
//! schedules. The backend enforces authorization; the frontend simply
//! calls through and surfaces any error via [`ApiError`].

use serde::{Deserialize, Serialize};

use crate::api::client::{ApiClient, ApiError, API_BASE};

// ── Wire types ───────────────────────────────────────────────────────────────

/// Query filters embedded inside a report definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReportQueryFilters {
    #[serde(default)]
    pub department_id: Option<String>,
    #[serde(default)]
    pub date_from: Option<String>,
    #[serde(default)]
    pub date_to: Option<String>,
    #[serde(default)]
    pub status_filter: Option<String>,
}

/// The `query_definition` sub-object inside a report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReportQueryDefinition {
    pub report_type: String,
    pub filters: ReportQueryFilters,
}

/// A report definition as returned by the backend.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ReportView {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub description: Option<String>,
    pub query_definition: ReportQueryDefinition,
    pub default_format: String,
    #[serde(default)]
    pub created_by: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// A single report run as returned by the backend.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ReportRunView {
    pub id: String,
    pub report_id: String,
    pub report_title: String,
    #[serde(default)]
    pub triggered_by: Option<String>,
    pub triggered_source: String,
    pub format: String,
    pub status: String,
    #[serde(default)]
    pub artifact_available: bool,
    #[serde(default)]
    pub artifact_size_bytes: Option<i64>,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    pub created_at: String,
}

/// A report schedule as returned by the backend.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ReportScheduleView {
    pub id: String,
    pub report_id: String,
    pub report_title: String,
    pub cron_expression: String,
    #[serde(default)]
    pub department_scope_id: Option<String>,
    pub is_active: bool,
    pub format: String,
    #[serde(default)]
    pub last_run_at: Option<String>,
    #[serde(default)]
    pub next_run_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ── Input types ──────────────────────────────────────────────────────────────

/// Body accepted by `POST /reports`.
#[derive(Debug, Clone, Serialize)]
pub struct CreateReportInput {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub query_definition: ReportQueryDefinition,
    pub default_format: String,
}

/// Body accepted by `POST /reports/<id>/runs`.
#[derive(Debug, Clone, Serialize)]
pub struct TriggerRunInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
}

/// Body accepted by `POST /reports/<id>/schedules`.
#[derive(Debug, Clone, Serialize)]
pub struct CreateScheduleInput {
    pub cron_expression: String,
    pub format: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub department_scope_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_active: Option<bool>,
}

// ── Helper ───────────────────────────────────────────────────────────────────

fn client(token: &str) -> ApiClient {
    ApiClient::new(Some(token.to_string()))
}

// ── Report definitions ───────────────────────────────────────────────────────

/// Lists all report definitions.
pub async fn list_reports(token: &str) -> Result<Vec<ReportView>, ApiError> {
    client(token).get_json("/reports").await
}

/// Fetches a single report definition by id.
pub async fn get_report(token: &str, id: &str) -> Result<ReportView, ApiError> {
    let path = format!("/reports/{}", id);
    client(token).get_json(&path).await
}

/// Creates a new report definition.
pub async fn create_report(
    token: &str,
    input: &CreateReportInput,
) -> Result<ReportView, ApiError> {
    client(token).post_json("/reports", input).await
}

// ── Runs ─────────────────────────────────────────────────────────────────────

/// Triggers a new run of the given report. If `format` is `None` the
/// backend uses the report's `default_format`.
pub async fn trigger_run(
    token: &str,
    report_id: &str,
    format: Option<&str>,
) -> Result<ReportRunView, ApiError> {
    let path = format!("/reports/{}/runs", report_id);
    let body = TriggerRunInput {
        format: format.map(|s| s.to_string()),
    };
    client(token).post_json(&path, &body).await
}

/// Lists all runs for a report (most recent first).
pub async fn list_runs(token: &str, report_id: &str) -> Result<Vec<ReportRunView>, ApiError> {
    let path = format!("/reports/{}/runs", report_id);
    client(token).get_json(&path).await
}

/// Lists the most recent runs across all reports.
pub async fn list_all_runs(token: &str) -> Result<Vec<ReportRunView>, ApiError> {
    client(token).get_json("/reports/runs").await
}

/// Fetches a single run by id.
pub async fn get_run(token: &str, run_id: &str) -> Result<ReportRunView, ApiError> {
    let path = format!("/reports/runs/{}", run_id);
    client(token).get_json(&path).await
}

/// Returns the URL from which a run's artifact can be downloaded.
/// Because `<a href>` cannot send an `Authorization` header, callers
/// should use [`download_artifact_bytes`] to fetch via `gloo-net` and
/// trigger a blob-URL download instead.
pub fn artifact_download_url(run_id: &str) -> String {
    format!("{}/reports/runs/{}/download", API_BASE, run_id)
}

// ── Schedules ────────────────────────────────────────────────────────────────

/// Lists all schedules attached to a report.
pub async fn list_schedules(
    token: &str,
    report_id: &str,
) -> Result<Vec<ReportScheduleView>, ApiError> {
    let path = format!("/reports/{}/schedules", report_id);
    client(token).get_json(&path).await
}

/// Lists all active schedules across all reports.
pub async fn list_all_schedules(token: &str) -> Result<Vec<ReportScheduleView>, ApiError> {
    client(token).get_json("/reports/schedules").await
}

/// Creates a new schedule for a report.
pub async fn create_schedule(
    token: &str,
    report_id: &str,
    input: &CreateScheduleInput,
) -> Result<ReportScheduleView, ApiError> {
    let path = format!("/reports/{}/schedules", report_id);
    client(token).post_json(&path, input).await
}

/// Deletes (disables) a schedule by id.
pub async fn delete_schedule(token: &str, schedule_id: &str) -> Result<(), ApiError> {
    let path = format!("/reports/schedules/{}", schedule_id);
    client(token).delete(&path).await
}
