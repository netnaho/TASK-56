//! Report domain models.
//!
//! Reports have a structured `ReportQueryDefinition` that describes *what*
//! data to generate and which filters to apply.  Report runs capture
//! the execution result and a path to the generated artifact file.
//! Report schedules hold cron expressions that the background scheduler
//! evaluates to trigger automatic runs.

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ─── Output formats ──────────────────────────────────────────────────────────

/// Supported report output formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportFormat {
    Csv,
    Xlsx,
}

impl ReportFormat {
    /// Return the canonical DB enum label for this format.
    ///
    /// # Exhaustiveness guard
    ///
    /// The match below is intentionally non-wildcard so that adding a new
    /// `ReportFormat` variant without updating `as_db` is a **compile error**.
    /// Do NOT add a `_ =>` arm here.  The legacy label `"excel"` must never
    /// be written back; only the canonical labels `"csv"` and `"xlsx"` are
    /// valid DB values after migration 018.
    pub fn as_db(self) -> &'static str {
        match self {
            ReportFormat::Csv => "csv",
            ReportFormat::Xlsx => "xlsx",
        }
    }

    pub fn from_db(s: &str) -> Option<Self> {
        match s {
            "csv" => Some(ReportFormat::Csv),
            "xlsx" => Some(ReportFormat::Xlsx),
            // Legacy DB values — kept for safe reads during and after migration 018.
            // "excel" is the old enum label for what is now "xlsx".
            "excel" => Some(ReportFormat::Xlsx),
            // "pdf", "html", "json" had no renderer; degrade to csv.
            "pdf" | "html" | "json" => Some(ReportFormat::Csv),
            _ => None,
        }
    }

    pub fn mime_type(self) -> &'static str {
        match self {
            ReportFormat::Csv => "text/csv; charset=utf-8",
            ReportFormat::Xlsx => {
                "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
            }
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            ReportFormat::Csv => "csv",
            ReportFormat::Xlsx => "xlsx",
        }
    }
}

// ─── Report type / query definition ──────────────────────────────────────────

/// The kind of report to generate.  Each variant maps to a different SQL
/// query executed at report-generation time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReportType {
    /// Journal catalog: all journals with latest version status.
    JournalCatalog,
    /// Teaching resource catalog: all resources with latest version status.
    ResourceCatalog,
    /// Course catalog: courses with section count.
    CourseCatalog,
    /// Check-in activity: check-in counts by section over a date range.
    CheckinActivity,
    /// Audit summary: action counts grouped by action over a date range.
    AuditSummary,
    /// Section roster: sections with instructor and capacity.
    SectionRoster,
}

/// Optional filters applied at report-generation time.
/// All fields are optional; absent filters mean "no restriction".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReportFilters {
    /// Restrict output to a single department UUID.  `null` = all departments.
    pub department_id: Option<Uuid>,
    /// Inclusive lower-bound on date fields (ISO-8601, e.g. "2024-01-01").
    pub date_from: Option<String>,
    /// Inclusive upper-bound on date fields.
    pub date_to: Option<String>,
    /// Filter by record status string (e.g. "published", "draft").
    pub status_filter: Option<String>,
}

/// Serialised into the `query_definition` JSON column of `reports`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReportQueryDefinition {
    pub report_type: ReportType,
    #[serde(default)]
    pub filters: ReportFilters,
}

// ─── Core entities ───────────────────────────────────────────────────────────

/// A named, reusable report definition.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub id: Uuid,
    pub title: String,
    pub description: Option<String>,
    pub query_definition: ReportQueryDefinition,
    pub default_format: ReportFormat,
    pub created_by: Option<Uuid>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// Run status lifecycle: queued → running → completed | failed | cancelled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl RunStatus {
    pub fn as_db(self) -> &'static str {
        match self {
            RunStatus::Queued => "queued",
            RunStatus::Running => "running",
            RunStatus::Completed => "completed",
            RunStatus::Failed => "failed",
            RunStatus::Cancelled => "cancelled",
        }
    }

    pub fn from_db(s: &str) -> Option<Self> {
        match s {
            "queued" => Some(RunStatus::Queued),
            "running" => Some(RunStatus::Running),
            "completed" => Some(RunStatus::Completed),
            "failed" => Some(RunStatus::Failed),
            "cancelled" => Some(RunStatus::Cancelled),
            _ => None,
        }
    }
}

/// What triggered a report run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggeredSource {
    Manual,
    Scheduled,
}

impl TriggeredSource {
    pub fn as_db(self) -> &'static str {
        match self {
            TriggeredSource::Manual => "manual",
            TriggeredSource::Scheduled => "scheduled",
        }
    }
    pub fn from_db(s: &str) -> Option<Self> {
        match s {
            "scheduled" => Some(TriggeredSource::Scheduled),
            _ => Some(TriggeredSource::Manual),
        }
    }
}

/// A single execution of a report definition.
#[derive(Debug, Clone, Serialize)]
pub struct ReportRun {
    pub id: Uuid,
    pub report_id: Uuid,
    pub triggered_by: Option<Uuid>,
    pub triggered_source: TriggeredSource,
    pub format: ReportFormat,
    pub status: RunStatus,
    /// Path relative to `reports_storage_path`, e.g. `"{run_id}.csv"`.
    pub artifact_path: Option<String>,
    pub artifact_size_bytes: Option<i64>,
    pub error_message: Option<String>,
    pub started_at: Option<NaiveDateTime>,
    pub completed_at: Option<NaiveDateTime>,
    pub created_at: NaiveDateTime,
}

/// A recurring schedule that automatically triggers a report.
#[derive(Debug, Clone, Serialize)]
pub struct ReportSchedule {
    pub id: Uuid,
    pub report_id: Uuid,
    /// 7-field cron expression (sec min hour dom month dow year).
    /// Example: `"0 0 7 * * Mon *"` — every Monday at 07:00 UTC.
    pub cron_expression: String,
    /// If set, restrict the scheduled run to this department.
    pub department_scope_id: Option<Uuid>,
    pub is_active: bool,
    pub format: ReportFormat,
    pub last_run_at: Option<NaiveDateTime>,
    pub next_run_at: Option<NaiveDateTime>,
    pub created_by: Option<Uuid>,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_format_roundtrip() {
        for (s, f) in [("csv", ReportFormat::Csv), ("xlsx", ReportFormat::Xlsx)] {
            assert_eq!(ReportFormat::from_db(s), Some(f));
            assert_eq!(f.as_db(), s);
        }
        // Legacy values degrade gracefully
        assert_eq!(ReportFormat::from_db("pdf"), Some(ReportFormat::Csv));
    }

    /// Regression guard: `as_db()` must never return legacy or unsupported
    /// labels.  After migration 018 the only valid DB values are "csv" and
    /// "xlsx".  Writing back "excel", "pdf", "html", or "json" would corrupt
    /// reads for any code that calls `from_db` on the stored value.
    #[test]
    fn report_format_as_db_never_uses_legacy_labels() {
        // Canonical outputs
        assert_eq!(ReportFormat::Csv.as_db(), "csv");
        assert_eq!(ReportFormat::Xlsx.as_db(), "xlsx");

        // as_db must NOT produce legacy or degraded labels
        assert_ne!(
            ReportFormat::Xlsx.as_db(),
            "excel",
            "as_db must never write back the legacy 'excel' label (use 'xlsx')"
        );
        assert_ne!(
            ReportFormat::Csv.as_db(),
            "pdf",
            "as_db must never write back 'pdf'"
        );
        assert_ne!(
            ReportFormat::Csv.as_db(),
            "html",
            "as_db must never write back 'html'"
        );
        assert_ne!(
            ReportFormat::Csv.as_db(),
            "json",
            "as_db must never write back 'json'"
        );
    }

    #[test]
    fn run_status_roundtrip() {
        for s in ["queued", "running", "completed", "failed", "cancelled"] {
            let st = RunStatus::from_db(s).unwrap();
            assert_eq!(st.as_db(), s);
        }
    }

    #[test]
    fn report_type_serde() {
        let qd = ReportQueryDefinition {
            report_type: ReportType::CheckinActivity,
            filters: ReportFilters {
                date_from: Some("2024-01-01".to_string()),
                ..Default::default()
            },
        };
        let json = serde_json::to_string(&qd).unwrap();
        let back: ReportQueryDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(back.report_type, ReportType::CheckinActivity);
    }
}
