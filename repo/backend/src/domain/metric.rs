use serde::{Serialize, Deserialize};
use chrono::NaiveDateTime;
use uuid::Uuid;

/// A configurable metric that can be tracked and displayed on dashboards.
///
/// Metric definitions describe *what* is measured and *how* it is computed;
/// actual metric data points are stored separately (e.g. in a time-series
/// table or an analytics pipeline).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDefinition {
    pub id: Uuid,

    /// Machine-readable key, e.g. "attendance_rate", "avg_journal_length".
    pub key: String,

    /// Human-friendly label shown in the UI.
    pub display_name: String,

    /// Unit of measurement, e.g. "percent", "count", "minutes".
    /// TODO: consider an enum of known units in phase 2.
    pub unit: Option<String>,

    /// Whether higher values are better ("higher_is_better") or the inverse.
    /// TODO: formalise polarity as an enum in phase 2.
    pub polarity: Option<String>,

    /// Pointer to the current (latest) version of the computation logic.
    pub current_version_id: Option<Uuid>,

    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

/// An immutable snapshot of a metric definition's computation logic.
///
/// Versioning allows dashboards to pin to a specific formula revision and
/// enables audit-safe comparisons over time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDefinitionVersion {
    pub id: Uuid,

    /// The metric definition this version belongs to.
    pub metric_definition_id: Uuid,

    /// Monotonically increasing version number (1-based).
    pub version_number: i32,

    /// SQL fragment, expression DSL, or reference to a computation function.
    /// TODO: decide on expression language / sandboxing approach in phase 2.
    pub formula: String,

    /// Free-text description of what this version computes and why it changed.
    pub description: Option<String>,

    /// Who authored this version.
    pub authored_by: Uuid,

    pub created_at: NaiveDateTime,
}
